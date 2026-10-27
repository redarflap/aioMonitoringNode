//! Kind 4: the battery voltage.

use heapless::CapacityError;

use crate::{Bytes, Section};

/// The battery voltage as one wire byte: mV / 20, rounded (0–5.08 V).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Battery(u8);

impl Battery {
    /// No reading yet, or the reading failed.
    pub const NOT_MEASURED: Self = Self(0xFF);

    /// Rounds to 20 mV steps. Anything above 5.08 V clamps to 0xFE, so it
    /// can't turn into `NOT_MEASURED`.
    pub fn from_millivolts(mv: u16) -> Self {
        // u32, so adding 10 for the rounding can't overflow.
        let steps = (u32::from(mv) + 10) / 20;
        Self(steps.min(0xFE) as u8)
    }
}

impl Section for Battery {
    const KIND: u8 = 4;

    fn encode(&self, out: &mut Bytes) -> Result<(), CapacityError> {
        out.extend_from_slice(&[self.0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Packet;

    /// The body bytes of `battery`.
    fn body(battery: Battery) -> Bytes {
        let mut out = Bytes::new();
        battery.encode(&mut out).unwrap();
        out
    }

    #[test]
    fn golden_bytes() {
        assert_eq!(body(Battery::from_millivolts(0)), [0x00]);
        assert_eq!(body(Battery::from_millivolts(3700)), [0xB9]);
        assert_eq!(body(Battery::from_millivolts(3710)), [0xBA]); // rounds up
        assert_eq!(body(Battery::from_millivolts(5080)), [0xFE]);
        assert_eq!(body(Battery::from_millivolts(6000)), [0xFE]); // clamped
        assert_eq!(body(Battery::NOT_MEASURED), [0xFF]);
    }

    #[test]
    fn in_a_packet() {
        let mut packet = Packet::new(100);
        packet.push(&Battery::from_millivolts(3700)).unwrap();
        assert_eq!(packet.as_bytes(), [0x64, 0x04, 0xB9]);
    }
}
