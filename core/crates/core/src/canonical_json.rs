//! Canonical JSON serialiser for [`crate::model::desired_state::DesiredState`].
//!
//! # Guarantees
//!
//! - Map keys are sorted lexicographically at every nesting level.
//! - Floating-point numbers that are whole numbers are emitted as integers.
//! - Byte-identical inputs produce byte-identical outputs.
//! - Every produced byte string is valid UTF-8 and valid JSON.
//!
//! # Versioning
//!
//! The format is versioned by [`CANONICAL_JSON_VERSION`].  Snapshot rows
//! store this constant so that future format changes can be detected without
//! re-hashing all historical data.

use serde_json::Value;

use crate::model::desired_state::DesiredState;

/// Format version for the canonical JSON serialiser.
///
/// Increment this constant whenever the output format changes in any way that
/// would cause byte-identical inputs to produce different output.
pub const CANONICAL_JSON_VERSION: u32 = 1;

/// Serialise `state` to canonical JSON bytes.
///
/// The returned bytes are deterministic: byte-identical desired states always
/// produce byte-identical output, regardless of call order or thread.
///
/// # Errors
///
/// Returns a [`serde_json::Error`] when serialisation fails (this is
/// practically impossible for well-formed `DesiredState` values, but the
/// `Result` is propagated to let callers handle it cleanly).
pub fn to_canonical_bytes(state: &DesiredState) -> Result<Vec<u8>, serde_json::Error> {
    let value = serde_json::to_value(state)?;
    let canonical = canonicalise_value(value);
    serde_json::to_vec(&canonical)
}

/// Serialise `state` to a canonical JSON string.
///
/// # Errors
///
/// See [`to_canonical_bytes`].
pub fn to_canonical_string(state: &DesiredState) -> Result<String, serde_json::Error> {
    // `serde_json::to_string` returns a `String` directly without requiring
    // an intermediate byte vec; we route through `to_value` + serialise to
    // avoid duplicating the canonicalisation logic.
    let value = serde_json::to_value(state)?;
    let canonical = canonicalise_value(value);
    serde_json::to_string(&canonical)
}

/// Recursively canonicalise a [`Value`]:
///
/// - Objects: keys are sorted lexicographically; values are canonicalised
///   recursively.
/// - Arrays: elements are canonicalised in place (order is preserved).
/// - Numbers: whole-valued floats are converted to integers.
/// - Everything else: unchanged.
fn canonicalise_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut sorted: Vec<(String, Value)> = map
                .into_iter()
                .map(|(k, v)| (k, canonicalise_value(v)))
                .collect();
            sorted.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(canonicalise_value).collect()),
        Value::Number(n) => {
            // If the number is a float that has no fractional part, convert it
            // to an integer representation to produce a stable byte sequence.
            if let Some(f) = n.as_f64() {
                if f.fract() == 0.0 && f.is_finite() {
                    // Prefer i64 representation for values in the i64 range.
                    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
                    // reason: fract()==0 and is_finite() guarantee truncation is exact;
                    //         precision loss in bounds check is acceptable (boundary
                    //         values round away from i64 range, never into it)
                    if f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                        return Value::Number(serde_json::Number::from(f as i64));
                    }
                }
            }
            Value::Number(n)
        }
        other => other,
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
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use super::*;
    use crate::model::{
        desired_state::DesiredState,
        header::HeaderRules,
        identifiers::{PolicyId, PresetId, RouteId, UpstreamId},
        matcher::MatcherSet,
        policy::{PolicyAttachment, PresetVersion},
        route::{HostPattern, Route},
        upstream::{Upstream, UpstreamDestination, UpstreamProbe},
    };

    fn empty_state() -> DesiredState {
        DesiredState::empty()
    }

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

    /// Build a fixture `DesiredState` with a fixed set of routes and upstreams.
    fn fixture_state(version: i64, route_count: usize, upstream_count: usize) -> DesiredState {
        let mut state = DesiredState::empty();
        state.version = version;

        for i in 0..route_count {
            let id = format!("01ROUTE{i:020}");
            state.routes.insert(RouteId(id.clone()), make_route(&id));
        }

        for i in 0..upstream_count {
            let id = format!("01UPSTREAM{i:018}");
            let port = 8080 + u16::try_from(i % 1000).unwrap_or(0);
            state
                .upstreams
                .insert(UpstreamId(id.clone()), make_upstream(&id, port));
        }

        state
    }

    #[test]
    fn version_constant_is_set() {
        assert_eq!(CANONICAL_JSON_VERSION, 1);
    }

    #[test]
    fn empty_state_is_stable() {
        let state = empty_state();
        let a = to_canonical_bytes(&state).expect("first serialise");
        let b = to_canonical_bytes(&state).expect("second serialise");
        assert_eq!(a, b);
    }

    #[test]
    fn output_is_valid_json() {
        let state = fixture_state(1, 3, 2);
        let bytes = to_canonical_bytes(&state).expect("serialise");
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).expect("parse");
        assert!(parsed.is_object());
    }

    #[test]
    fn keys_are_sorted_lexicographically() {
        // A DesiredState that serialises to an object — we inspect the raw
        // JSON to verify key order.
        let state = fixture_state(1, 2, 2);
        let json = to_canonical_string(&state).expect("serialise");

        // Walk through the JSON and collect top-level key appearances.
        // serde_json preserves insertion order in Value::Object so the
        // canonicalised value has lexicographic order.
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse");
        let keys: Vec<&str> = value
            .as_object()
            .expect("root is object")
            .keys()
            .map(String::as_str)
            .collect();

        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted, "top-level keys are not sorted: {keys:?}");
    }

    #[test]
    fn byte_identical() {
        // Generate 50 fixture states with varying sizes and verify that
        // serialising the same state twice always produces identical bytes.
        for i in 0..50_usize {
            let state = fixture_state(i64::try_from(i).unwrap_or(0), i % 5, i % 3);
            let a = to_canonical_bytes(&state).expect("first serialise");
            let b = to_canonical_bytes(&state).expect("second serialise");
            assert_eq!(
                a, b,
                "fixture {i}: serialisations differ — canonical JSON is not deterministic"
            );
        }
    }

    #[test]
    fn map_key_order_is_stable_across_insertion_order() {
        // Insert routes in reverse alphabetical order; check that canonical
        // output has them in lexicographic order.
        let mut state = DesiredState::empty();
        state
            .routes
            .insert(RouteId("ZZZ".to_owned()), make_route("ZZZ"));
        state
            .routes
            .insert(RouteId("AAA".to_owned()), make_route("AAA"));
        state
            .routes
            .insert(RouteId("MMM".to_owned()), make_route("MMM"));

        let json = to_canonical_string(&state).expect("serialise");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse");
        let routes = value
            .get("routes")
            .expect("routes key")
            .as_object()
            .expect("routes is object");
        let route_keys: Vec<&str> = routes.keys().map(String::as_str).collect();
        let mut sorted = route_keys.clone();
        sorted.sort_unstable();
        assert_eq!(route_keys, sorted, "route keys not sorted: {route_keys:?}");
    }

    #[test]
    fn with_policy_and_preset() {
        let mut state = DesiredState::empty();
        let preset_id = PresetId("PRESET001".to_owned());
        state.presets.insert(
            preset_id.clone(),
            PresetVersion {
                preset_id: preset_id.clone(),
                version: 1,
                body_json: r#"{"rate_limit":100}"#.to_owned(),
            },
        );
        state.policies.insert(
            PolicyId("POLICY001".to_owned()),
            PolicyAttachment {
                preset_id,
                preset_version: 1,
            },
        );

        let a = to_canonical_bytes(&state).expect("first serialise");
        let b = to_canonical_bytes(&state).expect("second serialise");
        assert_eq!(a, b);
    }
}
