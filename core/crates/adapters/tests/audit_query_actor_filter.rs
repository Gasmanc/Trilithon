//! Integration test: `actor_id` filter returns only matching rows.
//!
//! Inserts rows with two different actor ids; asserts that filtering by
//! one actor id returns only those rows.

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

fn make_row(i: u64, actor_id: &str) -> AuditEventRow {
    AuditEventRow {
        id: AuditRowId(Ulid::from_parts(1_700_000_000_000 + i, 0).to_string()),
        prev_hash: audit_prev_hash_seed().to_owned(),
        caddy_instance_id: "local".to_owned(),
        correlation_id: Ulid::new().to_string(),
        occurred_at: 1_700_000_000 + i as i64,
        occurred_at_ms: (1_700_000_000_000 + i) as i64,
        actor_kind: ActorKind::User,
        actor_id: actor_id.to_owned(),
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
async fn actor_filter_returns_only_matching_rows() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    // 4 rows for alice, 3 rows for bob.
    for i in 0..4_u64 {
        store
            .record_audit_event(make_row(i, "alice"))
            .await
            .expect("insert must succeed");
    }
    for i in 4..7_u64 {
        store
            .record_audit_event(make_row(i, "bob"))
            .await
            .expect("insert must succeed");
    }

    let alice_rows = store
        .tail_audit_log(
            AuditSelector {
                actor_id: Some("alice".to_owned()),
                ..Default::default()
            },
            1000,
        )
        .await
        .expect("tail_audit_log must succeed");

    assert_eq!(
        alice_rows.len(),
        4,
        "actor filter for 'alice' must return 4 rows, got {}",
        alice_rows.len()
    );
    for row in &alice_rows {
        assert_eq!(
            row.actor_id, "alice",
            "all returned rows must have actor_id 'alice'"
        );
    }

    let bob_rows = store
        .tail_audit_log(
            AuditSelector {
                actor_id: Some("bob".to_owned()),
                ..Default::default()
            },
            1000,
        )
        .await
        .expect("tail_audit_log must succeed");

    assert_eq!(
        bob_rows.len(),
        3,
        "actor filter for 'bob' must return 3 rows, got {}",
        bob_rows.len()
    );
}
