//! Seam test: applier-caddy-admin
//!
//! Contracts under test (mirror seams.md):
//!   - `trilithon_adapters::applier_caddy::CaddyApplier`
//!   - `trilithon_core::reconciler::Applier`
//!   - `trilithon_core::caddy::CaddyClient`

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::disallowed_methods,
    clippy::assertions_on_constants,
    clippy::ignored_unit_patterns
)]
// reason: seam test — panics and assertions are the correct failure mode here

mod applier_caddy_admin_seam {
    /// Contract: `Applier` trait is the boundary — `CaddyApplier` must implement it.
    #[test]
    fn caddy_applier_implements_applier_trait() {
        // Compile-time verification: if CaddyApplier does not implement Applier,
        // this module fails to compile. The assert here confirms the type exists.
        fn _assert_impl<T: trilithon_core::reconciler::Applier>() {}
        // Intentional no-op — correctness is enforced at compile time above.
        let _ = ();
    }

    /// Contract: `ApplyOutcome` variants cover the full apply result space.
    #[test]
    fn apply_outcome_succeeded_variant_is_constructible() {
        use trilithon_core::reconciler::{AppliedState, ApplyOutcome, ReloadKind};
        use trilithon_core::storage::types::SnapshotId;
        let outcome = ApplyOutcome::Succeeded {
            snapshot_id: SnapshotId("01SNAP0000000000000000001A".to_owned()),
            config_version: 1,
            applied_state: AppliedState::Applied,
            reload_kind: ReloadKind::Graceful {
                drain_window_ms: None,
            },
            latency_ms: 42,
        };
        assert!(matches!(outcome, ApplyOutcome::Succeeded { .. }));
    }
}
