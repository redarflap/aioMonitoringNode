//! The battery voltage of a battery-powered node.

use defmt::info;
use embassy_time::{Duration, Timer};

use crate::config;

/// The battery's ADC pin, ready to read. `board` builds it for the pin in
/// boards.toml.
pub trait BatteryAdc {
    /// The voltage at the pin in mV, not yet multiplied by the divider.
    fn read_mv(&mut self) -> u16;
}

/// For now, logs the pin voltage every `HEARTBEAT_S` seconds.
#[embassy_executor::task]
pub async fn task(adc: &'static mut dyn BatteryAdc) {
    info!("battery: divider {}/1000", config::BATTERY_DIVIDER_PERMILLE);
    loop {
        info!("battery: pin {} mV", adc.read_mv());
        Timer::after(Duration::from_secs(config::HEARTBEAT_S.into())).await;
    }
}
