//! Rules that `boards.toml` and `nodes.toml` must follow.
//!
//! Each rule is a small function that returns its problems as messages.
//! An empty list means the rule passed.

use std::collections::BTreeMap;

use crate::config::{Board, Chip, NodesFile, Radio};

/// Runs every rule and prints what they find. Warnings are only printed, errors fail the check.
pub fn run(boards: &BTreeMap<String, Board>, nodes: &NodesFile) -> Result<(), String> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    errors.extend(boards_exist(boards, nodes));
    errors.extend(unique_node_ids(nodes));
    errors.extend(no_reserved_node_ids(nodes));
    for (name, board) in boards {
        errors.extend(pins_exist(name, board));
        errors.extend(no_pin_twice(name, board));
        errors.extend(no_flash_pins(name, board));
        errors.extend(busy_matches_radio(name, board));
        errors.extend(irq_on_sx126x(name, board));
        warnings.extend(strapping_pins(name, board));
    }

    for warning in &warnings {
        eprintln!("warning: {warning}");
    }
    for error in &errors {
        eprintln!("error: {error}");
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} error(s) in boards.toml / nodes.toml",
            errors.len()
        ))
    }
}

/// Every node's `board` is an entry in `boards.toml`.
fn boards_exist(boards: &BTreeMap<String, Board>, nodes: &NodesFile) -> Vec<String> {
    let mut errors = Vec::new();
    for (name, node) in &nodes.nodes {
        if !boards.contains_key(&node.board) {
            errors.push(format!(
                "node `{name}`: board `{}` isn't in boards.toml",
                node.board
            ));
        }
    }
    errors
}

/// No two nodes share a `node_id`.
fn unique_node_ids(nodes: &NodesFile) -> Vec<String> {
    let mut errors = Vec::new();
    // node_id -> name of the node that uses it
    let mut seen: BTreeMap<u8, &str> = BTreeMap::new();
    for (name, node) in &nodes.nodes {
        // `insert` hands back the old entry, if there was one.
        if let Some(other) = seen.insert(node.node_id, name) {
            errors.push(format!(
                "nodes `{other}` and `{name}` both use node_id {}",
                node.node_id
            ));
        }
    }
    errors
}

/// No node uses a `node_id` that a legacy node still sends.
fn no_reserved_node_ids(nodes: &NodesFile) -> Vec<String> {
    let mut errors = Vec::new();
    for (name, node) in &nodes.nodes {
        let id = node.node_id;
        if nodes.reserved_node_ids.contains(&id) {
            errors.push(format!(
                "node `{name}`: node_id {id} ({id:#04x}) is in reserved_node_ids"
            ));
        }
    }
    errors
}

/// Every pin exists on the board's chip.
fn pins_exist(name: &str, board: &Board) -> Vec<String> {
    let mut errors = Vec::new();
    for (pin, gpio) in board.pins() {
        if !board.chip.has_gpio(gpio) {
            errors.push(format!(
                "board `{name}`: {pin} is GPIO {gpio}, which the {:?} doesn't have",
                board.chip
            ));
        }
    }
    errors
}

/// No GPIO is used by two pins of the same board.
fn no_pin_twice(name: &str, board: &Board) -> Vec<String> {
    let mut errors = Vec::new();
    // GPIO -> name of the pin that uses it
    let mut seen: BTreeMap<u8, String> = BTreeMap::new();
    for (pin, gpio) in board.pins() {
        if let Some(other) = seen.insert(gpio, pin.clone()) {
            errors.push(format!(
                "board `{name}`: {other} and {pin} both use GPIO {gpio}"
            ));
        }
    }
    errors
}

/// No pin is wired to the chip's SPI flash.
fn no_flash_pins(name: &str, board: &Board) -> Vec<String> {
    let mut errors = Vec::new();
    for (pin, gpio) in board.pins() {
        if board.chip.is_flash_pin(gpio) {
            errors.push(format!(
                "board `{name}`: {pin} is GPIO {gpio}, which the flash uses"
            ));
        }
    }
    errors
}

/// Returns warnings, not errors: strapping pins work, but whatever is wired to them
/// must not pull them to the wrong level at reset.
fn strapping_pins(name: &str, board: &Board) -> Vec<String> {
    let mut warnings = Vec::new();
    for (pin, gpio) in board.pins() {
        if board.chip.is_strapping_pin(gpio) {
            warnings.push(format!(
                "board `{name}`: {pin} is GPIO {gpio}, a strapping pin; \
                 make sure the hardware leaves it at the right level during reset"
            ));
        }
    }
    warnings
}

/// SX126x radios have a BUSY pin, SX127x radios don't.
fn busy_matches_radio(name: &str, board: &Board) -> Vec<String> {
    match (&board.radio, board.lora.busy) {
        (Radio::Sx126x, None) => vec![format!("board `{name}`: sx126x radios need lora.busy")],
        (Radio::Sx127x, Some(_)) => vec![format!(
            "board `{name}`: sx127x radios have no BUSY pin, remove lora.busy"
        )],
        _ => Vec::new(),
    }
}

/// SX126x boards must wire the IRQ pin. SX127x boards may leave it out,
/// because the firmware can poll an SX127x radio over SPI instead.
fn irq_on_sx126x(name: &str, board: &Board) -> Vec<String> {
    match (&board.radio, board.lora.irq) {
        (Radio::Sx126x, None) => vec![format!(
            "board `{name}`: sx126x radios need lora.irq (DIO1)"
        )],
        _ => Vec::new(),
    }
}

impl Chip {
    /// Does the chip have this GPIO?
    fn has_gpio(&self, gpio: u8) -> bool {
        match self {
            Chip::Esp32c6 => gpio <= 30,
            Chip::Esp32s3 => gpio <= 21 || (26..=48).contains(&gpio),
        }
    }

    /// Is this GPIO wired to the SPI flash?
    ///
    /// S3 modules with octal PSRAM also lose GPIO 33–37. That isn't checked,
    /// because the feather-s3 and t3s3 use those pins for LoRa.
    fn is_flash_pin(&self, gpio: u8) -> bool {
        match self {
            Chip::Esp32c6 => (24..=30).contains(&gpio),
            Chip::Esp32s3 => (26..=32).contains(&gpio),
        }
    }

    /// Does the chip read this GPIO at reset, to pick how it boots?
    fn is_strapping_pin(&self, gpio: u8) -> bool {
        match self {
            Chip::Esp32c6 => matches!(gpio, 4 | 5 | 8 | 9 | 15),
            Chip::Esp32s3 => matches!(gpio, 0 | 3 | 45 | 46),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses one board entry: the lines below a `[boards.<name>]` header.
    fn board(text: &str) -> Board {
        toml::from_str(text).unwrap()
    }

    /// Parses a whole `nodes.toml`.
    fn nodes(text: &str) -> NodesFile {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn node_board_must_exist() {
        let boards: BTreeMap<String, Board> = toml::from_str(
            r#"
            [c6-dev]
            chip = "esp32c6"
            radio = "sx126x"
            lora = { sck = 6, mosi = 7, miso = 2, cs = 18, rst = 19, irq = 20, busy = 21 }
            "#,
        )
        .unwrap();
        let nodes = nodes(
            r#"
            reserved_node_ids = []
            nodes.good = { board = "c6-dev", node_id = 100, power = "always-on", heartbeat = 60, min_interval = 10 }
            nodes.bad = { board = "nope", node_id = 101, power = "always-on", heartbeat = 60, min_interval = 10 }
            "#,
        );
        assert_eq!(
            boards_exist(&boards, &nodes),
            ["node `bad`: board `nope` isn't in boards.toml"]
        );
    }

    #[test]
    fn node_ids_must_be_unique() {
        let good = nodes(
            r#"
            reserved_node_ids = []
            nodes.a = { board = "c6-dev", node_id = 100, power = "always-on", heartbeat = 60, min_interval = 10 }
            nodes.b = { board = "c6-dev", node_id = 101, power = "always-on", heartbeat = 60, min_interval = 10 }
            "#,
        );
        assert!(unique_node_ids(&good).is_empty());

        let bad = nodes(
            r#"
            reserved_node_ids = []
            nodes.a = { board = "c6-dev", node_id = 100, power = "always-on", heartbeat = 60, min_interval = 10 }
            nodes.b = { board = "c6-dev", node_id = 100, power = "always-on", heartbeat = 60, min_interval = 10 }
            "#,
        );
        assert_eq!(
            unique_node_ids(&bad),
            ["nodes `a` and `b` both use node_id 100"]
        );
    }

    #[test]
    fn node_id_must_not_be_reserved() {
        let nodes = nodes(
            r#"
            reserved_node_ids = [0x00, 0x7C]
            nodes.good = { board = "c6-dev", node_id = 100, power = "always-on", heartbeat = 60, min_interval = 10 }
            nodes.bad = { board = "c6-dev", node_id = 0x7C, power = "always-on", heartbeat = 60, min_interval = 10 }
            "#,
        );
        assert_eq!(
            no_reserved_node_ids(&nodes),
            ["node `bad`: node_id 124 (0x7c) is in reserved_node_ids"]
        );
    }

    #[test]
    fn pins_must_exist_on_the_chip() {
        // GPIO 33 and 34 exist on the S3.
        let good = board(
            r#"
            chip = "esp32s3"
            radio = "sx126x"
            lora = { sck = 5, mosi = 6, miso = 3, cs = 7, rst = 8, irq = 33, busy = 34 }
            "#,
        );
        assert!(pins_exist("test", &good).is_empty());

        // The S3 has no GPIO 22.
        let bad_s3 = board(
            r#"
            chip = "esp32s3"
            radio = "sx126x"
            lora = { sck = 22, mosi = 6, miso = 3, cs = 7, rst = 8, irq = 33, busy = 34 }
            "#,
        );
        assert_eq!(
            pins_exist("test", &bad_s3),
            ["board `test`: lora.sck is GPIO 22, which the Esp32s3 doesn't have"]
        );

        // The C6 stops at GPIO 30.
        let bad_c6 = board(
            r#"
            chip = "esp32c6"
            radio = "sx126x"
            lora = { sck = 6, mosi = 7, miso = 2, cs = 18, rst = 19, irq = 31, busy = 21 }
            "#,
        );
        assert_eq!(
            pins_exist("test", &bad_c6),
            ["board `test`: lora.irq is GPIO 31, which the Esp32c6 doesn't have"]
        );
    }

    #[test]
    fn no_gpio_used_twice() {
        let good = board(
            r#"
            chip = "esp32c6"
            radio = "sx126x"
            lora = { sck = 6, mosi = 7, miso = 2, cs = 18, rst = 19, irq = 20, busy = 21 }
            "#,
        );
        assert!(no_pin_twice("test", &good).is_empty());

        let bad = board(
            r#"
            chip = "esp32c6"
            radio = "sx126x"
            lora = { sck = 6, mosi = 6, miso = 2, cs = 18, rst = 19, irq = 20, busy = 21 }
            "#,
        );
        assert_eq!(
            no_pin_twice("test", &bad),
            ["board `test`: lora.sck and lora.mosi both use GPIO 6"]
        );
    }

    #[test]
    fn no_flash_pin() {
        let c6 = board(
            r#"
            chip = "esp32c6"
            radio = "sx126x"
            lora = { sck = 6, mosi = 7, miso = 2, cs = 24, rst = 19, irq = 20, busy = 21 }
            "#,
        );
        assert_eq!(
            no_flash_pins("test", &c6),
            ["board `test`: lora.cs is GPIO 24, which the flash uses"]
        );

        let s3 = board(
            r#"
            chip = "esp32s3"
            radio = "sx126x"
            lora = { sck = 5, mosi = 6, miso = 3, cs = 7, rst = 32, irq = 33, busy = 34 }
            "#,
        );
        assert_eq!(
            no_flash_pins("test", &s3),
            ["board `test`: lora.rst is GPIO 32, which the flash uses"]
        );
    }

    #[test]
    fn strapping_pin_is_a_warning() {
        let good = board(
            r#"
            chip = "esp32c6"
            radio = "sx126x"
            lora = { sck = 6, mosi = 7, miso = 2, cs = 18, rst = 19, irq = 20, busy = 21 }
            "#,
        );
        assert!(strapping_pins("test", &good).is_empty());

        let bad = board(
            r#"
            chip = "esp32c6"
            radio = "sx126x"
            lora = { sck = 6, mosi = 7, miso = 8, cs = 18, rst = 19, irq = 20, busy = 21 }
            "#,
        );
        assert_eq!(strapping_pins("test", &bad).len(), 1);
        assert!(strapping_pins("test", &bad)[0].contains("lora.miso is GPIO 8"));
    }

    #[test]
    fn busy_only_on_sx126x() {
        let sx126x_without_busy = board(
            r#"
            chip = "esp32c6"
            radio = "sx126x"
            lora = { sck = 6, mosi = 7, miso = 2, cs = 18, rst = 19, irq = 20 }
            "#,
        );
        assert_eq!(
            busy_matches_radio("test", &sx126x_without_busy),
            ["board `test`: sx126x radios need lora.busy"]
        );

        let sx127x_with_busy = board(
            r#"
            chip = "esp32c6"
            radio = "sx127x"
            lora = { sck = 6, mosi = 7, miso = 2, cs = 18, rst = 19, irq = 20, busy = 21 }
            "#,
        );
        assert_eq!(
            busy_matches_radio("test", &sx127x_with_busy),
            ["board `test`: sx127x radios have no BUSY pin, remove lora.busy"]
        );

        let sx127x_without_busy = board(
            r#"
            chip = "esp32c6"
            radio = "sx127x"
            lora = { sck = 6, mosi = 7, miso = 2, cs = 18, rst = 19, irq = 20 }
            "#,
        );
        assert!(busy_matches_radio("test", &sx127x_without_busy).is_empty());
    }

    #[test]
    fn irq_required_only_on_sx126x() {
        let sx126x_without_irq = board(
            r#"
            chip = "esp32c6"
            radio = "sx126x"
            lora = { sck = 6, mosi = 7, miso = 2, cs = 18, rst = 19, busy = 21 }
            "#,
        );
        assert_eq!(
            irq_on_sx126x("test", &sx126x_without_irq),
            ["board `test`: sx126x radios need lora.irq (DIO1)"]
        );

        let sx127x_without_irq = board(
            r#"
            chip = "esp32c6"
            radio = "sx127x"
            lora = { sck = 6, mosi = 7, miso = 2, cs = 18, rst = 19 }
            "#,
        );
        assert!(irq_on_sx126x("test", &sx127x_without_irq).is_empty());
        // The firmware learns that the pin is missing because it isn't exported.
        assert!(
            !sx127x_without_irq
                .pins()
                .iter()
                .any(|(pin, _)| pin == "lora.irq")
        );
    }
}
