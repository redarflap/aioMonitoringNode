//! Kind 4: the battery voltage.

use heapless::CapacityError;

use crate::{Bytes, Field, Section, Type};

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

    /// The wire value for `policy::changed`: `None` when not measured.
    fn value(self) -> Option<i64> {
        (self != Self::NOT_MEASURED).then_some(i64::from(self.0))
    }
}

impl Section for Battery {
    const KIND: u8 = 4;

    /// Wire steps of 20 mV are 1/50 V. No threshold: a battery drains slowly,
    /// so its voltage only rides along.
    const FIELDS: &'static [Field] = &[Field {
        key: "voltage",
        ty: Type::U8,
        divisor: 50,
        unit: Some("V"),
        device_class: Some("voltage"),
        state_class: Some("measurement"),
        threshold: None,
    }];

    fn encode(&self, out: &mut Bytes) -> Result<(), CapacityError> {
        out.extend_from_slice(&[self.0])
    }

    fn changed(&self, last: &Self, thresholds: &[Option<u32>]) -> bool {
        policy::changed(last.value(), self.value(), thresholds[0])
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

    fn mv(mv: u16) -> Battery {
        Battery::from_millivolts(mv)
    }

    #[test]
    fn voltage_has_no_default_threshold() {
        assert_eq!(Battery::FIELDS.len(), 1);
        assert_eq!(Battery::FIELDS[0].key, "voltage");
        assert_eq!(Battery::FIELDS[0].threshold, None);
    }

    #[test]
    fn body_matches_fields() {
        // So the schema and the encoder can't drift apart.
        let size: usize = Battery::FIELDS.iter().map(|field| field.ty.size()).sum();
        assert_eq!(body(Battery::from_millivolts(3700)).len(), size);
        assert_eq!(body(Battery::NOT_MEASURED).len(), size);
    }

    #[test]
    fn voltage_in_volts() {
        // 0xB9 = 185 steps of 20 mV = 3.7 V.
        let field = Battery::FIELDS[0];
        assert_eq!(field.ty, Type::U8);
        assert_eq!(field.divisor, 50);
        assert_eq!(field.unit, Some("V"));
        // NOT_MEASURED is 0xFF, the missing value of a U8.
        assert_eq!(body(Battery::NOT_MEASURED), [u8::MAX]);
    }

    #[test]
    fn without_threshold_nothing_is_a_change() {
        let none = [None];
        assert!(!mv(4000).changed(&mv(3700), &none));
        assert!(!Battery::NOT_MEASURED.changed(&mv(3700), &none));
        assert!(!mv(3700).changed(&Battery::NOT_MEASURED, &none));
        assert!(!Battery::NOT_MEASURED.changed(&Battery::NOT_MEASURED, &none));
    }

    #[test]
    fn with_threshold_2() {
        let two = [Some(2)];
        // 3700 -> 3740 mV is 2 steps of 20 mV: not more than the threshold.
        assert!(!mv(3740).changed(&mv(3700), &two));
        // 3700 -> 3760 mV is 3 steps.
        assert!(mv(3760).changed(&mv(3700), &two));
        // Measured <-> not measured.
        assert!(Battery::NOT_MEASURED.changed(&mv(3700), &two));
        assert!(mv(3700).changed(&Battery::NOT_MEASURED, &two));
        assert!(!Battery::NOT_MEASURED.changed(&Battery::NOT_MEASURED, &two));
    }

    #[test]
    fn in_a_packet() {
        let mut packet = Packet::new(100);
        packet.push(&Battery::from_millivolts(3700)).unwrap();
        assert_eq!(packet.as_bytes(), [0x64, 0x04, 0xB9]);
    }
}
