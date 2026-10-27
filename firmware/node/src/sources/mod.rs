//! The sources: one module and one task per thing a node reads.
//! The `source-*` features pick the node's sources; the xtask takes them from
//! `sources` in nodes.toml.

#[cfg(feature = "source-battery")]
mod battery;
#[cfg(feature = "source-ds18b20")]
mod ds18b20;
#[cfg(feature = "source-eder")]
mod eder;
#[cfg(feature = "source-pyromat")]
mod pyromat;

use embassy_executor::Spawner;

/// Spawns the task of each source the node runs.
#[allow(
    unused_variables,
    reason = "a node without sources doesn't use the spawner"
)]
pub fn spawn_all(spawner: Spawner) {
    // Each task can run only once, so spawning fails only if this runs twice.
    #[cfg(feature = "source-battery")]
    spawner.spawn(battery::task().unwrap());
    #[cfg(feature = "source-ds18b20")]
    spawner.spawn(ds18b20::task().unwrap());
    #[cfg(feature = "source-eder")]
    spawner.spawn(eder::task().unwrap());
    #[cfg(feature = "source-pyromat")]
    spawner.spawn(pyromat::task().unwrap());
}
