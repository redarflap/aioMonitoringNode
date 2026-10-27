//! The report task: the only code that sends over LoRa.
//!
//! For now it sends test bytes. Later it collects the sources' snapshots,
//! applies the send policy and sends the real packet.

use defmt::{error, info};
use embassy_time::{Duration, Timer};

use crate::{config, radio};

/// Starts the radio, then sends a packet every `HEARTBEAT_S` seconds.
/// Ends if the radio doesn't start, because then there is nothing to do.
#[embassy_executor::task]
#[allow(
    clippy::large_stack_frames,
    reason = "clippy counts lora-phy's state while it starts and sends as stack, \
    but that state lives in the task's future, which Embassy keeps in static memory"
)]
pub async fn task(hardware: radio::Hardware) {
    let mut radio = match radio::Radio::new(hardware).await {
        Ok(radio) => {
            info!("radio ready");
            radio
        }
        Err(e) => {
            error!("radio didn't start: {}", e);
            return;
        }
    };

    // Test bytes, until crates/proto defines the real packet.
    let packet = [config::NODE_ID, 0xAA, 0x55];
    loop {
        match radio.send(&packet).await {
            Ok(()) => info!("sent {=[u8]:02x}", packet),
            Err(e) => error!("send failed: {}", e),
        }
        Timer::after(Duration::from_secs(config::HEARTBEAT_S.into())).await;
    }
}
