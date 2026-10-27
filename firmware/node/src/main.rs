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

use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_println as _;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
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

    let mut radio = match radio::Radio::new(board.radio).await {
        Ok(radio) => {
            info!("radio ready");
            Some(radio)
        }
        Err(e) => {
            error!("radio didn't start: {}", e);
            None
        }
    };

    // TODO: Spawn some tasks
    let _ = spawner;

    // Test bytes, until crates/proto defines the real packet.
    let packet = [config::NODE_ID, 0xAA, 0x55];
    loop {
        if let Some(radio) = &mut radio {
            match radio.send(&packet).await {
                Ok(()) => info!("sent {=[u8]:02x}", packet),
                Err(e) => error!("send failed: {}", e),
            }
        }
        Timer::after(Duration::from_secs(config::HEARTBEAT_S.into())).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.2.2/examples
}
