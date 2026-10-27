//! The sources: one module and one task per thing a node reads.
//! The `source-*` features pick the node's sources; the xtask takes them from
//! `sources` in nodes.toml.

#[cfg(feature = "source-battery")]
pub mod battery;
#[cfg(feature = "source-ds18b20")]
mod ds18b20;
#[cfg(feature = "source-eder")]
mod eder;
#[cfg(feature = "source-pyromat")]
mod pyromat;

use embassy_executor::Spawner;

/// What the board hands over for the sources, ready to use.
pub struct Hardware {
    #[cfg(feature = "source-battery")]
    pub battery: &'static mut dyn battery::BatteryAdc,
}

/// Spawns the task of each source the node runs.
#[allow(
    unused_variables,
    reason = "a node without sources uses neither the spawner nor the hardware"
)]
pub fn spawn_all(spawner: Spawner, hardware: Hardware) {
    // Each task can run only once, so spawning fails only if this runs twice.
    #[cfg(feature = "source-battery")]
    spawner.spawn(battery::task(hardware.battery).unwrap());
    #[cfg(feature = "source-ds18b20")]
    spawner.spawn(ds18b20::task().unwrap());
    #[cfg(feature = "source-eder")]
    spawner.spawn(eder::task().unwrap());
    #[cfg(feature = "source-pyromat")]
    spawner.spawn(pyromat::task().unwrap());
}
