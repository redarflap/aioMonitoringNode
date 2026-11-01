//! The battery voltage of a battery-powered node.

use core::cell::Cell;

use defmt::info;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::{Duration, Timer};
use proto::Battery;

use crate::{config, report};

/// How many readings one sample averages, to smooth out the ADC's noise.
const READINGS: u32 = 8;

/// The battery's ADC pin, ready to read. `board` builds it for the pin in
/// boards.toml.
pub trait BatteryAdc {
    /// The voltage at the pin in mV, not yet multiplied by the divider.
    fn read_mv(&mut self) -> u16;
}

/// The latest sample, for the report task. `NOT_MEASURED` until the first one.
pub static LATEST: Mutex<CriticalSectionRawMutex, Cell<Battery>> =
    Mutex::new(Cell::new(Battery::NOT_MEASURED));

/// Samples the battery every `HEARTBEAT_S` seconds, stores the result in
/// `LATEST` and wakes the report task.
#[embassy_executor::task]
pub async fn task(adc: &'static mut dyn BatteryAdc) {
    info!("battery: divider {}/1000", config::BATTERY_DIVIDER_PERMILLE);
    loop {
        let pin_mv = average_mv(adc);
        let battery_mv = behind_divider(pin_mv);
        info!("battery: {} mV (pin {} mV)", battery_mv, pin_mv);
        LATEST.lock(|latest| latest.set(Battery::from_millivolts(battery_mv)));
        report::NEW_DATA.signal(());
        Timer::after(Duration::from_secs(config::HEARTBEAT_S.into())).await;
    }
}

/// The pin voltage in mV, averaged over `READINGS` readings and rounded.
fn average_mv(adc: &mut dyn BatteryAdc) -> u16 {
    let sum: u32 = (0..READINGS).map(|_| u32::from(adc.read_mv())).sum();
    // The average of u16 values always fits into a u16.
    ((sum + READINGS / 2) / READINGS) as u16
}

/// The battery voltage in mV for `pin_mv` at the pin, rounded. Saturates at
/// `u16::MAX`, which `Battery` clamps to 5.08 V anyway.
fn behind_divider(pin_mv: u16) -> u16 {
    // u64, so the multiplication can't overflow, even for a large divider.
    let mv = (u64::from(pin_mv) * u64::from(config::BATTERY_DIVIDER_PERMILLE) + 500) / 1000;
    u16::try_from(mv).unwrap_or(u16::MAX)
}
