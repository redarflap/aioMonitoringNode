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
