//! Builds the firmware of one node from `nodes.toml`.

use std::path::PathBuf;
use std::process::Command;

use crate::config::{self, Chip};
use crate::validate;

/// What cargo needs to know to build the firmware of one node.
struct Firmware {
    /// The compiler's name for the chip's CPU, for example `riscv32imac-unknown-none-elf`.
    target: &'static str,
    /// The cargo feature that picks the chip in `firmware/node`.
    chip_feature: &'static str,
}

/// Builds the node's firmware and returns the path of its ELF file.
pub fn build(node_name: &str) -> Result<PathBuf, String> {
    let firmware = resolve(node_name)?;
    firmware.cargo("build")?;
    Ok(firmware.elf())
}

/// Runs clippy on the node's firmware.
pub fn clippy(node_name: &str) -> Result<(), String> {
    resolve(node_name)?.cargo("clippy")
}

/// Validates the config files, then looks up the node and its board.
fn resolve(node_name: &str) -> Result<Firmware, String> {
    let boards = config::load_boards()?;
    let nodes = config::load_nodes()?;
    validate::run(&boards, &nodes)?;

    let Some(node) = nodes.nodes.get(node_name) else {
        let names: Vec<&str> = nodes.nodes.keys().map(String::as_str).collect();
        return Err(format!(
            "node `{node_name}` isn't in nodes.toml; the nodes are: {}",
            names.join(", ")
        ));
    };
    // `validate::run` has already checked that the board exists.
    let board = &boards[&node.board];

    Ok(Firmware {
        target: board.chip.target()?,
        chip_feature: board.chip.feature(),
    })
}

impl Firmware {
    /// Runs `cargo <subcommand>` inside `firmware/node`, so cargo also reads
    /// `firmware/node/.cargo/config.toml`.
    fn cargo(&self, subcommand: &str) -> Result<(), String> {
        let status = Command::new("cargo")
            .args([subcommand, "--release", "--target", self.target])
            .args(["--features", self.chip_feature])
            .current_dir(config::repo_root().join("firmware").join("node"))
            .status()
            .map_err(|e| format!("can't start cargo: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("cargo {subcommand} failed"))
        }
    }

    /// The file that `cargo build` writes. All crates of the workspace share the
    /// `target/` folder in the repo root.
    fn elf(&self) -> PathBuf {
        config::repo_root()
            .join("target")
            .join(self.target)
            .join("release")
            .join("node")
    }
}

impl Chip {
    /// The compiler's name for the chip's CPU.
    fn target(&self) -> Result<&'static str, String> {
        match self {
            Chip::Esp32c6 => Ok("riscv32imac-unknown-none-elf"),
            Chip::Esp32s3 => Err("S3 nodes can't be built yet".to_string()),
        }
    }

    /// The cargo feature that picks this chip in `firmware/node`.
    fn feature(&self) -> &'static str {
        match self {
            Chip::Esp32c6 => "esp32c6",
            Chip::Esp32s3 => "esp32s3",
        }
    }
}
