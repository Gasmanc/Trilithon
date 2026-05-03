//! Property-based tests for the mutation-application layer.
//!
//! Tests:
//! 1. `idempotency_on_mutation_id`  — same inputs → same result (determinism).
//! 2. `ordering_of_independent_mutations_is_irrelevant` — two `CreateRoute`
//!    mutations on different IDs commute.
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
use trilithon_core::model::header::HeaderRules;
use trilithon_core::model::identifiers::RouteId;
use trilithon_core::model::matcher::MatcherSet;
use trilithon_core::model::route::Route;
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
        redirects: None,
        policy_attachment: None,
        enabled: true,
        created_at: ts,
        updated_at: ts,
    }
}

/// A `CreateRoute` mutation whose `expected_version` matches `version`.
fn create_route_mutation(version: i64) -> Mutation {
    Mutation::CreateRoute {
        expected_version: version,
        route: minimal_route(RouteId::new(), 0),
    }
}

/// A `CapabilitySet` with no modules loaded — sufficient for `CreateRoute`
/// mutations that have no upstreams, redirects, or header rules.
fn caps_with_everything() -> CapabilitySet {
    CapabilitySet {
        loaded_modules: BTreeSet::new(),
        caddy_version: "v2.8.4".to_owned(),
        probed_at: 0,
    }
}

// ---------------------------------------------------------------------------
// Property 1 — determinism / "idempotency on same inputs"
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn idempotency_on_mutation_id(version in 0_i64..=100_i64) {
        let state = DesiredState {
            version,
            ..DesiredState::default()
        };
        let mutation = create_route_mutation(version);
        let caps = caps_with_everything();

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
        let caps = caps_with_everything();

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
    fn postconditions_hold(version in 0_i64..=100_i64) {
        let state = DesiredState {
            version,
            ..DesiredState::default()
        };
        let mutation = create_route_mutation(version);
        let caps = caps_with_everything();

        let outcome = apply_mutation(&state, &mutation, &caps)
            .expect("compatible mutation must succeed");

        prop_assert_eq!(outcome.new_state.version, state.version + 1);
    }
}
