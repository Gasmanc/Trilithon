//! Integration test: kind validation via `Storage::record_audit_event`.
//!
//! An `AuditEventRow` whose `kind` is not in the §6.6 vocabulary must be
//! rejected with `StorageError::AuditKindUnknown` before any database write.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use tempfile::TempDir;
use trilithon_adapters::{migrate::apply_migrations, sqlite_storage::SqliteStorage};
use trilithon_core::storage::{
    error::StorageError,
    helpers::audit_prev_hash_seed,
    trait_def::Storage,
    types::{ActorKind, AuditEventRow, AuditOutcome, AuditRowId},
};

async fn open(dir: &TempDir) -> SqliteStorage {
    let store = SqliteStorage::open(dir.path())
        .await
        .expect("SqliteStorage::open should succeed");
    apply_migrations(store.pool())
        .await
        .expect("apply_migrations should succeed");
    store
}

/// Build an `AuditEventRow` with the given kind string.
fn row_with_kind(kind: &str) -> AuditEventRow {
    AuditEventRow {
        id: AuditRowId(ulid::Ulid::new().to_string()),
        prev_hash: audit_prev_hash_seed().to_owned(),
        caddy_instance_id: "local".to_owned(),
        correlation_id: ulid::Ulid::new().to_string(),
        occurred_at: 1_700_000_000,
        occurred_at_ms: 1_700_000_000_000,
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

/// A row with a kind string that is syntactically valid but not in the §6.6
/// vocabulary must be rejected with `StorageError::AuditKindUnknown`.
#[tokio::test]
async fn unknown_kind_returns_audit_kind_unknown() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    // "not.in.vocab" matches the dotted-kind regex but is not in AUDIT_KINDS.
    let event = row_with_kind("not.in.vocab");
    let err = store
        .record_audit_event(event)
        .await
        .expect_err("unknown kind must be rejected");

    assert!(
        matches!(err, StorageError::AuditKindUnknown { .. }),
        "expected AuditKindUnknown, got {err:?}"
    );
}

/// A row with a kind string that violates the §6.6 pattern entirely must also
/// be rejected with `StorageError::AuditKindUnknown`.
#[tokio::test]
async fn malformed_kind_returns_audit_kind_unknown() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    // "INVALID" — no dot, uppercase — both vocabulary and pattern checks fail.
    let event = row_with_kind("INVALID");
    let err = store
        .record_audit_event(event)
        .await
        .expect_err("malformed kind must be rejected");

    assert!(
        matches!(err, StorageError::AuditKindUnknown { .. }),
        "expected AuditKindUnknown for malformed kind, got {err:?}"
    );
}

/// A row with a valid §6.6 kind must be accepted by the storage adapter.
#[tokio::test]
async fn valid_kind_accepted() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let event = row_with_kind("config.applied");
    store
        .record_audit_event(event)
        .await
        .expect("valid kind must be accepted");
}
