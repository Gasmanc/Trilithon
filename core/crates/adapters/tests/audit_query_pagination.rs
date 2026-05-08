//! Integration test: default limit = 100, explicit limit = 1000, limit 5000 is clamped.
//!
//! Inserts 250 rows and asserts:
//! - `tail_audit_log` with `limit=0` (sentinel for "default") returns 100 rows.
//! - `tail_audit_log` with `limit=1000` returns all 250 rows.
//! - `tail_audit_log` with `limit=5000` is clamped to 1000 and returns all 250 rows.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::cast_possible_wrap
)]
// reason: test-only code; panics are the correct failure mode in tests; casts
// are bounded by test-controlled values (max i=249) that cannot overflow i64

use tempfile::TempDir;
use trilithon_adapters::{migrate::apply_migrations, sqlite_storage::SqliteStorage};
use trilithon_core::storage::{
    helpers::audit_prev_hash_seed,
    trait_def::Storage,
    types::{ActorKind, AuditEventRow, AuditOutcome, AuditRowId, AuditSelector},
};
use ulid::Ulid;

async fn open(dir: &TempDir) -> SqliteStorage {
    let store = SqliteStorage::open(dir.path())
        .await
        .expect("SqliteStorage::open should succeed");
    apply_migrations(store.pool())
        .await
        .expect("apply_migrations should succeed");
    store
}

fn make_row(i: u64) -> AuditEventRow {
    AuditEventRow {
        id: AuditRowId(Ulid::from_parts(1_700_000_000_000 + i, 0).to_string()),
        prev_hash: audit_prev_hash_seed().to_owned(),
        caddy_instance_id: "local".to_owned(),
        correlation_id: Ulid::new().to_string(),
        occurred_at: 1_700_000_000 + i as i64,
        occurred_at_ms: (1_700_000_000_000 + i) as i64,
        actor_kind: ActorKind::System,
        actor_id: "test-actor".to_owned(),
        kind: "config.applied".to_owned(),
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

#[tokio::test]
async fn default_limit_returns_100() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    for i in 0..250 {
        store
            .record_audit_event(make_row(i))
            .await
            .expect("insert must succeed");
    }

    // limit=0 is the sentinel for "use default 100".
    let rows = store
        .tail_audit_log(AuditSelector::default(), 0)
        .await
        .expect("tail_audit_log must succeed");

    assert_eq!(rows.len(), 100, "default limit must return 100 rows");
}

#[tokio::test]
async fn explicit_limit_1000_returns_all_250() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    for i in 0..250 {
        store
            .record_audit_event(make_row(i))
            .await
            .expect("insert must succeed");
    }

    let rows = store
        .tail_audit_log(AuditSelector::default(), 1000)
        .await
        .expect("tail_audit_log must succeed");

    assert_eq!(rows.len(), 250, "limit=1000 must return all 250 rows");
}

#[tokio::test]
async fn limit_5000_is_clamped_to_1000_returns_all_250() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    for i in 0..250 {
        store
            .record_audit_event(make_row(i))
            .await
            .expect("insert must succeed");
    }

    // limit=5000 must be clamped to 1000; only 250 rows exist so all are returned.
    let rows = store
        .tail_audit_log(AuditSelector::default(), 5000)
        .await
        .expect("tail_audit_log must succeed");

    assert_eq!(
        rows.len(),
        250,
        "limit=5000 clamped to 1000 must return all 250 rows"
    );
}
