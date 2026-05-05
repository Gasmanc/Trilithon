//! Integration tests for `SnapshotWriter` — deduplication, parent enforcement,
//! monotonicity, body equality on hash match, and all fetch shapes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use tempfile::TempDir;
use trilithon_adapters::{
    migrate::apply_migrations,
    sqlite_storage::{SnapshotDateRange, SqliteStorage},
};
use trilithon_core::{
    canonical_json::CANONICAL_JSON_VERSION,
    storage::{
        error::StorageError,
        trait_def::Storage,
        types::{Snapshot, SnapshotId},
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

fn make_snapshot_with_ts(
    id: &str,
    version: i64,
    parent: Option<&str>,
    body: &str,
    ts: i64,
) -> Snapshot {
    Snapshot {
        snapshot_id: SnapshotId(id.to_owned()),
        parent_id: parent.map(|p| SnapshotId(p.to_owned())),
        config_version: version,
        actor: "test".to_owned(),
        intent: "test snapshot".to_owned(),
        correlation_id: "corr-01".to_owned(),
        caddy_version: "2.8.0".to_owned(),
        trilithon_version: "0.1.0".to_owned(),
        created_at_unix_seconds: ts,
        created_at_monotonic_nanos: 0,
        canonical_json_version: CANONICAL_JSON_VERSION,
        desired_state_json: body.to_owned(),
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
// Write + fetch-back
// ---------------------------------------------------------------------------

/// Basic round-trip: write a snapshot and fetch it back by id.
#[tokio::test]
async fn write_and_fetch_by_id() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let id = "aa".repeat(32);
    let snap = make_snapshot(&id, 1, None, r#"{"routes":[]}"#);
    store
        .insert_snapshot(snap)
        .await
        .expect("insert should succeed");

    let fetched = store
        .get_snapshot(&SnapshotId(id.clone()))
        .await
        .expect("get should succeed")
        .expect("snapshot should be Some");

    assert_eq!(fetched.snapshot_id.0, id);
    assert_eq!(fetched.config_version, 1);
    assert_eq!(fetched.desired_state_json, r#"{"routes":[]}"#);
}

// ---------------------------------------------------------------------------
// Deduplication
// ---------------------------------------------------------------------------

/// Second write with the same id and same body is idempotent — returns the
/// existing id without inserting a duplicate row.
#[tokio::test]
async fn deduplication_same_body_idempotent() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let id = "bb".repeat(32);
    let snap = make_snapshot(&id, 1, None, r#"{"routes":[]}"#);

    let id1 = store
        .insert_snapshot(snap.clone())
        .await
        .expect("first insert should succeed");
    let id2 = store
        .insert_snapshot(snap)
        .await
        .expect("second insert (same body) must be idempotent");

    assert_eq!(id1, id2, "idempotent insert must return the same id");
}

/// Body equality check on hash match: same id, different body triggers
/// `SnapshotHashCollision` (forced-collision path — sha-256 collision).
///
/// In production this would require a genuine hash collision; here we inject
/// one by constructing a second snapshot with the same 64-char hex id but
/// different `desired_state_json`.  The writer MUST detect the mismatch and
/// return the collision error rather than silently overwriting.
#[tokio::test]
async fn body_equality_check_hash_collision_detected() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let id = "cc".repeat(32);
    let snap1 = make_snapshot(&id, 1, None, r#"{"routes":[]}"#);
    // Same id (the "hash"), different body — simulates a forced SHA-256 collision.
    let snap2 = make_snapshot(&id, 2, None, r#"{"routes":[{"handle":[]}]}"#);

    store
        .insert_snapshot(snap1)
        .await
        .expect("first insert should succeed");

    let err = store
        .insert_snapshot(snap2)
        .await
        .expect_err("second insert with different body must fail");

    assert!(
        matches!(err, StorageError::SnapshotHashCollision { .. }),
        "expected SnapshotHashCollision, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Parent enforcement
// ---------------------------------------------------------------------------

/// Inserting a snapshot with a `parent_id` that does not exist must fail.
#[tokio::test]
async fn parent_enforcement_nonexistent_parent_rejected() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let ghost_parent = "dd".repeat(32);
    let snap = make_snapshot(&"ee".repeat(32), 1, Some(&ghost_parent), r#"{"routes":[]}"#);

    let err = store
        .insert_snapshot(snap)
        .await
        .expect_err("insert with missing parent must fail");

    assert!(
        matches!(err, StorageError::SnapshotParentNotFound { .. }),
        "expected SnapshotParentNotFound, got {err:?}"
    );
}

/// Inserting a snapshot with a valid `parent_id` succeeds.
#[tokio::test]
async fn parent_enforcement_valid_parent_accepted() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let parent_id = "ff".repeat(32);
    let child_id = "00".repeat(32);

    let parent = make_snapshot(&parent_id, 1, None, r#"{"routes":[]}"#);
    store
        .insert_snapshot(parent)
        .await
        .expect("parent insert should succeed");

    let child = make_snapshot(&child_id, 2, Some(&parent_id), r#"{"routes":[{}]}"#);
    store
        .insert_snapshot(child)
        .await
        .expect("child insert with valid parent should succeed");
}

// ---------------------------------------------------------------------------
// Monotonicity enforcement
// ---------------------------------------------------------------------------

/// `config_version` must be strictly greater than the current maximum.
/// Inserting with an equal `config_version` must fail.
#[tokio::test]
async fn monotonicity_equal_version_rejected() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let snap1 = make_snapshot(&"11".repeat(32), 5, None, r#"{"routes":[]}"#);
    store
        .insert_snapshot(snap1)
        .await
        .expect("first insert should succeed");

    // Same config_version — must be rejected.
    let snap2 = make_snapshot(&"22".repeat(32), 5, None, r#"{"routes":[{}]}"#);
    let err = store
        .insert_snapshot(snap2)
        .await
        .expect_err("equal config_version must be rejected");

    assert!(
        matches!(err, StorageError::SnapshotVersionNotMonotonic { .. }),
        "expected SnapshotVersionNotMonotonic, got {err:?}"
    );
}

/// `config_version` less than the current maximum must also fail.
#[tokio::test]
async fn monotonicity_lower_version_rejected() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let snap1 = make_snapshot(&"33".repeat(32), 10, None, r#"{"routes":[]}"#);
    store
        .insert_snapshot(snap1)
        .await
        .expect("first insert should succeed");

    let snap2 = make_snapshot(&"44".repeat(32), 9, None, r#"{"routes":[{}]}"#);
    let err = store
        .insert_snapshot(snap2)
        .await
        .expect_err("lower config_version must be rejected");

    assert!(
        matches!(err, StorageError::SnapshotVersionNotMonotonic { .. }),
        "expected SnapshotVersionNotMonotonic, got {err:?}"
    );
}

/// `config_version` strictly greater than the current maximum is accepted.
#[tokio::test]
async fn monotonicity_higher_version_accepted() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let snap1 = make_snapshot(&"55".repeat(32), 1, None, r#"{"routes":[]}"#);
    store
        .insert_snapshot(snap1)
        .await
        .expect("first insert should succeed");

    let snap2 = make_snapshot(&"66".repeat(32), 2, None, r#"{"routes":[{}]}"#);
    store
        .insert_snapshot(snap2)
        .await
        .expect("strictly greater config_version must be accepted");
}

// ---------------------------------------------------------------------------
// Fetch by config_version
// ---------------------------------------------------------------------------

/// `fetch_by_config_version` returns the snapshot at the given version.
#[tokio::test]
async fn fetch_by_config_version_found() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let id = "77".repeat(32);
    let snap = make_snapshot(&id, 42, None, r#"{"routes":[]}"#);
    store
        .insert_snapshot(snap)
        .await
        .expect("insert should succeed");

    let results = store
        .fetch_by_config_version(42)
        .await
        .expect("fetch_by_config_version should succeed");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].snapshot_id.0, id);
    assert_eq!(results[0].config_version, 42);
}

/// `fetch_by_config_version` returns an empty vec when version does not exist.
#[tokio::test]
async fn fetch_by_config_version_not_found() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let results = store
        .fetch_by_config_version(999)
        .await
        .expect("fetch_by_config_version should succeed");

    assert!(
        results.is_empty(),
        "expected empty result for unknown version"
    );
}

// ---------------------------------------------------------------------------
// Fetch by parent_id
// ---------------------------------------------------------------------------

/// `fetch_by_parent_id` returns direct children of the given snapshot.
#[tokio::test]
async fn fetch_by_parent_id_returns_children() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let parent_id = "88".repeat(32);
    let child1_id = "89".repeat(32);
    let child2_id = "8a".repeat(32);

    // In this test we use a single linear chain since the unique index on
    // (caddy_instance_id, config_version) prevents two rows at the same
    // version.  We insert parent at v1, child1 at v2, then fetch by
    // parent_id to verify child1 is returned.
    store
        .insert_snapshot(make_snapshot(&parent_id, 1, None, r#"{"a":1}"#))
        .await
        .expect("parent insert");
    store
        .insert_snapshot(make_snapshot(&child1_id, 2, Some(&parent_id), r#"{"a":2}"#))
        .await
        .expect("child1 insert");

    // child2 has a different parent so it does not appear.
    // (Use a genesis snapshot to avoid monotonicity issues — we need a fresh store.)
    // Instead, just verify the one child we have.
    let _ = child2_id; // not inserted here

    let children = store
        .fetch_by_parent_id(&SnapshotId(parent_id.clone()))
        .await
        .expect("fetch_by_parent_id should succeed");

    assert_eq!(children.len(), 1);
    assert_eq!(children[0].snapshot_id.0, child1_id);
    assert_eq!(
        children[0].parent_id.as_ref().map(|p| &p.0),
        Some(&parent_id)
    );
}

/// `fetch_by_parent_id` returns empty vec when no children exist.
#[tokio::test]
async fn fetch_by_parent_id_no_children() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let id = "99".repeat(32);
    store
        .insert_snapshot(make_snapshot(&id, 1, None, r#"{"routes":[]}"#))
        .await
        .expect("insert should succeed");

    // No children inserted — result must be empty.
    let children = store
        .fetch_by_parent_id(&SnapshotId(id))
        .await
        .expect("fetch_by_parent_id should succeed");

    assert!(children.is_empty(), "expected no children");
}

// ---------------------------------------------------------------------------
// Fetch by date range
// ---------------------------------------------------------------------------

/// `fetch_by_date_range` with `since` and `until` filters correctly.
#[tokio::test]
async fn fetch_by_date_range_filters() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    // Three snapshots at different timestamps.
    store
        .insert_snapshot(make_snapshot_with_ts(
            &"a1".repeat(32),
            1,
            None,
            r#"{"v":1}"#,
            1_000,
        ))
        .await
        .expect("s1 insert");
    store
        .insert_snapshot(make_snapshot_with_ts(
            &"a2".repeat(32),
            2,
            None,
            r#"{"v":2}"#,
            2_000,
        ))
        .await
        .expect("s2 insert");
    store
        .insert_snapshot(make_snapshot_with_ts(
            &"a3".repeat(32),
            3,
            None,
            r#"{"v":3}"#,
            3_000,
        ))
        .await
        .expect("s3 insert");

    // Range [1000, 2000] should return the first two.
    let results = store
        .fetch_by_date_range(&SnapshotDateRange {
            since: Some(1_000),
            until: Some(2_000),
        })
        .await
        .expect("fetch_by_date_range should succeed");

    assert_eq!(
        results.len(),
        2,
        "expected snapshots at ts=1000 and ts=2000"
    );
    assert!(results.iter().any(|s| s.config_version == 1));
    assert!(results.iter().any(|s| s.config_version == 2));
    assert!(
        !results.iter().any(|s| s.config_version == 3),
        "ts=3000 must be excluded"
    );
}

/// `fetch_by_date_range` with no bounds returns all snapshots.
#[tokio::test]
async fn fetch_by_date_range_no_bounds_returns_all() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    store
        .insert_snapshot(make_snapshot_with_ts(
            &"b1".repeat(32),
            1,
            None,
            r#"{"v":1}"#,
            100,
        ))
        .await
        .expect("s1 insert");
    store
        .insert_snapshot(make_snapshot_with_ts(
            &"b2".repeat(32),
            2,
            None,
            r#"{"v":2}"#,
            200,
        ))
        .await
        .expect("s2 insert");

    let results = store
        .fetch_by_date_range(&SnapshotDateRange::default())
        .await
        .expect("fetch_by_date_range should succeed");

    assert_eq!(results.len(), 2, "expected all snapshots with no bounds");
}

// ---------------------------------------------------------------------------
// Immutability (migration 0004 — ADR-0009)
// ---------------------------------------------------------------------------

/// UPDATE on a snapshots row must be rejected by the database-level trigger.
#[tokio::test]
async fn immutability_update_rejected() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let id = "c1".repeat(32);
    store
        .insert_snapshot(make_snapshot(&id, 1, None, r#"{"routes":[]}"#))
        .await
        .expect("insert should succeed");

    let result = sqlx::query("UPDATE snapshots SET intent = 'tampered' WHERE id = ?")
        .bind(&id)
        .execute(store.pool())
        .await;

    assert!(
        result.is_err(),
        "UPDATE on snapshots must be rejected by immutability trigger"
    );
}

/// DELETE on a snapshots row must be rejected by the database-level trigger.
#[tokio::test]
async fn immutability_delete_rejected() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let id = "c2".repeat(32);
    store
        .insert_snapshot(make_snapshot(&id, 1, None, r#"{"routes":[]}"#))
        .await
        .expect("insert should succeed");

    let result = sqlx::query("DELETE FROM snapshots WHERE id = ?")
        .bind(&id)
        .execute(store.pool())
        .await;

    assert!(
        result.is_err(),
        "DELETE on snapshots must be rejected by immutability trigger"
    );
}
