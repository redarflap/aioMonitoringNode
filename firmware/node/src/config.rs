//! Values from nodes.toml and boards.toml, passed in by the xtask at build time.

#[cfg(feature = "source-battery")]
use proto::{Battery, Section};

const fn parse_u8(s: &str) -> u8 {
    match u8::from_str_radix(s, 10) {
        Ok(v) => v,
        Err(_) => panic!("expected a number from 0 to 255"),
    }
}

#[cfg(feature = "sx126x")]
const fn parse_u16(s: &str) -> u16 {
    match u16::from_str_radix(s, 10) {
        Ok(v) => v,
        Err(_) => panic!("expected a number from 0 to 65535"),
    }
}

const fn parse_u32(s: &str) -> u32 {
    match u32::from_str_radix(s, 10) {
        Ok(v) => v,
        Err(_) => panic!("expected a number"),
    }
}

/// Reads a comma-separated list like `5,-,0`: a number per entry, `-` for
/// none. Fails the build unless it has exactly `N` entries.
#[allow(dead_code, reason = "only sources with fields read a list")]
const fn parse_list<const N: usize>(s: &str) -> [Option<u32>; N] {
    let bytes = s.as_bytes();
    let mut list = [None; N];
    let mut i = 0; // position in `bytes`
    let mut n = 0; // entries read so far
    loop {
        if n == N {
            panic!("more entries than fields");
        }
        if i < bytes.len() && bytes[i] == b'-' {
            i += 1;
        } else {
            let start = i;
            let mut value: u32 = 0;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                // Too big for a u32 fails the build too.
                value = value * 10 + (bytes[i] - b'0') as u32;
                i += 1;
            }
            if i == start {
                panic!("expected a number or `-`");
            }
            list[n] = Some(value);
        }
        n += 1;
        if i == bytes.len() {
            break;
        }
        if bytes[i] != b',' {
            panic!("expected `,` between entries");
        }
        i += 1;
    }
    if n != N {
        panic!("fewer entries than fields");
    }
    list
}

pub const NODE_ID: u8 = parse_u8(env!("AIO_NODE_ID"));
pub const HEARTBEAT_S: u32 = parse_u32(env!("AIO_HEARTBEAT"));
pub const MIN_INTERVAL_S: u32 = parse_u32(env!("AIO_MIN_INTERVAL"));

// GPIO numbers of the LoRa radio.
pub const LORA_SCK: u8 = parse_u8(env!("AIO_PIN_LORA_SCK"));
pub const LORA_MOSI: u8 = parse_u8(env!("AIO_PIN_LORA_MOSI"));
pub const LORA_MISO: u8 = parse_u8(env!("AIO_PIN_LORA_MISO"));
pub const LORA_CS: u8 = parse_u8(env!("AIO_PIN_LORA_CS"));
pub const LORA_RST: u8 = parse_u8(env!("AIO_PIN_LORA_RST"));
/// DIO1 on SX126x, DIO0 on SX127x. SX127x boards may leave it out; the
/// firmware then polls the radio over SPI instead.
pub const LORA_IRQ: Option<u8> = match option_env!("AIO_PIN_LORA_IRQ") {
    Some(s) => Some(parse_u8(s)),
    None => None,
};
/// Only SX126x radios have a BUSY pin.
pub const LORA_BUSY: Option<u8> = match option_env!("AIO_PIN_LORA_BUSY") {
    Some(s) => Some(parse_u8(s)),
    None => None,
};

/// The GPIO that reads the battery through its voltage divider.
#[cfg(feature = "source-battery")]
pub const BATTERY_ADC: u8 = parse_u8(env!("AIO_PIN_BATTERY_ADC"));
/// Battery voltage / pin voltage, × 1000.
#[cfg(feature = "source-battery")]
pub const BATTERY_DIVIDER_PERMILLE: u32 = parse_u32(env!("AIO_BATTERY_DIVIDER_PERMILLE"));
/// One threshold per field of `Battery::FIELDS`, in wire units; `None` never
/// triggers a packet. The defaults from `crates/proto`, with the overrides
/// from nodes.toml.
#[cfg(feature = "source-battery")]
pub const BATTERY_THRESHOLDS: [Option<u32>; Battery::FIELDS.len()] =
    parse_list(env!("AIO_THRESHOLDS_BATTERY"));

/// SX126x only: the voltage in mV that the radio feeds its TCXO.
/// `None` when the board has no TCXO.
#[cfg(feature = "sx126x")]
pub const TCXO_MV: Option<u16> = match option_env!("AIO_TCXO_MV") {
    Some(s) => Some(parse_u16(s)),
    None => None,
};
