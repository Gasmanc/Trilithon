//! Integration test: `since`/`until` half-open interval is honoured.
//!
//! `since` is inclusive; `until` is exclusive.  A row with `occurred_at`
//! exactly equal to `until` must NOT be returned.

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

fn make_row(i: u64, occurred_at: i64) -> AuditEventRow {
    AuditEventRow {
        id: AuditRowId(Ulid::from_parts(1_700_000_000_000 + i, 0).to_string()),
        prev_hash: audit_prev_hash_seed().to_owned(),
        caddy_instance_id: "local".to_owned(),
        correlation_id: Ulid::new().to_string(),
        occurred_at,
        occurred_at_ms: occurred_at * 1_000,
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

/// Time layout (Unix seconds):
///   t=100 — before the window  (excluded by `since`)
///   t=200 — start of window    (included: `since` is inclusive)
///   t=300 — inside the window  (included)
///   t=400 — end boundary row   (excluded: `until` is exclusive)
///   t=500 — after the window   (excluded by `until`)
///
/// `since=200`, `until=400` → rows at t=200 and t=300 must be returned.
#[tokio::test]
async fn time_range_half_open_interval_is_honoured() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let timestamps: &[(u64, i64)] = &[
        (0, 100), // before window
        (1, 200), // window start (inclusive)
        (2, 300), // inside window
        (3, 400), // boundary at `until` (must be excluded)
        (4, 500), // after window
    ];

    for &(i, ts) in timestamps {
        store
            .record_audit_event(make_row(i, ts))
            .await
            .expect("insert must succeed");
    }

    let rows = store
        .tail_audit_log(
            AuditSelector {
                since: Some(200),
                until: Some(400),
                ..Default::default()
            },
            1000,
        )
        .await
        .expect("tail_audit_log must succeed");

    // Exactly the rows at t=200 and t=300.
    assert_eq!(
        rows.len(),
        2,
        "expected 2 rows in [200, 400), got {}: {:?}",
        rows.len(),
        rows.iter().map(|r| r.occurred_at).collect::<Vec<_>>()
    );

    let mut times: Vec<i64> = rows.iter().map(|r| r.occurred_at).collect();
    times.sort_unstable();
    assert_eq!(
        times,
        vec![200, 300],
        "returned rows must have occurred_at 200 and 300"
    );
}

/// A query with only `since` returns all rows from that timestamp onwards.
#[tokio::test]
async fn since_only_returns_from_lower_bound_inclusive() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    for (i, ts) in [(0u64, 100i64), (1, 200), (2, 300)] {
        store
            .record_audit_event(make_row(i, ts))
            .await
            .expect("insert must succeed");
    }

    let rows = store
        .tail_audit_log(
            AuditSelector {
                since: Some(200),
                ..Default::default()
            },
            1000,
        )
        .await
        .expect("tail_audit_log must succeed");

    assert_eq!(
        rows.len(),
        2,
        "since=200 must return 2 rows (200, 300), got {}",
        rows.len()
    );
}

/// A query with only `until` returns all rows strictly before that timestamp.
#[tokio::test]
async fn until_only_excludes_boundary_row() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    for (i, ts) in [(0u64, 100i64), (1, 200), (2, 300)] {
        store
            .record_audit_event(make_row(i, ts))
            .await
            .expect("insert must succeed");
    }

    let rows = store
        .tail_audit_log(
            AuditSelector {
                until: Some(200),
                ..Default::default()
            },
            1000,
        )
        .await
        .expect("tail_audit_log must succeed");

    // `until=200` is exclusive — only the row at t=100 must be returned.
    assert_eq!(
        rows.len(),
        1,
        "until=200 (exclusive) must return 1 row (t=100), got {}",
        rows.len()
    );
    assert_eq!(
        rows[0].occurred_at, 100,
        "only the t=100 row must be returned"
    );
}
