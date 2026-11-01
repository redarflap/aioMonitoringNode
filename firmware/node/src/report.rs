//! The report task: the only code that sends over LoRa.
//!
//! It takes a snapshot of every source's latest section and asks the send
//! policy (`crates/policy`) whether it's worth a packet: a field changed by
//! more than its threshold (but not sooner than `min_interval` after the last
//! packet), or `heartbeat` has passed.

use defmt::{error, info};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, with_timeout};
use policy::{Decision, Policy};
use proto::{Packet, Section};

use crate::{config, radio};

/// A source signals this after it stores a new snapshot, to wake the report
/// task.
pub static NEW_DATA: Signal<CriticalSectionRawMutex, ()> = Signal::new();

const POLICY: Policy = Policy {
    min_interval_s: config::MIN_INTERVAL_S,
    heartbeat_s: config::HEARTBEAT_S,
};

/// Starts the radio, waits for the first sample, then sends whenever the
/// policy says so. Ends if the radio doesn't start, because then there is
/// nothing to do.
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

    // The first packet goes out right after the first sample. A signal from
    // before the radio was ready isn't lost: `wait` returns right away then.
    NEW_DATA.wait().await;

    // What the last packet carried; `None` until one went out. A failed send
    // keeps the old values, so the change goes out again after `min_interval`.
    let mut last_sent: Option<Snapshot> = None;
    // When the last packet went out, even if it failed. The heartbeat counts
    // from here.
    let mut last_send: Option<Instant> = None;
    loop {
        let snapshot = Snapshot::latest();
        let since_last_send_s =
            last_send.map(|at| u32::try_from(at.elapsed().as_secs()).unwrap_or(u32::MAX));
        // Nothing sent yet counts as a change, so a failed first packet is
        // tried again after `min_interval`.
        let changed = last_sent.as_ref().is_none_or(|last| snapshot.changed(last));
        match POLICY.decide(since_last_send_s, changed) {
            Decision::Send => {
                let reason = if last_sent.is_none() {
                    "first packet"
                } else if changed {
                    "change"
                } else {
                    "heartbeat"
                };
                last_send = Some(Instant::now());
                let packet = snapshot.packet();
                let bytes = packet.as_bytes();
                match radio.send(bytes).await {
                    Ok(()) => {
                        info!("sent ({=str}): {=[u8]:02x}", reason, bytes);
                        last_sent = Some(snapshot);
                    }
                    Err(e) => error!("send failed ({=str}): {}", reason, e),
                }
            }
            Decision::Wait(s) => {
                // Ends early when a source has new data; then ask again.
                let _ = with_timeout(Duration::from_secs(s.into()), NEW_DATA.wait()).await;
            }
        }
    }
}

/// The latest section of every source the node runs: what a packet carries.
struct Snapshot {
    #[cfg(feature = "source-battery")]
    battery: proto::Battery,
}

impl Snapshot {
    /// Reads each source's `LATEST` cell.
    fn latest() -> Self {
        Self {
            #[cfg(feature = "source-battery")]
            battery: crate::sources::battery::LATEST.lock(core::cell::Cell::get),
        }
    }

    /// Whether any field changed since `last` by more than its threshold.
    #[allow(
        unused_variables,
        reason = "a node whose sources have no section yet never changes"
    )]
    fn changed(&self, last: &Self) -> bool {
        #[cfg(feature = "source-battery")]
        if self
            .battery
            .changed(&last.battery, &config::BATTERY_THRESHOLDS)
        {
            return true;
        }
        false
    }

    /// The node ID, then one section per source the node runs.
    #[allow(
        unused_mut,
        reason = "a node whose sources have no section yet pushes nothing"
    )]
    fn packet(&self) -> Packet {
        let mut packet = Packet::new(config::NODE_ID);
        #[cfg(feature = "source-battery")]
        push(&mut packet, &self.battery);
        packet
    }
}

/// Appends `section` to `packet`. A section that doesn't fit is left out and
/// logged.
#[allow(
    dead_code,
    reason = "a node whose sources have no section yet pushes nothing"
)]
fn push<S: Section>(packet: &mut Packet, section: &S) {
    if packet.push(section).is_err() {
        error!("kind {} doesn't fit into the packet", S::KIND);
    }
}
