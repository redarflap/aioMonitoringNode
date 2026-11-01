//! Send policy (thresholds, min_interval, heartbeat); also runs on the coprocessor.
//!
//! No floats and no `core::fmt`, so it stays small enough for the LP core.
#![cfg_attr(not(test), no_std)]

/// Whether one field changed enough to be worth a packet. Values are in wire
/// units; `None` is a missing value (e.g. "not measured", or a sensor that's
/// gone).
///
/// - No threshold: never a change. The field only rides along.
/// - Both values missing: no change. One missing: change.
/// - Both there: change if they differ by more than `threshold`.
///
/// `i64`, so every wire type up to `u32` fits.
pub fn changed(last: Option<i64>, now: Option<i64>, threshold: Option<u32>) -> bool {
    let Some(threshold) = threshold else {
        return false;
    };
    match (last, now) {
        (None, None) => false,
        // `abs_diff` returns a `u64`, so it can't overflow.
        (Some(last), Some(now)) => last.abs_diff(now) > u64::from(threshold),
        _ => true,
    }
}

/// When to send, in seconds. The xtask makes sure that
/// `1 <= min_interval_s <= heartbeat_s`.
#[derive(Clone, Copy)]
pub struct Policy {
    /// Never send a change sooner than this after the last packet.
    pub min_interval_s: u32,
    /// Send at least this often, even without a change.
    pub heartbeat_s: u32,
}

/// What the report task should do now.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum Decision {
    Send,
    /// Ask again after this many seconds, or sooner when new data arrives.
    Wait(u32),
}

impl Policy {
    /// `since_last_send_s` is `None` until the first packet went out.
    /// `changed` says whether any field changed since the last packet.
    pub fn decide(&self, since_last_send_s: Option<u32>, changed: bool) -> Decision {
        let Some(since) = since_last_send_s else {
            return Decision::Send;
        };
        if since >= self.heartbeat_s {
            Decision::Send
        } else if !changed {
            Decision::Wait(self.heartbeat_s - since)
        } else if since >= self.min_interval_s {
            Decision::Send
        } else {
            Decision::Wait(self.min_interval_s - since)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // changed()

    #[test]
    fn no_threshold_is_never_a_change() {
        assert!(!changed(Some(0), Some(1000), None));
        assert!(!changed(Some(5), None, None));
        assert!(!changed(None, Some(5), None));
        assert!(!changed(None, None, None));
    }

    #[test]
    fn both_missing_is_no_change() {
        assert!(!changed(None, None, Some(0)));
    }

    #[test]
    fn one_missing_is_a_change() {
        assert!(changed(Some(5), None, Some(100)));
        assert!(changed(None, Some(5), Some(100)));
    }

    #[test]
    fn more_than_threshold_is_a_change() {
        assert!(changed(Some(100), Some(103), Some(2)));
        assert!(changed(Some(103), Some(100), Some(2))); // downwards too
    }

    #[test]
    fn exactly_at_threshold_is_no_change() {
        assert!(!changed(Some(100), Some(102), Some(2)));
        assert!(!changed(Some(102), Some(100), Some(2)));
    }

    #[test]
    fn threshold_0_is_any_change() {
        assert!(changed(Some(7), Some(8), Some(0)));
        assert!(!changed(Some(7), Some(7), Some(0)));
    }

    #[test]
    fn negative_values() {
        // -5 → 3 is 8 steps.
        assert!(changed(Some(-5), Some(3), Some(7)));
        assert!(!changed(Some(-5), Some(3), Some(8)));
        assert!(changed(Some(-20), Some(-10), Some(9)));
    }

    #[test]
    fn whole_u32_range_fits() {
        let max = i64::from(u32::MAX);
        assert!(changed(Some(0), Some(max), Some(u32::MAX - 1)));
        assert!(!changed(Some(0), Some(max), Some(u32::MAX)));
    }

    // Policy::decide()

    const POLICY: Policy = Policy {
        min_interval_s: 30,
        heartbeat_s: 600,
    };

    #[test]
    fn nothing_sent_yet_sends() {
        assert_eq!(POLICY.decide(None, false), Decision::Send);
        assert_eq!(POLICY.decide(None, true), Decision::Send);
    }

    #[test]
    fn heartbeat_passed_sends() {
        assert_eq!(POLICY.decide(Some(600), false), Decision::Send); // exactly
        assert_eq!(POLICY.decide(Some(700), false), Decision::Send);
        assert_eq!(POLICY.decide(Some(600), true), Decision::Send);
    }

    #[test]
    fn change_after_min_interval_sends() {
        assert_eq!(POLICY.decide(Some(30), true), Decision::Send); // exactly
        assert_eq!(POLICY.decide(Some(31), true), Decision::Send);
    }

    #[test]
    fn change_before_min_interval_waits_for_it() {
        assert_eq!(POLICY.decide(Some(0), true), Decision::Wait(30));
        assert_eq!(POLICY.decide(Some(29), true), Decision::Wait(1));
    }

    #[test]
    fn no_change_waits_for_heartbeat() {
        assert_eq!(POLICY.decide(Some(0), false), Decision::Wait(600));
        assert_eq!(POLICY.decide(Some(30), false), Decision::Wait(570));
        assert_eq!(POLICY.decide(Some(599), false), Decision::Wait(1));
    }
}
