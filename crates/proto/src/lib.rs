//! LoRa packet format and schema registry.
//!
//! A packet is `[node_id] { [kind] [section body] }+`: the node ID, then one
//! section per source. ARCHITECTURE.md has the rules for the bodies.
#![cfg_attr(not(test), no_std)]

use heapless::{CapacityError, Vec};

mod battery;

pub use battery::Battery;

/// The most bytes a LoRa packet can carry.
pub const MAX_PACKET_LEN: usize = 255;

/// The bytes of a packet while it's being built.
pub type Bytes = Vec<u8, MAX_PACKET_LEN>;

/// One value of a section. Step 3 adds its type, scale and unit, so that
/// this becomes the schema the decoder reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    /// The field's name, for example `voltage`. `nodes.toml` uses it for
    /// threshold overrides.
    pub key: &'static str,
    /// The default threshold in wire units: a change by more than this is
    /// worth a packet. `None`: the field never triggers a packet and only
    /// rides along.
    pub threshold: Option<u32>,
}

/// The data of one source, as it goes on the wire.
pub trait Section {
    /// Picks the section's schema. Each source has its own kind, and numbers
    /// are never reused.
    const KIND: u8;

    /// The section's fields, in wire order.
    const FIELDS: &'static [Field];

    /// Appends the section body to `out`. Fails if it doesn't fit.
    fn encode(&self, out: &mut Bytes) -> Result<(), CapacityError>;

    /// Whether any field changed since `last` by more than its threshold
    /// (see `policy::changed`). `thresholds` has one entry per field of
    /// `FIELDS`, in the same order.
    fn changed(&self, last: &Self, thresholds: &[Option<u32>]) -> bool;
}

/// A section doesn't fit into the packet's 255 bytes.
#[derive(Debug, PartialEq, Eq)]
pub struct TooLong;

/// One packet: the node ID, then one section per source.
pub struct Packet {
    bytes: Bytes,
}

impl Packet {
    /// Starts a packet with the node ID and no sections yet.
    pub fn new(node_id: u8) -> Self {
        let mut bytes = Bytes::new();
        // Can't fail: the packet is still empty.
        bytes.push(node_id).unwrap();
        Self { bytes }
    }

    /// Appends `[kind] [body]`. If the section doesn't fit, the packet stays
    /// as it was.
    pub fn push<S: Section>(&mut self, section: &S) -> Result<(), TooLong> {
        let len = self.bytes.len();
        let fits = self.bytes.push(S::KIND).is_ok() && section.encode(&mut self.bytes).is_ok();
        if fits {
            Ok(())
        } else {
            self.bytes.truncate(len);
            Err(TooLong)
        }
    }

    /// The finished packet, ready to send.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test-only section of kind `KIND`. `Fill(len, value)` has a body of
    /// `len` bytes, all set to `value`. It has no fields, so it never changes.
    struct Fill<const KIND: u8>(usize, u8);

    impl<const KIND: u8> Section for Fill<KIND> {
        const KIND: u8 = KIND;
        const FIELDS: &'static [Field] = &[];

        fn encode(&self, out: &mut Bytes) -> Result<(), CapacityError> {
            out.extend_from_slice(&vec![self.1; self.0])
        }

        fn changed(&self, _last: &Self, _thresholds: &[Option<u32>]) -> bool {
            false
        }
    }

    #[test]
    fn one_section() {
        let mut packet = Packet::new(100);
        packet.push(&Fill::<0xF0>(2, 0xAB)).unwrap();
        assert_eq!(packet.as_bytes(), [100, 0xF0, 0xAB, 0xAB]);
    }

    #[test]
    fn two_sections() {
        let mut packet = Packet::new(100);
        packet.push(&Fill::<0xF0>(1, 0x11)).unwrap();
        packet.push(&Fill::<0xF1>(2, 0x22)).unwrap();
        assert_eq!(packet.as_bytes(), [100, 0xF0, 0x11, 0xF1, 0x22, 0x22]);
    }

    #[test]
    fn fills_up_to_255_bytes() {
        let mut packet = Packet::new(100);
        // node_id + kind + 253 bytes of body = 255 bytes.
        packet.push(&Fill::<0xF0>(253, 0)).unwrap();
        assert_eq!(packet.as_bytes().len(), 255);

        // Not even the kind byte fits now.
        assert_eq!(packet.push(&Fill::<0xF1>(0, 0)), Err(TooLong));
        assert_eq!(packet.as_bytes().len(), 255);
    }

    #[test]
    fn too_long_section_leaves_packet_unchanged() {
        let mut packet = Packet::new(100);
        packet.push(&Fill::<0xF0>(1, 0x11)).unwrap();
        assert_eq!(packet.push(&Fill::<0xF1>(253, 0x22)), Err(TooLong));
        assert_eq!(packet.as_bytes(), [100, 0xF0, 0x11]);
    }
}
