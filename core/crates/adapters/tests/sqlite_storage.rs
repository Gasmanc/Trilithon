//! Integration tests for `SqliteStorage`.
//!
//! Every test uses a `tempdir` so tests are fully isolated from each other
//! and from the development database.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use tempfile::TempDir;
use trilithon_adapters::{
    lock::LockHandle, migrate::apply_migrations, sqlite_storage::SqliteStorage,
};
use trilithon_core::{
    canonical_json::CANONICAL_JSON_VERSION,
    storage::{
        audit_vocab::AUDIT_KINDS,
        error::StorageError,
        trait_def::Storage,
        types::{
            ActorKind, AuditEventRow, AuditOutcome, AuditRowId, AuditSelector, Snapshot, SnapshotId,
        },
    },
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_snapshot(id: &str, version: i64, parent: Option<&str>, body: &str) -> Snapshot {
    Snapshot {
        snapshot_id: SnapshotId(id.to_owned()),
        parent_id: parent.map(|p| SnapshotId(p.to_owned())),
        config_version: version,
        actor: "test".to_owned(),
        intent: "test snapshot".to_owned(),
        correlation_id: "corr-01".to_owned(),
        caddy_version: "2.8.0".to_owned(),
        trilithon_version: "0.1.0".to_owned(),
        created_at_unix_seconds: 1_700_000_000,
        created_at_monotonic_nanos: 0,
        canonical_json_version: CANONICAL_JSON_VERSION,
        desired_state_json: body.to_owned(),
    }
}

fn make_audit_event(kind: &str, correlation_id: &str) -> AuditEventRow {
    AuditEventRow {
        id: AuditRowId(ulid::Ulid::new().to_string()),
        caddy_instance_id: "local".to_owned(),
        correlation_id: correlation_id.to_owned(),
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

async fn open(dir: &TempDir) -> SqliteStorage {
    let store = SqliteStorage::open(dir.path())
        .await
        .expect("SqliteStorage::open should succeed");
    apply_migrations(store.pool())
        .await
        .expect("apply_migrations should succeed");
    store
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// After opening a fresh DB all four pragmas must be in their required state.
#[tokio::test]
async fn pragmas_applied_after_open() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;
    let pool = store.pool();

    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(pool)
        .await
        .expect("PRAGMA journal_mode");
    assert_eq!(journal_mode, "wal", "journal_mode must be WAL");

    let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(pool)
        .await
        .expect("PRAGMA synchronous");
    assert_eq!(synchronous, 1, "synchronous must be NORMAL (1)");

    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(pool)
        .await
        .expect("PRAGMA foreign_keys");
    assert_eq!(foreign_keys, 1, "foreign_keys must be ON");

    let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(pool)
        .await
        .expect("PRAGMA busy_timeout");
    assert_eq!(busy_timeout, 5000, "busy_timeout must be 5000 ms");
}

/// Basic round-trip: insert then get.
#[tokio::test]
async fn insert_get_snapshot_round_trip() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let snap = make_snapshot("aa".repeat(32).as_str(), 1, None, r#"{"routes":[]}"#);
    let id = store
        .insert_snapshot(snap.clone())
        .await
        .expect("insert should succeed");

    let fetched = store
        .get_snapshot(&id)
        .await
        .expect("get should succeed")
        .expect("snapshot should be Some");

    assert_eq!(fetched.config_version, 1);
    assert_eq!(fetched.desired_state_json, r#"{"routes":[]}"#);
    assert_eq!(fetched.actor, "test");
}

/// Inserting the same snapshot twice with the same body is idempotent.
#[tokio::test]
async fn insert_duplicate_same_body_idempotent() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let snap = make_snapshot("bb".repeat(32).as_str(), 1, None, r#"{"routes":[]}"#);
    let id1 = store
        .insert_snapshot(snap.clone())
        .await
        .expect("first insert should succeed");
    let id2 = store
        .insert_snapshot(snap.clone())
        .await
        .expect("second insert (same body) should succeed idempotently");

    assert_eq!(id1, id2, "idempotent insert must return the same id");
}

/// Inserting a snapshot with the same id but a different body must fail.
#[tokio::test]
async fn insert_duplicate_different_body_returns_duplicate_error() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let id = "cc".repeat(32);
    let snap1 = make_snapshot(&id, 1, None, r#"{"routes":[]}"#);
    let snap2 = make_snapshot(&id, 1, None, r#"{"routes":[{"handle":[]}]}"#);

    store
        .insert_snapshot(snap1)
        .await
        .expect("first insert should succeed");

    let err = store
        .insert_snapshot(snap2)
        .await
        .expect_err("insert with different body should fail");

    assert!(
        matches!(err, StorageError::SnapshotDuplicate { .. }),
        "expected SnapshotDuplicate, got {err:?}"
    );
}

/// Recording an event with a known kind succeeds.
#[tokio::test]
async fn record_audit_event_known_kind_succeeds() {
    assert!(
        AUDIT_KINDS.contains(&"config.applied"),
        "config.applied must be in AUDIT_KINDS"
    );

    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let event = make_audit_event("config.applied", "corr-01");
    store
        .record_audit_event(event)
        .await
        .expect("known kind should be accepted");
}

/// Recording an event with an unknown kind is rejected before the INSERT.
#[tokio::test]
async fn record_audit_event_unknown_kind_rejected() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let event = make_audit_event("made.up", "corr-01");
    let err = store
        .record_audit_event(event)
        .await
        .expect_err("unknown kind must be rejected");

    assert!(
        matches!(err, StorageError::AuditKindUnknown { .. }),
        "expected AuditKindUnknown, got {err:?}"
    );
}

/// `tail_audit_log` filters correctly by `correlation_id`.
#[tokio::test]
async fn tail_audit_log_filters_by_correlation_id() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    // Three events: two with corr-A, one with corr-B.
    store
        .record_audit_event(make_audit_event("config.applied", "corr-A"))
        .await
        .expect("e1");
    store
        .record_audit_event(make_audit_event("config.applied", "corr-B"))
        .await
        .expect("e2");
    store
        .record_audit_event(make_audit_event("mutation.submitted", "corr-A"))
        .await
        .expect("e3");

    let rows = store
        .tail_audit_log(
            AuditSelector {
                correlation_id: Some("corr-A".to_owned()),
                ..Default::default()
            },
            100,
        )
        .await
        .expect("tail_audit_log should succeed");

    assert_eq!(rows.len(), 2, "only corr-A rows expected");
    assert!(
        rows.iter().all(|r| r.correlation_id == "corr-A"),
        "all rows must have correlation_id == corr-A"
    );
}

/// The advisory lock rejects a second acquire within the same process.
///
/// Note: on some operating systems advisory locks are per-process rather than
/// per-file-descriptor, so we test `LockHandle::acquire` directly here
/// rather than opening `SqliteStorage` twice.
#[tokio::test]
async fn advisory_lock_rejects_second_open() {
    let dir = TempDir::new().unwrap();

    // First acquire succeeds.
    let _lock1 = LockHandle::acquire(dir.path()).expect("first lock acquire should succeed");

    // Second acquire in the same process must fail.
    let err = LockHandle::acquire(dir.path()).expect_err("second lock acquire must fail");

    assert!(
        matches!(err, trilithon_adapters::lock::LockError::AlreadyHeld { .. }),
        "expected AlreadyHeld, got {err:?}"
    );
}
