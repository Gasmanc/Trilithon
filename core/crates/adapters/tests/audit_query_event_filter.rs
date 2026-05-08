//! Integration test: `kind_glob` filter returns only rows matching the event kind.
//!
//! Inserts rows with two different event kinds; asserts that filtering by
//! `kind_glob = "config.applied"` returns only those rows.

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

fn make_row(i: u64, kind: &str) -> AuditEventRow {
    AuditEventRow {
        id: AuditRowId(Ulid::from_parts(1_700_000_000_000 + i, 0).to_string()),
        prev_hash: audit_prev_hash_seed().to_owned(),
        caddy_instance_id: "local".to_owned(),
        correlation_id: Ulid::new().to_string(),
        occurred_at: 1_700_000_000 + i as i64,
        occurred_at_ms: (1_700_000_000_000 + i) as i64,
        actor_kind: ActorKind::System,
        actor_id: "test-actor".to_owned(),
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

#[tokio::test]
async fn kind_glob_exact_returns_only_matching_rows() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    // 3 rows with `config.applied`, 2 rows with `config.apply-failed`.
    for i in 0..3_u64 {
        store
            .record_audit_event(make_row(i, "config.applied"))
            .await
            .expect("insert must succeed");
    }
    for i in 3..5_u64 {
        store
            .record_audit_event(make_row(i, "config.apply-failed"))
            .await
            .expect("insert must succeed");
    }

    let rows = store
        .tail_audit_log(
            AuditSelector {
                kind_glob: Some("config.applied".to_owned()),
                ..Default::default()
            },
            1000,
        )
        .await
        .expect("tail_audit_log must succeed");

    assert_eq!(
        rows.len(),
        3,
        "exact kind filter must return exactly 3 rows, got {}",
        rows.len()
    );
    for row in &rows {
        assert_eq!(
            row.kind, "config.applied",
            "all returned rows must have kind 'config.applied'"
        );
    }
}

#[tokio::test]
async fn kind_glob_prefix_wildcard_matches_all_config_rows() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    // 3 rows with config.* kinds, 2 rows with mutation.*.
    store
        .record_audit_event(make_row(0, "config.applied"))
        .await
        .expect("insert must succeed");
    store
        .record_audit_event(make_row(1, "config.apply-failed"))
        .await
        .expect("insert must succeed");
    store
        .record_audit_event(make_row(2, "config.drift-detected"))
        .await
        .expect("insert must succeed");
    store
        .record_audit_event(make_row(3, "mutation.applied"))
        .await
        .expect("insert must succeed");
    store
        .record_audit_event(make_row(4, "mutation.rejected"))
        .await
        .expect("insert must succeed");

    let rows = store
        .tail_audit_log(
            AuditSelector {
                kind_glob: Some("config.*".to_owned()),
                ..Default::default()
            },
            1000,
        )
        .await
        .expect("tail_audit_log must succeed");

    assert_eq!(
        rows.len(),
        3,
        "config.* glob must return exactly 3 config rows, got {}",
        rows.len()
    );
    for row in &rows {
        assert!(
            row.kind.starts_with("config."),
            "all returned rows must have kind starting with 'config.', got {:?}",
            row.kind
        );
    }
}
