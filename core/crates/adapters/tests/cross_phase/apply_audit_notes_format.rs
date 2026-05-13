//! Seam test: apply-audit-notes-format
//!
//! Contracts under test (mirror seams.md):
//!   - `trilithon_core::reconciler::ApplyAuditNotes`
//!   - `trilithon_adapters::audit_notes::notes_to_string`

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::disallowed_methods
)]
// reason: seam test — panics and expect are the correct failure mode here

mod apply_audit_notes_format_seam {
    use trilithon_adapters::audit_notes::notes_to_string;
    use trilithon_core::reconciler::{AppliedStateTag, ApplyAuditNotes, ReloadKind};

    /// Contract: `notes_to_string` output is valid JSON.
    #[test]
    fn notes_to_string_produces_valid_json() {
        let notes = ApplyAuditNotes {
            reload_kind: ReloadKind::Graceful {
                drain_window_ms: None,
            },
            applied_state: AppliedStateTag::Applied,
            drain_window_ms: None,
            error_kind: None,
            error_detail: None,
            caddy_status: None,
            stale_version: None,
            current_version: None,
        };
        let s = notes_to_string(&notes);
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&s);
        assert!(
            parsed.is_ok(),
            "notes_to_string must produce valid JSON; got: {s}"
        );
    }

    /// Contract: keys in notes output are lexicographically sorted (canonical form).
    #[test]
    fn notes_to_string_keys_are_sorted() {
        let notes = ApplyAuditNotes {
            reload_kind: ReloadKind::Graceful {
                drain_window_ms: Some(200),
            },
            applied_state: AppliedStateTag::Applied,
            drain_window_ms: Some(200),
            error_kind: Some("CaddyServerError".to_owned()),
            error_detail: Some("internal error".to_owned()),
            caddy_status: Some(500),
            stale_version: None,
            current_version: None,
        };
        let s = notes_to_string(&notes);
        let obj: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&s).expect("valid JSON object");
        let keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(
            keys, sorted,
            "notes_to_string keys must be lexicographically sorted"
        );
    }
}
