//! SX126x radios (SX1262).

use embedded_hal_async::spi::SpiDevice;
use esp_hal::gpio::{Input, Output};
use lora_phy::iv::GenericSx126xInterfaceVariant;
use lora_phy::mod_params::RadioError;
use lora_phy::sx126x::{self, Sx126x, Sx1262, TcxoCtrlVoltage};

use super::{Error, LockedSpi, SharedSpi};
use crate::config;

const READ_REGISTER: u8 = 0x1D;
const REG_SYNC_WORD: u16 = 0x0740;
const PRIVATE_SYNC_WORD: u16 = 0x1424;

/// The TCXO voltage from boards.toml. `cargo xtask check` only lets through
/// the values listed here.
const TCXO: Option<TcxoCtrlVoltage> = match config::TCXO_MV {
    None => None,
    Some(1600) => Some(TcxoCtrlVoltage::Ctrl1V6),
    Some(1700) => Some(TcxoCtrlVoltage::Ctrl1V7),
    Some(1800) => Some(TcxoCtrlVoltage::Ctrl1V8),
    Some(2200) => Some(TcxoCtrlVoltage::Ctrl2V2),
    Some(2400) => Some(TcxoCtrlVoltage::Ctrl2V4),
    Some(2700) => Some(TcxoCtrlVoltage::Ctrl2V7),
    Some(3000) => Some(TcxoCtrlVoltage::Ctrl3V0),
    Some(3300) => Some(TcxoCtrlVoltage::Ctrl3V3),
    Some(_) => panic!("tcxo_mv in boards.toml isn't a voltage lora-phy supports"),
};

/// The radio's pins, apart from SPI.
pub struct Pins {
    /// Low resets the radio.
    pub reset: Output<'static>,
    /// Goes high when the radio is done.
    pub dio1: Input<'static>,
    /// High while the radio is busy.
    pub busy: Input<'static>,
}

/// The lora-phy driver for this radio family.
pub type Kind =
    Sx126x<LockedSpi, GenericSx126xInterfaceVariant<Output<'static>, Input<'static>>, Sx1262>;

/// Builds the lora-phy driver for an SX1262. Its DIO2 switches the antenna
/// (the `Sx1262` default), its DIO3 powers the TCXO, and it runs on its LDO
/// regulator.
pub fn kind(spi: &'static SharedSpi, pins: Pins) -> Result<Kind, RadioError> {
    let iv = GenericSx126xInterfaceVariant::new(pins.reset, pins.dio1, pins.busy, None, None)?;
    let config = sx126x::Config {
        chip: Sx1262,
        tcxo_ctrl: TCXO,
        use_dcdc: false, // LDO
        rx_boost: false,
    };
    Ok(Sx126x::new(LockedSpi(spi), iv, config))
}

/// Reads back the sync word that lora-phy has just written.
/// With wrong SPI or CS pins, something else comes back.
pub async fn check_sync_word(spi: &SharedSpi) -> Result<(), Error> {
    // The command, the 2-byte address and a dummy byte, then one dummy byte for
    // each byte to read. The radio answers during those last dummy bytes.
    let [high, low] = REG_SYNC_WORD.to_be_bytes();
    let mut bytes = [READ_REGISTER, high, low, 0x00, 0x00, 0x00];
    spi.lock()
        .await
        .transfer_in_place(&mut bytes)
        .await
        .map_err(|_| RadioError::SPI)?;
    if u16::from_be_bytes([bytes[4], bytes[5]]) == PRIVATE_SYNC_WORD {
        Ok(())
    } else {
        Err(Error::NoAnswer)
    }
}
