//! `schemas.json`: what the decoder needs to know about kinds and nodes.
//!
//! `cargo xtask schemas` writes it to the repo root. Never edit it by hand.

use std::collections::BTreeMap;
use std::path::PathBuf;

use proto::{Field, Type};
use serde::Serialize;

use crate::config::{self, NodesFile, Source};
use crate::validate;

/// The whole `schemas.json`. Maps are sorted by key, so the same config
/// always gives the same file.
#[derive(Serialize)]
struct Schemas {
    /// Kind number -> the source it belongs to and its fields.
    kinds: BTreeMap<u8, Kind>,
    /// Node ID -> node.
    nodes: BTreeMap<u8, Node>,
    /// Device key -> HA device from `[devices.*]`.
    devices: BTreeMap<String, Device>,
    /// First bytes of legacy packets. The decoder hands those packets to the
    /// legacy flow.
    reserved_node_ids: Vec<u8>,
}

/// One kind: the section of one source.
#[derive(Serialize)]
struct Kind {
    source: &'static str,
    /// In wire order.
    fields: Vec<FieldSchema>,
}

/// One field of a kind. Thresholds stay out; the decoder doesn't need them.
#[derive(Serialize)]
struct FieldSchema {
    key: &'static str,
    #[serde(rename = "type")]
    ty: &'static str,
    divisor: u32,
    // Left out when the field has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    unit: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_class: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state_class: Option<&'static str>,
}

/// One node from `nodes.toml`.
#[derive(Serialize)]
struct Node {
    name: String,
    /// Seconds; HA marks the node's sensors unavailable after 2.5 × this.
    heartbeat: u32,
    sources: Vec<&'static str>,
}

/// One `[devices.<key>]` entry.
#[derive(Serialize)]
struct Device {
    name: String,
    sources: Vec<DeviceSource>,
}

/// One source of a device: `dev-c6.battery` becomes `{ "node": 100, "source": "battery" }`.
#[derive(Serialize)]
struct DeviceSource {
    node: u8,
    source: &'static str,
}

/// Validates the config files, then writes `schemas.json`. Returns its path.
pub fn write() -> Result<PathBuf, String> {
    let boards = config::load_boards()?;
    let nodes = config::load_nodes()?;
    validate::run(&boards, &nodes)?;

    let path = config::repo_root().join("schemas.json");
    std::fs::write(&path, render(&nodes))
        .map_err(|e| format!("can't write {}: {e}", path.display()))?;
    Ok(path)
}

/// The text of `schemas.json`: pretty JSON with a trailing newline. `nodes`
/// must be valid.
fn render(nodes: &NodesFile) -> String {
    let schemas = Schemas {
        kinds: kinds(),
        nodes: nodes
            .nodes
            .iter()
            .map(|(name, node)| {
                let node_schema = Node {
                    name: name.clone(),
                    heartbeat: node.heartbeat,
                    sources: node.sources.keys().map(Source::name).collect(),
                };
                (node.node_id, node_schema)
            })
            .collect(),
        devices: nodes
            .devices
            .iter()
            .map(|(key, device)| {
                let sources = device
                    .sources
                    .iter()
                    .map(|entry| {
                        // `validate::run` has already checked every entry.
                        let (node, source) = nodes.resolve(entry).expect("valid entry");
                        DeviceSource {
                            node: node.node_id,
                            source: source.name(),
                        }
                    })
                    .collect();
                let device_schema = Device {
                    name: device.name.clone(),
                    sources,
                };
                (key.clone(), device_schema)
            })
            .collect(),
        reserved_node_ids: nodes.reserved_node_ids.clone(),
    };
    // Can't fail: every map key is a number, and every value plain data.
    let mut text = serde_json::to_string_pretty(&schemas).expect("serializable");
    text.push('\n');
    text
}

/// Every kind that a ported source has.
fn kinds() -> BTreeMap<u8, Kind> {
    let mut kinds = BTreeMap::new();
    for source in Source::ALL {
        // Sources that aren't ported yet have no kind yet.
        let Some(kind) = source.kind() else {
            continue;
        };
        let fields = source.fields().iter().map(field_schema).collect();
        kinds.insert(
            kind,
            Kind {
                source: source.name(),
                fields,
            },
        );
    }
    kinds
}

fn field_schema(field: &Field) -> FieldSchema {
    FieldSchema {
        key: field.key,
        ty: type_name(field.ty),
        divisor: field.divisor,
        unit: field.unit,
        device_class: field.device_class,
        state_class: field.state_class,
    }
}

/// The type's name in `schemas.json`, for example `u8`.
fn type_name(ty: Type) -> &'static str {
    match ty {
        Type::U8 => "u8",
        Type::I8 => "i8",
        Type::U16 => "u16",
        Type::I16 => "i16",
        Type::U32 => "u32",
        Type::I32 => "i32",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_nodes_toml() {
        let nodes: NodesFile = toml::from_str(
            r#"
            reserved_node_ids = [0x7C, 0x00]
            nodes.b = { board = "c6-dev", node_id = 101, power = "always-on", heartbeat = 300, min_interval = 10, sources.battery = {}, sources.eder = {} }
            nodes.a = { board = "c6-dev", node_id = 100, power = "always-on", heartbeat = 60, min_interval = 10, sources.battery = { thresholds = { voltage = 2 } } }
            devices.heating = { name = "Heating", sources = ["b.eder", "a.battery"] }
            "#,
        )
        .unwrap();
        // Nodes sorted by ID; eder has no kind yet, but a node can still run it.
        // Device sources keep their order from nodes.toml.
        let expected = r#"{
  "kinds": {
    "4": {
      "source": "battery",
      "fields": [
        {
          "key": "voltage",
          "type": "u8",
          "divisor": 50,
          "unit": "V",
          "device_class": "voltage",
          "state_class": "measurement"
        }
      ]
    }
  },
  "nodes": {
    "100": {
      "name": "a",
      "heartbeat": 60,
      "sources": [
        "battery"
      ]
    },
    "101": {
      "name": "b",
      "heartbeat": 300,
      "sources": [
        "battery",
        "eder"
      ]
    }
  },
  "devices": {
    "heating": {
      "name": "Heating",
      "sources": [
        {
          "node": 101,
          "source": "eder"
        },
        {
          "node": 100,
          "source": "battery"
        }
      ]
    }
  },
  "reserved_node_ids": [
    124,
    0
  ]
}
"#;
        assert_eq!(render(&nodes), expected);
    }

    #[test]
    fn empty_options_are_left_out() {
        let field = Field {
            key: "flags",
            ty: Type::U8,
            divisor: 1,
            unit: None,
            device_class: None,
            state_class: None,
            threshold: Some(0),
        };
        assert_eq!(
            serde_json::to_string(&field_schema(&field)).unwrap(),
            r#"{"key":"flags","type":"u8","divisor":1}"#
        );
    }
}
