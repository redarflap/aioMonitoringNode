#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

mod board;
mod config;
mod radio;
mod report;

use defmt::info;
use embassy_executor::Spawner;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_println as _;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

/// Starts the board and the tasks, then returns. The tasks keep running.
#[esp_rtos::main]
async fn main(spawner: Spawner) {
    // generator version: 1.4.0
    // generator parameters: -o esp32c6 -o unstable-hal -o embassy -o defmt -o esp-backtrace (-o esp32s3 generates the same code)

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let board = board::init(peripherals);

    info!("Embassy initialized!");
    info!(
        "node_id {}, heartbeat {} s, min_interval {} s",
        config::NODE_ID,
        config::HEARTBEAT_S,
        config::MIN_INTERVAL_S
    );

    // Each task can run only once, so spawning fails only if this runs twice.
    spawner.spawn(report::task(board.radio).unwrap());

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.2.2/examples
}
