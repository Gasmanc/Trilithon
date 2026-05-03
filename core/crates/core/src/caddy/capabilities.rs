//! `CaddyCapabilities` — the probe result record capturing loaded modules,
//! Caddy version, and the timestamp at which the probe was taken.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::storage::types::UnixSeconds;

/// The set of capabilities observed from a Caddy instance at a point in time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CaddyCapabilities {
    /// Caddy module identifiers present in the running instance.
    pub loaded_modules: BTreeSet<String>,
    /// Caddy server version string reported by the admin API.
    pub caddy_version: String,
    /// Unix epoch seconds at which the probe was taken.
    pub probed_at: UnixSeconds,
}

/// Mutation-time alias used by Phase 4 when composing mutation payloads.
pub type CapabilitySet = CaddyCapabilities;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use super::*;

    fn make_caps() -> CaddyCapabilities {
        CaddyCapabilities {
            loaded_modules: BTreeSet::from([
                "http.handlers.reverse_proxy".to_owned(),
                "http.handlers.static_response".to_owned(),
            ]),
            caddy_version: "v2.8.4".to_owned(),
            probed_at: 1_700_000_000,
        }
    }

    #[test]
    fn serde_round_trip() {
        let original = make_caps();
        let json = serde_json::to_string(&original).expect("serialise");
        let round_tripped: CaddyCapabilities = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(original, round_tripped);
    }

    fn compute_hash(v: &impl Hash) -> u64 {
        let mut h = DefaultHasher::new();
        v.hash(&mut h);
        h.finish()
    }

    #[test]
    fn eq_and_hash_stable() {
        let a = make_caps();
        let b = make_caps();

        assert_eq!(a, b, "identical values must compare equal");
        assert_eq!(
            compute_hash(&a),
            compute_hash(&b),
            "identical values must produce the same hash"
        );
    }
}
