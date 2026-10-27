//! The config files in the repo root: `boards.toml` and `nodes.toml`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The whole `boards.toml` file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoardsFile {
    boards: BTreeMap<String, Board>,
}

/// One `[boards.<name>]` entry.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Board {
    pub chip: Chip,
    pub radio: Radio,
    pub lora: LoraPins,
}

impl Board {
    /// Every pin of the board, as (name, GPIO number), for example ("lora.sck", 6).
    /// Add new pin groups (1-Wire, UART, ...) here, so the pin rules check them too.
    pub fn pins(&self) -> Vec<(String, u8)> {
        let mut pins = Vec::new();
        for (name, gpio) in self.lora.pins() {
            pins.push((format!("lora.{name}"), gpio));
        }
        pins
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Chip {
    Esp32c6,
    Esp32s3,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Radio {
    Sx126x,
    Sx127x,
}

/// GPIO numbers of the LoRa radio.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoraPins {
    pub sck: u8,
    pub mosi: u8,
    pub miso: u8,
    pub cs: u8,
    pub rst: u8,
    /// DIO1 on SX126x, DIO0 on SX127x. SX127x boards may leave it out; the
    /// firmware then polls the radio over SPI instead.
    pub irq: Option<u8>,
    /// Only SX126x radios have a BUSY pin.
    pub busy: Option<u8>,
}

impl LoraPins {
    /// Every pin the board has, as (name, GPIO number).
    pub fn pins(&self) -> Vec<(&'static str, u8)> {
        let mut pins = vec![
            ("sck", self.sck),
            ("mosi", self.mosi),
            ("miso", self.miso),
            ("cs", self.cs),
            ("rst", self.rst),
        ];
        if let Some(irq) = self.irq {
            pins.push(("irq", irq));
        }
        if let Some(busy) = self.busy {
            pins.push(("busy", busy));
        }
        pins
    }
}

/// The whole `nodes.toml` file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodesFile {
    /// First bytes that legacy nodes still send. No node may use them.
    pub reserved_node_ids: Vec<u8>,
    /// Node name -> node, sorted by name.
    pub nodes: BTreeMap<String, Node>,
}

/// One `[nodes.<name>]` entry.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    /// Name of an entry in `boards.toml`.
    pub board: String,
    /// First byte of every packet this node sends.
    pub node_id: u8,
    pub power: Power,
    /// Seconds between packets when nothing changes.
    pub heartbeat: u32,
    /// Fewest seconds between two packets.
    pub min_interval: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Power {
    AlwaysOn,
    DeepSleep,
}

/// Reads `boards.toml`. Returns board name -> board, sorted by name.
pub fn load_boards() -> Result<BTreeMap<String, Board>, String> {
    let file: BoardsFile = load("boards.toml")?;
    Ok(file.boards)
}

/// Reads `nodes.toml`.
pub fn load_nodes() -> Result<NodesFile, String> {
    load("nodes.toml")
}

/// Reads one TOML file from the repo root and turns it into `T`.
fn load<T: for<'de> Deserialize<'de>>(name: &str) -> Result<T, String> {
    let path = repo_root().join(name);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("can't read {}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| format!("in {name}:\n{e}"))
}

/// The repo root is the folder that holds `xtask/`.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ is inside the repo")
        .to_path_buf()
}
