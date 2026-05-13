//! Seam test: applier-audit-writer
//!
//! Contracts under test (mirror seams.md):
//!   - `trilithon_core::reconciler::ApplyOutcome`
//!   - `trilithon_core::reconciler::ApplyAuditNotes`
//!   - `trilithon_core::audit::AuditEvent`

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::disallowed_methods
)]
// reason: seam test — panics are the correct failure mode here

mod applier_audit_writer_seam {
    use trilithon_core::reconciler::{ApplyAuditNotes, ReloadKind};

    /// Contract: `ApplyAuditNotes` is serialisable (`notes_to_string` produces non-empty output).
    #[test]
    fn apply_audit_notes_serialises_to_non_empty_string() {
        use trilithon_adapters::audit_notes::notes_to_string;
        let notes = ApplyAuditNotes {
            reload_kind: ReloadKind::Graceful {
                drain_window_ms: None,
            },
            applied_state: trilithon_core::reconciler::AppliedStateTag::Applied,
            drain_window_ms: None,
            error_kind: None,
            error_detail: None,
            caddy_status: None,
            stale_version: None,
            current_version: None,
        };
        let s = notes_to_string(&notes);
        assert!(
            !s.is_empty(),
            "notes_to_string must produce non-empty output"
        );
    }

    /// Contract: `AuditEvent::ApplyFailed` is a distinct variant from `ApplySucceeded`.
    #[test]
    fn audit_event_apply_variants_are_distinct() {
        use trilithon_core::audit::AuditEvent;
        assert_ne!(
            format!("{:?}", AuditEvent::ApplySucceeded),
            format!("{:?}", AuditEvent::ApplyFailed),
        );
    }
}
