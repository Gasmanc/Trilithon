//! `DesiredState` aggregate over all domain model types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::{
    global::GlobalConfig,
    identifiers::{PolicyId, PresetId, RouteId, UpstreamId},
    policy::{PolicyAttachment, PresetVersion},
    primitive::JsonPointer,
    route::Route,
    tls::TlsConfig,
    upstream::Upstream,
};

/// Aggregate of all configuration that the proxy should converge to.
///
/// All collections are [`BTreeMap`] to ensure deterministic key ordering,
/// which is required by the snapshot writer in Phase 5 for canonical hashing.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesiredState {
    /// Monotonic optimistic-concurrency anchor.
    pub version: i64,
    /// Route definitions keyed by route id.
    pub routes: BTreeMap<RouteId, Route>,
    /// Upstream backends keyed by upstream id.
    pub upstreams: BTreeMap<UpstreamId, Upstream>,
    /// Policy attachments keyed by policy id.
    pub policies: BTreeMap<PolicyId, PolicyAttachment>,
    /// Preset versions keyed by preset id.
    pub presets: BTreeMap<PresetId, PresetVersion>,
    /// Global TLS configuration.
    pub tls: TlsConfig,
    /// Global proxy configuration.
    pub global: GlobalConfig,
    /// Opaque JSON extensions to be merged into the rendered Caddy config.
    ///
    /// Keys are RFC 6901 JSON Pointers pointing to paths in the Caddy
    /// configuration tree. Values are merged after the Trilithon-owned keys
    /// are written; a collision with a Trilithon-owned key is an error.
    ///
    /// This field MUST NOT be used to overwrite keys managed by the renderer.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unknown_extensions: BTreeMap<JsonPointer, serde_json::Value>,
}

impl DesiredState {
    /// Return an empty [`DesiredState`] at version 0.
    pub fn empty() -> Self {
        Self::default()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    missing_docs
)]
mod tests {
    use super::*;
    use crate::model::{
        header::HeaderRules,
        identifiers::{PolicyId, PresetId, RouteId, UpstreamId},
        matcher::MatcherSet,
        route::{HostPattern, Route},
        upstream::{Upstream, UpstreamDestination, UpstreamProbe},
    };

    fn make_route(id: &str) -> Route {
        Route {
            id: RouteId(id.to_owned()),
            hostnames: vec![HostPattern::Exact("example.com".to_owned())],
            upstreams: vec![],
            matchers: MatcherSet::default(),
            headers: HeaderRules::default(),
            redirects: None,
            policy_attachment: None,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn make_upstream(id: &str, port: u16) -> Upstream {
        Upstream {
            id: UpstreamId(id.to_owned()),
            destination: UpstreamDestination::TcpAddr {
                host: "127.0.0.1".to_owned(),
                port,
            },
            probe: UpstreamProbe::Disabled,
            weight: 1,
            max_request_bytes: None,
        }
    }

    #[test]
    fn serde_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let route_id = RouteId("01ROUTE0000000000000000001".to_owned());
        let up1_id = UpstreamId("01UPSTREAM000000000000001A".to_owned());
        let up2_id = UpstreamId("01UPSTREAM000000000000002B".to_owned());
        let preset_id = PresetId("01PRESET0000000000000000A1".to_owned());

        let mut state = DesiredState::empty();
        state.version = 7;
        state
            .routes
            .insert(route_id, make_route("01ROUTE0000000000000000001"));
        state
            .upstreams
            .insert(up1_id, make_upstream("01UPSTREAM000000000000001A", 8080));
        state
            .upstreams
            .insert(up2_id, make_upstream("01UPSTREAM000000000000002B", 8081));
        state.presets.insert(
            preset_id.clone(),
            PresetVersion {
                preset_id: preset_id.clone(),
                version: 2,
                body_json: r#"{"rate_limit":50}"#.to_owned(),
            },
        );
        state.policies.insert(
            PolicyId("01POLICY000000000000000001".to_owned()),
            PolicyAttachment {
                preset_id,
                preset_version: 2,
            },
        );

        let value = serde_json::to_value(&state)?;
        let restored: DesiredState = serde_json::from_value(value)?;
        assert_eq!(state, restored);
        Ok(())
    }

    #[test]
    fn btreemap_iteration_is_sorted() {
        let mut state = DesiredState::empty();
        // Insert in reverse order.
        state.routes.insert(
            RouteId("02ROUTE0000000000000000002".to_owned()),
            make_route("02ROUTE0000000000000000002"),
        );
        state.routes.insert(
            RouteId("01ROUTE0000000000000000001".to_owned()),
            make_route("01ROUTE0000000000000000001"),
        );

        let keys: Vec<&RouteId> = state.routes.keys().collect();
        assert_eq!(keys[0].as_str(), "01ROUTE0000000000000000001");
        assert_eq!(keys[1].as_str(), "02ROUTE0000000000000000002");
    }

    // -----------------------------------------------------------------------
    // Slice 8.3: unknown_extensions round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn unknown_extensions_round_trip() {
        let mut state = DesiredState::empty();
        state.unknown_extensions.insert(
            JsonPointer("/apps/foo".to_owned()),
            serde_json::json!({"bar": 1}),
        );

        let bytes_a = crate::canonical_json::to_canonical_bytes(&state).expect("canonicalise");
        let restored: DesiredState = serde_json::from_slice(&bytes_a).expect("deserialise");
        let bytes_b =
            crate::canonical_json::to_canonical_bytes(&restored).expect("canonicalise restored");

        assert_eq!(
            bytes_a, bytes_b,
            "canonical bytes must be stable after round-trip"
        );
        assert_eq!(state.unknown_extensions, restored.unknown_extensions);
    }

    /// Deserialise a pre-Phase-8 fixture (no `unknown_extensions` field) and
    /// confirm it succeeds — the `#[serde(default)]` annotation is the guard.
    #[test]
    fn pre_phase8_fixture_deserialises_without_unknown_extensions() {
        // Minimal DesiredState JSON as produced before Phase 8, without the
        // `unknown_extensions` field.
        let fixture = r#"{
            "version": 0,
            "routes": {},
            "upstreams": {},
            "policies": {},
            "presets": {},
            "tls": {"on_demand_enabled": false},
            "global": {}
        }"#;
        let state: DesiredState = serde_json::from_str(fixture).expect("should deserialise");
        assert!(
            state.unknown_extensions.is_empty(),
            "unknown_extensions must default to empty"
        );
    }

    // -----------------------------------------------------------------------
    // Slice 8.3: canonical JSON byte-stability under insert order
    // -----------------------------------------------------------------------

    proptest::proptest! {
        #[test]
        fn canonical_json_byte_stable_under_insert_order(
            // Generate a small set of key suffixes (a-z) and value ints.
            keys in proptest::collection::vec(
                proptest::sample::select(&["a", "b", "c", "d", "e", "f"]),
                1..=6,
            ),
            vals in proptest::collection::vec(0i64..=100, 1..=6),
        ) {
            // Deduplicate: last-writer-wins, so collect into a BTreeMap
            // first to get one canonical value per key.
            let deduped: std::collections::BTreeMap<&str, i64> =
                keys.iter().copied().zip(vals.iter().copied()).collect();
            let pairs: Vec<(&str, i64)> = deduped.into_iter().collect();

            // Insert in forward order.
            let mut state_fwd = DesiredState::empty();
            for (k, v) in &pairs {
                state_fwd.unknown_extensions.insert(
                    JsonPointer(format!("/apps/{k}")),
                    serde_json::json!(v),
                );
            }

            // Insert in reverse order.
            let mut state_rev = DesiredState::empty();
            for (k, v) in pairs.iter().rev() {
                state_rev.unknown_extensions.insert(
                    JsonPointer(format!("/apps/{k}")),
                    serde_json::json!(v),
                );
            }

            let bytes_fwd = crate::canonical_json::to_canonical_bytes(&state_fwd)
                .expect("canonicalise fwd");
            let bytes_rev = crate::canonical_json::to_canonical_bytes(&state_rev)
                .expect("canonicalise rev");

            proptest::prop_assert_eq!(bytes_fwd, bytes_rev);
        }
    }
}
