//! Builds the firmware of one node from `nodes.toml`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use crate::config::{self, Board, Chip, Node, Radio, Source};
use crate::validate;

/// What cargo needs to know to build the firmware of one node.
struct Firmware {
    /// The chip of the node's board.
    chip: Chip,
    /// The radio chip of the node's board.
    radio: Radio,
    /// The node's sources.
    sources: Vec<Source>,
    /// `AIO_*` env vars with the node's values and its board's pins and settings.
    /// The firmware reads them with `env!`.
    env: BTreeMap<String, String>,
}

/// Builds the node's firmware and returns the path of its ELF file.
pub fn build(node_name: &str) -> Result<PathBuf, String> {
    let firmware = resolve(node_name)?;
    firmware.cargo("build")?;
    Ok(firmware.elf())
}

/// Builds the firmware of every node in `nodes.toml`, one after the other, and
/// stops at the first one that fails. Nodes on the same chip share one ELF path,
/// so each build overwrites the ELF of the one before.
pub fn build_all() -> Result<(), String> {
    let nodes = config::load_nodes()?;
    for node_name in nodes.nodes.keys() {
        println!("== {node_name}");
        build(node_name).map_err(|e| format!("{node_name}: {e}"))?;
    }
    println!("built {} nodes", nodes.nodes.len());
    Ok(())
}

/// Runs clippy on the node's firmware.
pub fn clippy(node_name: &str) -> Result<(), String> {
    resolve(node_name)?.cargo("clippy")
}

/// Builds the node's firmware, flashes it and shows the log.
pub fn run(node_name: &str, port: Option<&str>) -> Result<(), String> {
    let elf = build(node_name)?;
    wait(
        "espflash flash",
        espflash("flash", port).arg("--monitor").arg(&elf),
    )
}

/// Shows the log of a node that is already flashed. espflash needs the ELF file
/// to turn the defmt log back into text.
pub fn monitor(node_name: &str, port: Option<&str>) -> Result<(), String> {
    let elf = resolve(node_name)?.elf();
    if !elf.exists() {
        return Err(format!(
            "{} doesn't exist; run `cargo xtask build {node_name}` first",
            elf.display()
        ));
    }
    wait(
        "espflash monitor",
        espflash("monitor", port).arg("--elf").arg(&elf),
    )
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

    let mut env = node_env(node);
    env.extend(board_env(board));
    Ok(Firmware {
        chip: board.chip,
        radio: board.radio,
        sources: node.sources.keys().copied().collect(),
        env,
    })
}

/// The node's values from `nodes.toml`, as env vars for `firmware/node/src/config.rs`.
fn node_env(node: &Node) -> BTreeMap<String, String> {
    [
        ("AIO_NODE_ID", node.node_id.to_string()),
        ("AIO_HEARTBEAT", node.heartbeat.to_string()),
        ("AIO_MIN_INTERVAL", node.min_interval.to_string()),
    ]
    .into_iter()
    .map(|(var, value)| (var.to_string(), value))
    .collect()
}

/// The board's pins and settings from `boards.toml`, as env vars for
/// `firmware/node/src/config.rs`. `lora.sck = 6` becomes `AIO_PIN_LORA_SCK=6`,
/// `tcxo_mv = 1800` becomes `AIO_TCXO_MV=1800`. What the board doesn't have
/// (like `busy` on an SX127x board) isn't exported.
fn board_env(board: &Board) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = board
        .pins()
        .into_iter()
        .map(|(name, gpio)| {
            let var = format!("AIO_PIN_{}", name.replace('.', "_").to_uppercase());
            (var, gpio.to_string())
        })
        .collect();
    if let Some(mv) = board.tcxo_mv {
        env.insert("AIO_TCXO_MV".to_string(), mv.to_string());
    }
    env
}

impl Firmware {
    /// Runs `cargo <subcommand>` inside `firmware/node`, so cargo also reads
    /// `firmware/node/.cargo/config.toml`. Cargo rebuilds by itself when an
    /// `AIO_*` value changes.
    fn cargo(&self, subcommand: &str) -> Result<(), String> {
        wait(
            &format!("cargo {subcommand}"),
            self.chip
                .cargo(subcommand)
                .args(["--release", "--target", self.chip.target()])
                .args(["--features", &self.features().join(",")])
                .envs(&self.env)
                .current_dir(config::repo_root().join("firmware").join("node")),
        )
    }

    /// The cargo features that pick the chip, the radio and the sources.
    fn features(&self) -> Vec<&'static str> {
        let mut features = vec![self.chip.feature(), self.radio.feature()];
        features.extend(self.sources.iter().map(Source::feature));
        features
    }

    /// The file that `cargo build` writes. All crates of the workspace share the
    /// `target/` folder in the repo root.
    fn elf(&self) -> PathBuf {
        config::repo_root()
            .join("target")
            .join(self.chip.target())
            .join("release")
            .join("node")
    }
}

/// Starts an `espflash <subcommand>` command that decodes the defmt log.
/// Without a port, espflash looks for the board itself.
fn espflash(subcommand: &str, port: Option<&str>) -> Command {
    let mut command = Command::new("espflash");
    command.args([subcommand, "--log-format", "defmt"]);
    if let Some(port) = port {
        command.args(["--port", port]);
    }
    command
}

/// Runs `command` and waits until it ends. Its output goes straight to the terminal.
/// `name` is what error messages call it, for example `cargo build`.
fn wait(name: &str, command: &mut Command) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|e| format!("can't start {name}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{name} failed"))
    }
}

impl Chip {
    /// Starts `cargo <subcommand>` with the toolchain this chip needs.
    ///
    /// It starts `cargo` by name, not through the `CARGO` env var that
    /// `cargo xtask` sets: that one points straight to the stable cargo, which
    /// doesn't understand `+esp`.
    fn cargo(&self, subcommand: &str) -> Command {
        let mut command = Command::new("cargo");
        match self {
            Chip::Esp32c6 => command.arg(subcommand),
            // S3 only; delete this arm together with S3 support. Normal Rust can't
            // compile for the S3's Xtensa CPU, so it takes Espressif's `esp`
            // toolchain. That one has no ready-made `core` library for the S3, so
            // cargo builds it from source (`build-std`).
            Chip::Esp32s3 => command.args(["+esp", subcommand, "-Zbuild-std=core"]),
        };
        command
    }

    /// The compiler's name for the chip's CPU.
    fn target(&self) -> &'static str {
        match self {
            Chip::Esp32c6 => "riscv32imac-unknown-none-elf",
            Chip::Esp32s3 => "xtensa-esp32s3-none-elf",
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

impl Radio {
    /// The cargo feature that picks this radio chip in `firmware/node`.
    fn feature(&self) -> &'static str {
        match self {
            Radio::Sx126x => "sx126x",
            Radio::Sx127x => "sx127x",
        }
    }
}

impl Source {
    /// The cargo feature that turns this source on in `firmware/node`.
    fn feature(&self) -> &'static str {
        match self {
            Source::Ds18b20 => "source-ds18b20",
            Source::Battery => "source-battery",
            Source::Eder => "source-eder",
            Source::Pyromat => "source-pyromat",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_become_env_vars() {
        let board: Board = toml::from_str(
            r#"
            chip = "esp32c6"
            radio = "sx127x"
            lora = { sck = 6, mosi = 7, miso = 2, cs = 18, rst = 19, irq = 20 }
            "#,
        )
        .unwrap();
        let env = board_env(&board);
        assert_eq!(env["AIO_PIN_LORA_SCK"], "6");
        assert_eq!(env["AIO_PIN_LORA_IRQ"], "20");
        // An SX127x board has no `busy` pin and no TCXO, so they aren't exported.
        assert!(!env.contains_key("AIO_PIN_LORA_BUSY"));
        assert!(!env.contains_key("AIO_TCXO_MV"));
        assert_eq!(env.len(), 6);
    }

    #[test]
    fn tcxo_becomes_env_var() {
        let board: Board = toml::from_str(
            r#"
            chip = "esp32s3"
            radio = "sx126x"
            tcxo_mv = 1800
            lora = { sck = 5, mosi = 6, miso = 3, cs = 7, rst = 8, irq = 33, busy = 34 }
            "#,
        )
        .unwrap();
        assert_eq!(board_env(&board)["AIO_TCXO_MV"], "1800");
    }

    #[test]
    fn sources_become_features() {
        let firmware = Firmware {
            chip: Chip::Esp32c6,
            radio: Radio::Sx127x,
            sources: vec![Source::Battery, Source::Eder],
            env: BTreeMap::new(),
        };
        assert_eq!(
            firmware.features(),
            ["esp32c6", "sx127x", "source-battery", "source-eder"]
        );
    }
}
