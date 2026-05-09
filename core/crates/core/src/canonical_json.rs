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
use sha2::{Digest, Sha256};

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
pub(crate) fn canonicalise_value(value: Value) -> Value {
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
            // Only normalise numbers that are natively represented as f64 in
            // serde_json (i.e. they came from a JSON float literal).  Integer
            // values stored as i64/u64 are already in their canonical form and
            // must not be converted through f64, which only represents integers
            // exactly up to 2^53 — converting larger i64/u64 values via f64
            // silently loses precision and corrupts the state before hashing.
            if n.is_f64() {
                if let Some(f) = n.as_f64() {
                    if f.fract() == 0.0 && f.is_finite() {
                        // Prefer i64 representation for values in the safe
                        // integer range (−2^53 to 2^53).
                        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
                        // reason: fract()==0 and is_finite() guarantee truncation is exact;
                        //         bounds are intentionally the f64-safe integer range so
                        //         that round-trip through f64 is lossless
                        if (-9_007_199_254_740_992.0_f64..=9_007_199_254_740_992.0_f64).contains(&f)
                        {
                            return Value::Number(serde_json::Number::from(f as i64));
                        }
                    }
                }
            }
            Value::Number(n)
        }
        other => other,
    }
}

/// Compute the SHA-256 content address of a canonical JSON byte string.
///
/// Returns a lowercase 64-character hex string.  This is the value stored in
/// [`crate::storage::types::SnapshotId`].
#[must_use]
pub fn content_address_bytes(canonical_json_bytes: &[u8]) -> String {
    let digest = Sha256::digest(canonical_json_bytes);
    format!("{digest:x}")
}

/// Compute the SHA-256 content address of `state`'s canonical JSON bytes.
///
/// Returns a lowercase hex string.  Used by tests to verify that semantically
/// equivalent states produce the same identifier.
///
/// # Errors
///
/// Propagates any [`serde_json::Error`] from [`to_canonical_bytes`].
pub fn content_address(state: &DesiredState) -> Result<String, serde_json::Error> {
    let bytes = to_canonical_bytes(state)?;
    Ok(content_address_bytes(&bytes))
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

/// Corpus tests: 50 semantically equivalent JSON variants MUST hash to the
/// same content address.
///
/// The test name path is `canonical_json::corpus` so it matches the spec's
/// `cargo test -p trilithon-core canonical_json::corpus` filter.
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    missing_docs
)]
// reason: test-only code; panics are the correct failure mode in tests
mod corpus {
    use super::*;
    use crate::model::{
        desired_state::DesiredState,
        header::HeaderRules,
        identifiers::{RouteId, UpstreamId},
        matcher::MatcherSet,
        route::{HostPattern, Route},
        upstream::{Upstream, UpstreamDestination, UpstreamProbe},
    };

    /// Build a `DesiredState` with `n` routes and `n` upstreams inserted in
    /// the order defined by `route_order` (a permutation of `0..n`).
    ///
    /// Because `DesiredState.routes` is a `BTreeMap`, the canonical output is
    /// identical regardless of insertion order — this is what the corpus verifies.
    fn state_with_insertion_order(n: usize, route_order: &[usize]) -> DesiredState {
        let mut state = DesiredState::empty();
        state.version = 1;

        for &i in route_order {
            if i >= n {
                continue;
            }
            let rid = format!("CORPUS_ROUTE_{i:04}");
            state.routes.insert(
                RouteId(rid.clone()),
                Route {
                    id: RouteId(rid),
                    hostnames: vec![HostPattern::Exact(format!("host{i}.example.com"))],
                    upstreams: vec![],
                    matchers: MatcherSet::default(),
                    headers: HeaderRules::default(),
                    redirects: None,
                    policy_attachment: None,
                    enabled: true,
                    created_at: 0,
                    updated_at: 0,
                },
            );

            let uid = format!("CORPUS_UPSTREAM_{i:04}");
            let port = u16::try_from(8000 + (i % 1000)).unwrap_or(8000);
            state.upstreams.insert(
                UpstreamId(uid.clone()),
                Upstream {
                    id: UpstreamId(uid),
                    destination: UpstreamDestination::TcpAddr {
                        host: "127.0.0.1".to_owned(),
                        port,
                    },
                    probe: UpstreamProbe::Disabled,
                    weight: 1,
                    max_request_bytes: None,
                },
            );
        }

        state
    }

    /// Compute the SHA-256 content address of canonical bytes for a `DesiredState`.
    fn addr(state: &DesiredState) -> String {
        use sha2::{Digest, Sha256};
        let bytes = to_canonical_bytes(state).expect("canonical serialise");
        format!("{:x}", Sha256::digest(&bytes))
    }

    /// Compute the canonical address by round-tripping through raw JSON that
    /// uses float literals for integer values (e.g. `1.0` instead of `1`).
    /// The canonicaliser MUST normalise them, producing the same address.
    fn addr_via_float_json(state: &DesiredState) -> String {
        use sha2::{Digest, Sha256};
        // Serialise canonically, then re-parse the JSON into a serde_json::Value.
        // Inject a float-valued number at the top level to simulate an alternate
        // encoding: we manually canonicalise the Value and re-serialise.
        let canonical_value = {
            let raw = serde_json::to_value(state).expect("to_value");
            canonicalise_value(raw)
        };

        // Build an equivalent Value where the integer `version` field is
        // represented as a float (1 → 1.0).  After canonicalisation both
        // representations must be byte-identical.
        let mut float_obj = canonical_value;
        if let Value::Object(ref mut map) = float_obj {
            if let Some(v) = map.get("version").and_then(Value::as_i64) {
                #[allow(clippy::cast_precision_loss)]
                // reason: test-only; small integer so no precision loss in practice
                let float_num = serde_json::Number::from_f64(v as f64).expect("finite float");
                map.insert("version".to_owned(), Value::Number(float_num));
            }
        }

        // Canonicalise the float variant — the float-normalisation rule must
        // convert `1.0` back to `1`.
        let canonical_float = canonicalise_value(float_obj);
        let bytes = serde_json::to_vec(&canonical_float).expect("serialise float variant");
        format!("{:x}", Sha256::digest(&bytes))
    }

    /// 50-entry corpus:
    ///
    /// - Entries 0–24: same state (`n` routes) inserted in forward order vs
    ///   reverse order.  Both must produce the same content address because the
    ///   backing `BTreeMap` sorts keys regardless of insertion order.
    ///
    /// - Entries 25–49: same state serialised normally vs with the integer
    ///   `version` field represented as a whole-valued float in the JSON Value.
    ///   The canonicaliser's float-normalisation rule must make both identical.
    #[test]
    fn all_variants_hash_identically() {
        // --- Part 1: insertion-order variants (entries 0-24) ---
        for i in 1..=25_usize {
            let forward: Vec<usize> = (0..i).collect();
            let reverse: Vec<usize> = (0..i).rev().collect();

            let state_fwd = state_with_insertion_order(i, &forward);
            let state_rev = state_with_insertion_order(i, &reverse);

            let addr_fwd = addr(&state_fwd);
            let addr_rev = addr(&state_rev);

            assert_eq!(
                addr_fwd, addr_rev,
                "corpus entry {i} (insertion-order variant): addresses differ.\n\
                 forward  = {addr_fwd}\n\
                 reverse  = {addr_rev}"
            );
        }

        // --- Part 2: float-normalisation variants (entries 25-49) ---
        for i in 1..=25_usize {
            let order: Vec<usize> = (0..i).collect();
            let state = state_with_insertion_order(i, &order);

            let addr_normal = addr(&state);
            let addr_float = addr_via_float_json(&state);

            assert_eq!(
                addr_normal,
                addr_float,
                "corpus entry {} (float-normalisation variant): addresses differ.\n\
                 normal = {addr_normal}\n\
                 float  = {addr_float}",
                i + 25
            );
        }
    }
}
