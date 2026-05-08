//! Integration test: `correlation_id` filter returns only matching rows.
//!
//! Inserts 5 rows total: 3 share one correlation id, 2 use different ids.
//! Asserts that filtering by the shared correlation id returns exactly 3 rows.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::cast_possible_wrap
)]
// reason: test-only code; panics are the correct failure mode in tests; casts
// are bounded by test-controlled values that cannot overflow i64

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

fn make_row(i: u64, correlation_id: &str) -> AuditEventRow {
    AuditEventRow {
        id: AuditRowId(Ulid::from_parts(1_700_000_000_000 + i, 0).to_string()),
        prev_hash: audit_prev_hash_seed().to_owned(),
        caddy_instance_id: "local".to_owned(),
        correlation_id: correlation_id.to_owned(),
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
async fn correlation_filter_returns_only_matching_rows() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let shared_corr = Ulid::new().to_string();
    let other_corr_a = Ulid::new().to_string();
    let other_corr_b = Ulid::new().to_string();

    // Three rows share the target correlation id.
    for i in 0..3_u64 {
        store
            .record_audit_event(make_row(i, &shared_corr))
            .await
            .expect("insert must succeed");
    }
    // Two rows use different correlation ids.
    store
        .record_audit_event(make_row(3, &other_corr_a))
        .await
        .expect("insert must succeed");
    store
        .record_audit_event(make_row(4, &other_corr_b))
        .await
        .expect("insert must succeed");

    let rows = store
        .tail_audit_log(
            AuditSelector {
                correlation_id: Some(shared_corr.clone()),
                ..Default::default()
            },
            1000,
        )
        .await
        .expect("tail_audit_log must succeed");

    assert_eq!(
        rows.len(),
        3,
        "correlation filter must return exactly 3 rows, got {}",
        rows.len()
    );
    for row in &rows {
        assert_eq!(
            row.correlation_id, shared_corr,
            "all returned rows must have the target correlation_id"
        );
    }
}
