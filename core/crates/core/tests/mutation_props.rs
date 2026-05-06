//! Property-based tests for the mutation-application layer.
//!
//! Tests:
//! 1. `idempotency_on_mutation_id`  — same inputs → same result (determinism).
//! 2. `ordering_of_independent_mutations_is_irrelevant` — two independent create
//!    mutations commute regardless of application order.
//! 3. `postconditions_hold` — on success, `new_state.version == state.version + 1`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics and expect are the correct failure mode in tests

use std::collections::BTreeSet;

use proptest::prelude::*;
use trilithon_core::caddy::capabilities::CapabilitySet;
use trilithon_core::model::desired_state::DesiredState;
use trilithon_core::model::global::GlobalConfigPatch;
use trilithon_core::model::header::HeaderRules;
use trilithon_core::model::identifiers::{RouteId, UpstreamId};
use trilithon_core::model::matcher::MatcherSet;
use trilithon_core::model::redirect::RedirectRule;
use trilithon_core::model::route::Route;
use trilithon_core::model::tls::TlsConfigPatch;
use trilithon_core::model::upstream::{Upstream, UpstreamDestination, UpstreamProbe};
use trilithon_core::mutation::apply::apply_mutation;
use trilithon_core::mutation::types::Mutation;
use trilithon_core::storage::types::UnixSeconds;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a minimal `Route` with a caller-supplied `RouteId`.
fn minimal_route(id: RouteId, ts: UnixSeconds) -> Route {
    Route {
        id,
        hostnames: vec![],
        upstreams: vec![],
        matchers: MatcherSet::default(),
        headers: HeaderRules::default(),
        redirects: Some(RedirectRule {
            to: "https://example.com".to_owned(),
            status: 301,
        }),
        policy_attachment: None,
        enabled: true,
        created_at: ts,
        updated_at: ts,
    }
}

/// Build a minimal `Upstream` with a caller-supplied `UpstreamId`.
fn minimal_upstream(id: UpstreamId) -> Upstream {
    Upstream {
        id,
        destination: UpstreamDestination::TcpAddr {
            host: "127.0.0.1".to_owned(),
            port: 8080,
        },
        probe: UpstreamProbe::Disabled,
        weight: 1,
        max_request_bytes: None,
    }
}

/// A `CreateRoute` mutation whose `expected_version` matches `version`.
fn create_route_mutation(version: i64) -> Mutation {
    Mutation::CreateRoute {
        expected_version: version,
        route: minimal_route(RouteId::new(), 0),
    }
}

/// A `CreateUpstream` mutation whose `expected_version` matches `version`.
fn create_upstream_mutation(version: i64) -> Mutation {
    Mutation::CreateUpstream {
        expected_version: version,
        upstream: minimal_upstream(UpstreamId::new()),
    }
}

/// A `SetGlobalConfig` mutation that sets `log_level` to "info".
fn set_global_config_mutation(version: i64) -> Mutation {
    Mutation::SetGlobalConfig {
        expected_version: version,
        patch: GlobalConfigPatch {
            log_level: Some(Some("info".to_owned())),
            ..GlobalConfigPatch::default()
        },
    }
}

/// A `SetTlsConfig` mutation that sets `on_demand_enabled`.
fn set_tls_config_mutation(version: i64) -> Mutation {
    Mutation::SetTlsConfig {
        expected_version: version,
        patch: TlsConfigPatch {
            on_demand_enabled: Some(false),
            ..TlsConfigPatch::default()
        },
    }
}

/// A `CapabilitySet` with no modules loaded — sufficient for `CreateRoute`
/// mutations with no upstreams, redirects, or header rules, and for config-only
/// mutations (`SetGlobalConfig`, `SetTlsConfig`).
fn empty_caps() -> CapabilitySet {
    CapabilitySet {
        loaded_modules: BTreeSet::new(),
        caddy_version: "v2.8.4".to_owned(),
        probed_at: 0,
    }
}

/// A `CapabilitySet` with `http.handlers.reverse_proxy` — required for any
/// mutation that references an upstream (e.g. `CreateUpstream`).
fn proxy_caps() -> CapabilitySet {
    CapabilitySet {
        loaded_modules: BTreeSet::from(["http.handlers.reverse_proxy".to_owned()]),
        caddy_version: "v2.8.4".to_owned(),
        probed_at: 0,
    }
}

/// A `CapabilitySet` with `tls` — required for `SetTlsConfig` mutations that
/// set any TLS field.
fn tls_caps() -> CapabilitySet {
    CapabilitySet {
        loaded_modules: BTreeSet::from(["tls".to_owned()]),
        caddy_version: "v2.8.4".to_owned(),
        probed_at: 0,
    }
}

/// A `CapabilitySet` with `http.handlers.static_response` — required for
/// `CreateRoute` mutations that include a redirect rule.
fn static_response_caps() -> CapabilitySet {
    CapabilitySet {
        loaded_modules: BTreeSet::from(["http.handlers.static_response".to_owned()]),
        caddy_version: "v2.8.4".to_owned(),
        probed_at: 0,
    }
}

// ---------------------------------------------------------------------------
// Proptest strategies
// ---------------------------------------------------------------------------

/// Return the `CapabilitySet` required for a given mutation variant index.
fn caps_for_variant(variant: u32) -> CapabilitySet {
    match variant {
        0 => static_response_caps(), // CreateRoute with redirect needs static_response
        1 => proxy_caps(),           // CreateUpstream needs reverse_proxy
        3 => tls_caps(),             // SetTlsConfig needs tls
        _ => empty_caps(),
    }
}

/// Build one mutation by variant index (0-3).
fn mutation_for_variant(variant: u32, version: i64) -> Mutation {
    match variant {
        0 => create_route_mutation(version),
        1 => create_upstream_mutation(version),
        2 => set_global_config_mutation(version),
        _ => set_tls_config_mutation(version),
    }
}

// ---------------------------------------------------------------------------
// Property 1 — determinism / "idempotency on same inputs"
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn apply_mutation_is_deterministic(version in 0_i64..=100_i64, variant in 0_u32..4) {
        let state = DesiredState {
            version,
            ..DesiredState::default()
        };
        let mutation = mutation_for_variant(variant, version);
        let caps = caps_for_variant(variant);

        let result1 = apply_mutation(&state, &mutation, &caps);
        let result2 = apply_mutation(&state, &mutation, &caps);

        // Both calls must succeed or both must fail with the same error.
        match (result1, result2) {
            (Ok(o1), Ok(o2)) => {
                prop_assert_eq!(o1.new_state, o2.new_state);
            }
            (Err(e1), Err(e2)) => {
                prop_assert_eq!(format!("{e1}"), format!("{e2}"));
            }
            _ => {
                return Err(TestCaseError::fail("one call succeeded and the other failed"));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Property 2 — commutativity of independent mutations
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn ordering_of_independent_mutations_is_irrelevant(version in 0_i64..=100_i64) {
        let state = DesiredState {
            version,
            ..DesiredState::default()
        };
        let caps = static_response_caps();

        // Two CreateRoute mutations with different RouteIds.
        let id_a = RouteId::new();
        let id_b = RouteId::new();

        let mut_a = Mutation::CreateRoute {
            expected_version: version,
            route: minimal_route(id_a.clone(), 0),
        };
        let mut_b = Mutation::CreateRoute {
            expected_version: version,
            route: minimal_route(id_b.clone(), 0),
        };
        let mut_b_after_a = Mutation::CreateRoute {
            expected_version: version + 1,
            route: minimal_route(id_b, 0),
        };
        let mut_a_after_b = Mutation::CreateRoute {
            expected_version: version + 1,
            route: minimal_route(id_a, 0),
        };

        // Order A then B.
        let after_a = apply_mutation(&state, &mut_a, &caps)
            .expect("A should succeed");
        let ab = apply_mutation(&after_a.new_state, &mut_b_after_a, &caps)
            .expect("B after A should succeed");

        // Order B then A.
        let after_b = apply_mutation(&state, &mut_b, &caps)
            .expect("B should succeed");
        let ba = apply_mutation(&after_b.new_state, &mut_a_after_b, &caps)
            .expect("A after B should succeed");

        // Both orderings must produce identical final states.
        prop_assert_eq!(ab.new_state, ba.new_state);
    }
}

// ---------------------------------------------------------------------------
// Property 3 — post-condition: version increments by exactly 1
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn postconditions_hold(version in 0_i64..=100_i64, variant in 0_u32..4) {
        let state = DesiredState {
            version,
            ..DesiredState::default()
        };
        let mutation = mutation_for_variant(variant, version);
        let caps = caps_for_variant(variant);

        let outcome = apply_mutation(&state, &mutation, &caps)
            .expect("compatible mutation must succeed");

        prop_assert_eq!(outcome.new_state.version, state.version + 1);
    }
}
