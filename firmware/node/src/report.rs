//! The report task: the only code that sends over LoRa.
//!
//! It sends a packet with every source's latest snapshot each time a source
//! has new data. The send policy (step 2 of plan.md) comes later.

use defmt::{error, info};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use proto::{Packet, Section};

use crate::{config, radio};

/// A source signals this after it stores a new snapshot, to wake the report
/// task.
pub static NEW_DATA: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Starts the radio, then sends a packet each time a source signals
/// `NEW_DATA`. Ends if the radio doesn't start, because then there is nothing
/// to do.
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

    loop {
        // A signal from before the radio was ready isn't lost: `wait` returns
        // right away then.
        NEW_DATA.wait().await;
        let packet = build_packet();
        let bytes = packet.as_bytes();
        match radio.send(bytes).await {
            Ok(()) => info!("sent {=[u8]:02x}", bytes),
            Err(e) => error!("send failed: {}", e),
        }
    }
}

/// The node ID, then one section per source the node runs, each with the
/// source's latest snapshot.
#[allow(
    unused_mut,
    reason = "a node whose sources have no section yet pushes nothing"
)]
fn build_packet() -> Packet {
    let mut packet = Packet::new(config::NODE_ID);
    #[cfg(feature = "source-battery")]
    push(
        &mut packet,
        crate::sources::battery::LATEST.lock(core::cell::Cell::get),
    );
    packet
}

/// Appends `section` to `packet`. A section that doesn't fit is left out and
/// logged.
#[allow(
    dead_code,
    reason = "a node whose sources have no section yet pushes nothing"
)]
fn push<S: Section>(packet: &mut Packet, section: S) {
    if packet.push(&section).is_err() {
        error!("kind {} doesn't fit into the packet", S::KIND);
    }
}
