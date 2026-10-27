//! Values from nodes.toml and boards.toml, passed in by the xtask at build time.

const fn parse_u8(s: &str) -> u8 {
    match u8::from_str_radix(s, 10) {
        Ok(v) => v,
        Err(_) => panic!("expected a number from 0 to 255"),
    }
}

const fn parse_u32(s: &str) -> u32 {
    match u32::from_str_radix(s, 10) {
        Ok(v) => v,
        Err(_) => panic!("expected a number"),
    }
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
/// DIO1 on SX126x, DIO0 on SX127x.
pub const LORA_IRQ: u8 = parse_u8(env!("AIO_PIN_LORA_IRQ"));
/// Only SX126x radios have a BUSY pin.
pub const LORA_BUSY: Option<u8> = match option_env!("AIO_PIN_LORA_BUSY") {
    Some(s) => Some(parse_u8(s)),
    None => None,
};
