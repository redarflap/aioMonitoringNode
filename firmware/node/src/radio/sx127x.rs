//! SX127x radios (SX1276), on boards with or without DIO0 wired.

use embassy_time::{Duration, Timer};
use embedded_hal::digital::{ErrorKind, ErrorType};
use embedded_hal_async::digital::Wait;
use embedded_hal_async::spi::SpiDevice;
use esp_hal::gpio::{Input, Output};
use lora_phy::iv::GenericSx127xInterfaceVariant;
use lora_phy::mod_params::RadioError;
use lora_phy::sx127x::{self, Sx127x, Sx1276};

use super::{Error, LockedSpi, SharedSpi};

/// Register with one bit for each event the radio reports, like "sent".
const REG_IRQ_FLAGS: u8 = 0x12;
const REG_SYNC_WORD: u8 = 0x39;
const PRIVATE_SYNC_WORD: u8 = 0x12;
/// How often the stand-in for DIO0 reads the IRQ flags.
const POLL_INTERVAL: Duration = Duration::from_millis(1);

/// The radio's pins, apart from SPI.
pub struct Pins {
    /// Low resets the radio.
    pub reset: Output<'static>,
    /// Goes high when the radio is done. `None` when the board doesn't wire it.
    pub dio0: Option<Input<'static>>,
}

/// The lora-phy driver for this radio family.
pub type Kind = Sx127x<LockedSpi, GenericSx127xInterfaceVariant<Output<'static>, Dio0>, Sx1276>;

/// Builds the lora-phy driver for an SX1276 that sends through its PA_BOOST pin.
pub fn kind(spi: &'static SharedSpi, pins: Pins) -> Result<Kind, RadioError> {
    let dio0 = match pins.dio0 {
        Some(pin) => Dio0::Pin(pin),
        None => Dio0::Poll(spi),
    };
    let iv = GenericSx127xInterfaceVariant::new(pins.reset, dio0, None, None)?;
    let config = sx127x::Config {
        chip: Sx1276,
        tcxo_used: false,
        tx_boost: true, // PA_BOOST
        rx_boost: false,
    };
    Ok(Sx127x::new(LockedSpi(spi), iv, config))
}

/// Reads back the sync word that lora-phy has just written.
/// With wrong SPI or CS pins, something else comes back.
pub async fn check_sync_word(spi: &SharedSpi) -> Result<(), Error> {
    if read_register(spi, REG_SYNC_WORD).await? == PRIVATE_SYNC_WORD {
        Ok(())
    } else {
        Err(Error::NoAnswer)
    }
}

/// Sends the register's address and a dummy byte. The radio answers with the
/// value during the dummy byte.
async fn read_register(spi: &SharedSpi, register: u8) -> Result<u8, RadioError> {
    let mut bytes = [register, 0x00];
    spi.lock()
        .await
        .transfer_in_place(&mut bytes)
        .await
        .map_err(|_| RadioError::SPI)?;
    Ok(bytes[1])
}

/// What lora-phy waits on until the radio is done.
pub enum Dio0 {
    /// The board wires DIO0 to a GPIO.
    Pin(Input<'static>),
    /// The board doesn't wire DIO0, so read the radio's IRQ flags over SPI
    /// instead. lora-phy masks every flag except the one it waits for, so any
    /// set flag means the same as DIO0 going high.
    Poll(&'static SharedSpi),
}

impl ErrorType for Dio0 {
    type Error = ErrorKind;
}

impl Wait for Dio0 {
    async fn wait_for_high(&mut self) -> Result<(), ErrorKind> {
        match self {
            Dio0::Pin(pin) => {
                pin.wait_for_high().await;
                Ok(())
            }
            // Wait for the flag here, instead of telling lora-phy to check every
            // few ms: lora-phy reads the flags and then clears all of them, so a
            // flag that arrived between those two steps would be lost.
            Dio0::Poll(spi) => loop {
                let flags = read_register(spi, REG_IRQ_FLAGS)
                    .await
                    .map_err(|_| ErrorKind::Other)?;
                if flags != 0 {
                    return Ok(());
                }
                Timer::after(POLL_INTERVAL).await;
            },
        }
    }

    // lora-phy only ever calls `wait_for_high`.
    async fn wait_for_low(&mut self) -> Result<(), ErrorKind> {
        unimplemented!()
    }

    async fn wait_for_rising_edge(&mut self) -> Result<(), ErrorKind> {
        unimplemented!()
    }

    async fn wait_for_falling_edge(&mut self) -> Result<(), ErrorKind> {
        unimplemented!()
    }

    async fn wait_for_any_edge(&mut self) -> Result<(), ErrorKind> {
        unimplemented!()
    }
}
