//! Potato routing modes and the project's privacy-over-latency policy.
//!
//! These are pure policy/data types; the networking that consumes them lives in the `potato`
//! crate. The modes and their documented leaks are defined in PLAN.md section 2.2, and the
//! latency/cover-traffic/decoy rules in sections 3.3, 3.7, and 4.3.

/// A Potato routing mode: the user's chosen latency/privacy trade-off.
///
/// The project priority is **privacy over latency** (PLAN.md section 0), so [`Mode::Balanced`]
/// is the default and [`Mode::Swift`]'s latency features are gated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Lower latency, accepting a small, bounded, documented location leak (PLAN.md 3.3).
    Swift,
    /// Tor-equivalent privacy with the best existing speedups enabled. The default.
    #[default]
    Balanced,
    /// Maximum resistance: cover traffic and timing jitter, at a bandwidth/latency cost.
    Fortress,
}

impl Mode {
    /// Whether this mode may bias path selection by latency (PLAN.md 3.3). Only Swift does,
    /// and even then within a hard cap; every other mode selects paths exactly as Tor does.
    pub fn allows_latency_aware_paths(self) -> bool {
        matches!(self, Mode::Swift)
    }

    /// Whether constant-rate cover traffic is on by default in this mode (PLAN.md 3.7).
    pub fn cover_traffic(self) -> bool {
        matches!(self, Mode::Fortress)
    }

    /// Whether client-side decoy queries default to on in this mode (PLAN.md 4.3 item 7).
    pub fn decoy_queries_default(self) -> bool {
        matches!(self, Mode::Fortress)
    }

    /// Human-readable name, matching the names used in PLAN.md.
    pub fn name(self) -> &'static str {
        match self {
            Mode::Swift => "Swift",
            Mode::Balanced => "Balanced",
            Mode::Fortress => "Fortress",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_balanced() {
        assert_eq!(Mode::default(), Mode::Balanced);
    }

    #[test]
    fn only_swift_biases_paths() {
        assert!(Mode::Swift.allows_latency_aware_paths());
        assert!(!Mode::Balanced.allows_latency_aware_paths());
        assert!(!Mode::Fortress.allows_latency_aware_paths());
    }

    #[test]
    fn fortress_enables_cover_and_decoys() {
        assert!(Mode::Fortress.cover_traffic());
        assert!(Mode::Fortress.decoy_queries_default());
        assert!(!Mode::Balanced.cover_traffic());
        assert!(!Mode::Balanced.decoy_queries_default());
    }
}
