//! The battery voltage of a battery-powered node.

use defmt::info;

use crate::config;

#[embassy_executor::task]
pub async fn task() {
    info!(
        "battery: GPIO {}, divider {}/1000, not ported yet",
        config::BATTERY_ADC,
        config::BATTERY_DIVIDER_PERMILLE
    );
}
