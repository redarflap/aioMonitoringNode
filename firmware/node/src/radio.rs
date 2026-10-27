//! The LoRa radio, driven by `lora-phy` with the settings from "Radio" in ARCHITECTURE.md.
//!
//! What differs between the radio families is in `radio/sx126x.rs` and
//! `radio/sx127x.rs`. The `sx126x` or `sx127x` feature picks one.

#[cfg(not(any(feature = "sx126x", feature = "sx127x")))]
compile_error!("pick the radio with the feature `sx126x` or `sx127x`; `cargo xtask` does that");
#[cfg(all(feature = "sx126x", feature = "sx127x"))]
compile_error!("pick only one of the features `sx126x` and `sx127x`");

#[cfg(feature = "sx126x")]
mod sx126x;
#[cfg(feature = "sx127x")]
mod sx127x;

// `family` is whichever radio family the feature picked.
#[cfg(feature = "sx126x")]
use sx126x as family;
#[cfg(feature = "sx127x")]
use sx127x as family;

pub use family::Pins;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Delay, Duration, with_timeout};
use embedded_hal_async::spi::{ErrorType, Operation, SpiDevice};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::Async;
use esp_hal::gpio::Output;
use esp_hal::spi::master::Spi;
use lora_phy::LoRa;
use lora_phy::mod_params::{
    Bandwidth, CodingRate, ModulationParams, PacketParams, RadioError, SpreadingFactor,
};
use static_cell::StaticCell;

// The settings of both radio families. They match OpenMQTTGateway's LoRa
// defaults, so change them only together with the gateway.
const FREQUENCY_HZ: u32 = 868_000_000;
const SPREADING_FACTOR: SpreadingFactor = SpreadingFactor::_7;
const BANDWIDTH: Bandwidth = Bandwidth::_125KHz;
const CODING_RATE: CodingRate = CodingRate::_4_5;
const PREAMBLE_SYMBOLS: u16 = 8;
/// A private network uses the sync word 0x12 on SX127x, which is 0x1424 on SX126x.
const PUBLIC_NETWORK: bool = false;
/// 14 dBm stays within the 25 mW ERP that 868.0–868.6 MHz allows.
#[expect(dead_code, reason = "`send` uses it from step 6.2 on")]
const TX_POWER_DBM: i32 = 14;

/// A working radio starts well within this time. Without a limit, a radio that
/// doesn't answer could keep start-up waiting forever.
const START_TIMEOUT: Duration = Duration::from_secs(2);

/// The SPI bus together with the radio's CS pin.
pub type RadioSpi = ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>;

/// The radio's SPI device behind a lock, so code other than lora-phy can use it too.
type SharedSpi = Mutex<CriticalSectionRawMutex, RadioSpi>;

/// What the board hands over for the radio, ready to use.
pub struct Hardware {
    pub spi: RadioSpi,
    pub pins: Pins,
}

/// Why the radio didn't start.
#[derive(defmt::Format)]
pub enum Error {
    /// lora-phy reported an error.
    Driver(RadioError),
    /// The radio wasn't ready after 2 s. On SX126x, a wrong BUSY pin does that.
    Timeout,
    /// The radio didn't read back the sync word. Usually a wrong SPI or CS pin.
    NoAnswer,
}

impl From<RadioError> for Error {
    fn from(error: RadioError) -> Self {
        Error::Driver(error)
    }
}

/// The LoRa radio, started and set up.
#[expect(dead_code, reason = "`send` reads the fields from step 6.2 on")]
pub struct Radio {
    lora: LoRa<family::Kind, Delay>,
    modulation: ModulationParams,
    tx_params: PacketParams,
}

impl Radio {
    /// Resets the radio, sets it up and checks that it answers.
    /// The board has one radio, so this can only be called once.
    #[allow(
        clippy::large_stack_frames,
        reason = "lora-phy's start-up code is large; it runs once, on the main stack, \
        which gets all the RAM that is left over"
    )]
    pub async fn new(hardware: Hardware) -> Result<Self, Error> {
        match with_timeout(START_TIMEOUT, Self::start(hardware)).await {
            Ok(result) => result,
            Err(_) => Err(Error::Timeout),
        }
    }

    #[allow(clippy::large_stack_frames, reason = "see `new`")]
    async fn start(hardware: Hardware) -> Result<Self, Error> {
        // `static` gives the lock a place that lives as long as the program, so
        // lora-phy and the code in `family` can both hold a reference to it.
        static SPI: StaticCell<SharedSpi> = StaticCell::new();
        let spi: &'static SharedSpi = SPI.init(Mutex::new(hardware.spi));

        let kind = family::kind(spi, hardware.pins)?;
        let mut lora = LoRa::new(kind, PUBLIC_NETWORK, Delay).await?;
        family::check_sync_word(spi).await?;

        let modulation =
            lora.create_modulation_params(SPREADING_FACTOR, BANDWIDTH, CODING_RATE, FREQUENCY_HZ)?;
        let implicit_header = false;
        let crc_on = true;
        let iq_inverted = false;
        let tx_params = lora.create_tx_packet_params(
            PREAMBLE_SYMBOLS,
            implicit_header,
            crc_on,
            iq_inverted,
            &modulation,
        )?;
        Ok(Radio {
            lora,
            modulation,
            tx_params,
        })
    }
}

/// Hands lora-phy the SPI device. It locks the device for each transaction,
/// so others can use the device in between.
struct LockedSpi(&'static SharedSpi);

impl ErrorType for LockedSpi {
    type Error = <RadioSpi as ErrorType>::Error;
}

impl SpiDevice for LockedSpi {
    async fn transaction(
        &mut self,
        operations: &mut [Operation<'_, u8>],
    ) -> Result<(), Self::Error> {
        self.0.lock().await.transaction(operations).await
    }
}
