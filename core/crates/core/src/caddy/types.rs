//! Value types for the Caddy admin API.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Opaque Caddy admin JSON document. Internally a `serde_json::Value`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaddyConfig(pub serde_json::Value);

/// An RFC 6901 JSON Pointer targeting a Caddy configuration path.
///
/// Must start with `/apps/`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CaddyJsonPointer(pub String);

/// A sequence of JSON Patch operations (RFC 6902).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonPatch(pub Vec<JsonPatchOp>);

/// A single JSON Patch operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum JsonPatchOp {
    /// Add a value at `path`.
    Add {
        /// JSON Pointer target path.
        path: String,
        /// Value to add.
        value: serde_json::Value,
    },
    /// Remove the value at `path`.
    Remove {
        /// JSON Pointer target path.
        path: String,
    },
    /// Replace the value at `path`.
    Replace {
        /// JSON Pointer target path.
        path: String,
        /// Replacement value.
        value: serde_json::Value,
    },
    /// Assert the value at `path` equals `value`.
    Test {
        /// JSON Pointer target path.
        path: String,
        /// Expected value.
        value: serde_json::Value,
    },
}

/// Set of loaded Caddy module identifiers and the running Caddy version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadedModules {
    /// Module identifiers, for example `"http.handlers.reverse_proxy"`.
    pub modules: BTreeSet<String>,
    /// Caddy server version string.
    pub caddy_version: String,
}

/// Health information for a single upstream backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamHealth {
    /// Network address of the upstream (e.g. `"127.0.0.1:8080"`).
    pub address: String,
    /// Whether the upstream is currently considered healthy.
    pub healthy: bool,
    /// Total number of requests routed to this upstream.
    pub num_requests: u64,
    /// Number of failed requests to this upstream.
    pub fails: u64,
}

/// Summary of a TLS certificate managed by Caddy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsCertificate {
    /// Subject alternative names covered by this certificate.
    pub names: Vec<String>,
    /// Certificate validity start time as a Unix timestamp (seconds).
    pub not_before: i64,
    /// Certificate validity end time as a Unix timestamp (seconds).
    pub not_after: i64,
    /// Issuer distinguished name.
    pub issuer: String,
}

/// Reachability state of the Caddy admin endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    /// The admin endpoint responded successfully.
    Reachable,
    /// The admin endpoint could not be reached.
    Unreachable,
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use super::*;

    #[test]
    fn serde_round_trip_loaded_modules() {
        let original = LoadedModules {
            modules: BTreeSet::from([
                "http.handlers.reverse_proxy".to_owned(),
                "http.handlers.static_response".to_owned(),
            ]),
            caddy_version: "v2.8.4".to_owned(),
        };

        let json = serde_json::to_string(&original).expect("serialise");
        let round_tripped: LoadedModules = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(original, round_tripped);
    }
}
