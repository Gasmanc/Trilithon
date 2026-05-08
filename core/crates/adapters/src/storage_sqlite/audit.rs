//! Storage-side audit insertion with kind validation.
//!
//! `validate_kind` is the belt-and-braces gate for `audit_log` row inserts.
//! It checks `row.kind` against the §6.6 closed vocabulary **and** the
//! dotted-kind pattern so that a future vocabulary entry whose string does not
//! conform to the pattern is caught before touching the database.

use trilithon_core::storage::{
    audit_vocab::AUDIT_KINDS, error::StorageError, types::AuditEventRow,
};

/// Validate that `kind` matches the §6.6 dotted-kind pattern.
///
/// Pattern: one or more dot-separated segments, each starting with `[a-z]`
/// and consisting of `[a-z0-9-]*`.  At least two segments are required.
///
/// Returns `Ok(())` when `kind` is valid, or
/// `Err(StorageError::AuditKindUnknown)` otherwise.
fn validate_kind_pattern(kind: &str) -> Result<(), StorageError> {
    // Manual match — avoids a `regex` dependency in adapters.
    let matches = kind.contains('.')
        && kind.split('.').all(|seg| {
            let mut chars = seg.chars();
            let Some(first) = chars.next() else {
                return false;
            };
            first.is_ascii_lowercase()
                && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        });

    if matches {
        Ok(())
    } else {
        Err(StorageError::AuditKindUnknown {
            kind: kind.to_owned(),
        })
    }
}

/// Validate that `row.kind` is in the §6.6 closed vocabulary AND matches the
/// dotted-kind pattern.
///
/// The closed-vocabulary check is the primary gate; the pattern check is
/// belt-and-braces against malformed future additions.
///
/// # Errors
///
/// Returns `StorageError::AuditKindUnknown` when `row.kind` is not in
/// `AUDIT_KINDS` or does not match the §6.6 dotted-kind pattern.
pub fn validate_kind(row: &AuditEventRow) -> Result<(), StorageError> {
    // Primary check: closed vocabulary list.
    if !AUDIT_KINDS.contains(&row.kind.as_str()) {
        return Err(StorageError::AuditKindUnknown {
            kind: row.kind.clone(),
        });
    }
    // Belt-and-braces: pattern check.
    validate_kind_pattern(&row.kind)
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use trilithon_core::storage::{
        helpers::audit_prev_hash_seed,
        types::{ActorKind, AuditOutcome, AuditRowId},
    };

    use super::*;

    fn make_row(kind: &str) -> AuditEventRow {
        AuditEventRow {
            id: AuditRowId("01J".to_owned()),
            prev_hash: audit_prev_hash_seed().to_owned(),
            caddy_instance_id: "local".to_owned(),
            correlation_id: "01J".to_owned(),
            occurred_at: 0,
            occurred_at_ms: 0,
            actor_kind: ActorKind::System,
            actor_id: "test".to_owned(),
            kind: kind.to_owned(),
            target_kind: None,
            target_id: None,
            snapshot_id: None,
            redacted_diff_json: None,
            redaction_sites: 0,
            outcome: AuditOutcome::Ok,
            error_kind: None,
            notes: None,
        }
    }

    #[test]
    fn valid_known_kinds_pass() {
        for kind in trilithon_core::storage::audit_vocab::AUDIT_KINDS {
            assert!(
                validate_kind(&make_row(kind)).is_ok(),
                "expected Ok for known kind {kind}"
            );
        }
    }

    #[test]
    fn unknown_kind_rejected() {
        assert!(
            matches!(
                validate_kind(&make_row("not.a.known.kind")),
                Err(StorageError::AuditKindUnknown { .. })
            ),
            "expected AuditKindUnknown for unknown kind"
        );
    }

    #[test]
    fn pattern_checks() {
        assert!(validate_kind_pattern("auth.login-succeeded").is_ok());
        assert!(validate_kind_pattern("config.applied").is_ok());
        assert!(validate_kind_pattern("mutation.rejected.missing-expected-version").is_ok());
        // Single segment — no dot.
        assert!(validate_kind_pattern("noDot").is_err());
        // Empty segment.
        assert!(validate_kind_pattern("auth.").is_err());
        assert!(validate_kind_pattern(".login").is_err());
        // Uppercase.
        assert!(validate_kind_pattern("Auth.Login").is_err());
        // Digit-start segment.
        assert!(validate_kind_pattern("1auth.login").is_err());
    }
}
