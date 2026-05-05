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
    mutation::types::content_address,
    storage::{
        error::StorageError,
        trait_def::Storage,
        types::{Snapshot, SnapshotId},
    },
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a snapshot whose `snapshot_id` is the SHA-256 of `body`.
fn make_snapshot_with_ts(version: i64, parent_body: Option<&str>, body: &str, ts: i64) -> Snapshot {
    let id = content_address(body.as_bytes());
    let parent_id = parent_body.map(|pb| SnapshotId(content_address(pb.as_bytes())));
    Snapshot {
        snapshot_id: SnapshotId(id),
        parent_id,
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

fn make_snapshot(version: i64, parent_body: Option<&str>, body: &str) -> Snapshot {
    make_snapshot_with_ts(version, parent_body, body, 1_700_000_000)
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

    let body = r#"{"routes":[]}"#;
    let expected_id = content_address(body.as_bytes());
    let snap = make_snapshot(1, None, body);
    store
        .insert_snapshot(snap)
        .await
        .expect("insert should succeed");

    let fetched = store
        .get_snapshot(&SnapshotId(expected_id.clone()))
        .await
        .expect("get should succeed")
        .expect("snapshot should be Some");

    assert_eq!(fetched.snapshot_id.0, expected_id);
    assert_eq!(fetched.config_version, 1);
    assert_eq!(fetched.desired_state_json, body);
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

    let body = r#"{"routes":[]}"#;
    let snap = make_snapshot(1, None, body);

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
/// `SnapshotHashCollision` (forced-collision path — simulates a SHA-256 collision).
///
/// We inject a fake row via raw SQL (bypassing validation), then attempt to
/// insert a snapshot whose body hashes to the same id — the writer MUST detect
/// the body mismatch and return the collision error rather than silently overwriting.
#[tokio::test]
async fn body_equality_check_hash_collision_detected() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let body_a = r#"{"routes":[]}"#;
    // Compute the real content-address for body_a.
    let id_a = content_address(body_a.as_bytes());

    // Inject a row with id_a but a DIFFERENT body directly into the DB,
    // bypassing the validation layer (this simulates a pre-existing corrupt row
    // or a genuine SHA-256 collision that slipped through an older code path).
    let different_body = r#"{"injected":true}"#;
    sqlx::query(
        r"INSERT INTO snapshots
            (id, parent_id, caddy_instance_id, actor_kind, actor_id,
             intent, correlation_id, caddy_version, trilithon_version,
             created_at, created_at_ms, config_version, desired_state_json)
          VALUES (?, NULL, 'local', 'system', 'test', 'intent', 'corr-01',
                  '2.8.0', '0.1.0', 1700000000, 1700000000000, 1, ?)",
    )
    .bind(&id_a)
    .bind(different_body)
    .execute(store.pool())
    .await
    .expect("raw SQL insert of fake row should succeed");

    // Now try to insert via the normal path with the correct body for id_a.
    // The writer finds id_a in the DB with a DIFFERENT body → SnapshotHashCollision.
    let snap = make_snapshot(2, None, body_a);
    let err = store
        .insert_snapshot(snap)
        .await
        .expect_err("insert with colliding id must fail");

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

    // The parent body is never inserted — only its content-addressed id appears
    // as the parent_id of the child snapshot.
    let ghost_parent_body = r#"{"ghost":true}"#;
    let child_body = r#"{"routes":[]}"#;
    let snap = make_snapshot(1, Some(ghost_parent_body), child_body);

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

    let parent_body = r#"{"routes":[]}"#;
    let child_body = r#"{"routes":[{}]}"#;

    let parent = make_snapshot(1, None, parent_body);
    store
        .insert_snapshot(parent)
        .await
        .expect("parent insert should succeed");

    let child = make_snapshot(2, Some(parent_body), child_body);
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

    let snap1 = make_snapshot(5, None, r#"{"routes":[]}"#);
    store
        .insert_snapshot(snap1)
        .await
        .expect("first insert should succeed");

    // Same config_version — must be rejected.
    let snap2 = make_snapshot(5, None, r#"{"routes":[{}]}"#);
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

    let snap1 = make_snapshot(10, None, r#"{"routes":[]}"#);
    store
        .insert_snapshot(snap1)
        .await
        .expect("first insert should succeed");

    let snap2 = make_snapshot(9, None, r#"{"routes":[{}]}"#);
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

    let snap1 = make_snapshot(1, None, r#"{"routes":[]}"#);
    store
        .insert_snapshot(snap1)
        .await
        .expect("first insert should succeed");

    let snap2 = make_snapshot(2, None, r#"{"routes":[{}]}"#);
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

    let body = r#"{"routes":[]}"#;
    let expected_id = content_address(body.as_bytes());
    let snap = make_snapshot(42, None, body);
    store
        .insert_snapshot(snap)
        .await
        .expect("insert should succeed");

    let results = store
        .fetch_by_config_version(42)
        .await
        .expect("fetch_by_config_version should succeed");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].snapshot_id.0, expected_id);
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

    let parent_body = r#"{"a":1}"#;
    let child1_body = r#"{"a":2}"#;

    let parent_id = content_address(parent_body.as_bytes());
    let child1_id = content_address(child1_body.as_bytes());

    store
        .insert_snapshot(make_snapshot(1, None, parent_body))
        .await
        .expect("parent insert");
    store
        .insert_snapshot(make_snapshot(2, Some(parent_body), child1_body))
        .await
        .expect("child1 insert");

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

    let body = r#"{"routes":[]}"#;
    let id = content_address(body.as_bytes());
    store
        .insert_snapshot(make_snapshot(1, None, body))
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
        .insert_snapshot(make_snapshot_with_ts(1, None, r#"{"v":1}"#, 1_000))
        .await
        .expect("s1 insert");
    store
        .insert_snapshot(make_snapshot_with_ts(2, None, r#"{"v":2}"#, 2_000))
        .await
        .expect("s2 insert");
    store
        .insert_snapshot(make_snapshot_with_ts(3, None, r#"{"v":3}"#, 3_000))
        .await
        .expect("s3 insert");

    // Range [1000, 2000] should return the first two.
    let results = store
        .fetch_by_date_range(&SnapshotDateRange {
            since: Some(1_000),
            until: Some(2_000),
            ..Default::default()
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
        .insert_snapshot(make_snapshot_with_ts(1, None, r#"{"v":1}"#, 100))
        .await
        .expect("s1 insert");
    store
        .insert_snapshot(make_snapshot_with_ts(2, None, r#"{"v":2}"#, 200))
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

    let body = r#"{"routes":[]}"#;
    let id = content_address(body.as_bytes());
    store
        .insert_snapshot(make_snapshot(1, None, body))
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

    let body = r#"{"routes":[]}"#;
    let id = content_address(body.as_bytes());
    store
        .insert_snapshot(make_snapshot(1, None, body))
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

// ---------------------------------------------------------------------------
// Root snapshot NULL parent
// ---------------------------------------------------------------------------

/// The very first snapshot for an instance MUST have `parent_id IS NULL`.
/// Subsequent snapshots MUST have a non-null `parent_id`.
#[tokio::test]
async fn root_snapshot_has_null_parent() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    let root_body = r#"{"routes":[]}"#;
    let root_id = content_address(root_body.as_bytes());

    store
        .insert_snapshot(make_snapshot(1, None, root_body))
        .await
        .expect("root insert should succeed");

    let root = store
        .get_snapshot(&SnapshotId(root_id.clone()))
        .await
        .expect("get_snapshot should succeed")
        .expect("root snapshot should be Some");

    assert!(
        root.parent_id.is_none(),
        "first snapshot must have parent_id IS NULL, got {:?}",
        root.parent_id
    );

    // Second snapshot — parent is the root.
    let child_body = r#"{"routes":[{}]}"#;
    let child_id = content_address(child_body.as_bytes());
    store
        .insert_snapshot(make_snapshot(2, Some(root_body), child_body))
        .await
        .expect("child insert should succeed");

    let child = store
        .get_snapshot(&SnapshotId(child_id.clone()))
        .await
        .expect("get_snapshot should succeed")
        .expect("child snapshot should be Some");

    assert!(
        child.parent_id.is_some(),
        "subsequent snapshot must have a non-null parent_id"
    );
    assert_eq!(
        child.parent_id.as_ref().map(|p| &p.0),
        Some(&root_id),
        "child parent_id must point to root"
    );
}

// ---------------------------------------------------------------------------
// Monotonicity property tests (loop-based, no proptest dependency)
// ---------------------------------------------------------------------------

/// Property: strict monotonic increase of `config_version` per `caddy_instance_id`.
///
/// Simulates N sequential writes (each must succeed) and asserts that after each
/// insertion the `config_version` is strictly greater than all previous versions.
///
/// The test also verifies that out-of-order or equal versions are correctly
/// rejected even when interleaved with valid writes.
mod props {
    use super::*;

    /// Verify strict monotonic increase across N sequential writes to the same
    /// `caddy_instance_id` ('local' — the fixed value used by `SqliteStorage`).
    #[tokio::test]
    async fn monotonic_version() {
        const N: usize = 30;

        let dir = TempDir::new().unwrap();
        let store = open(&dir).await;

        // Phase 1: Insert N snapshots with strictly increasing versions.
        // Each body is unique so that each content-address is distinct.
        let mut last_version: i64 = 0;
        let mut prev_body: Option<String> = None;
        for i in 1..=N {
            let version = i64::try_from(i).unwrap();
            let body = format!(r#"{{"v":{i}}}"#);
            let snap = make_snapshot(version, prev_body.as_deref(), &body);
            store
                .insert_snapshot(snap)
                .await
                .unwrap_or_else(|e| panic!("insert {i} must succeed: {e}"));

            assert!(
                version > last_version,
                "version {version} not strictly greater than last {last_version}"
            );
            last_version = version;
            prev_body = Some(body);
        }

        // Phase 2: Verify that equal and lower versions are now rejected.
        // Try inserting at the current max (N) — must fail.
        let err = store
            .insert_snapshot(make_snapshot(
                i64::try_from(N).unwrap(),
                None,
                r#"{"v":"dup"}"#,
            ))
            .await
            .expect_err("equal version must be rejected");
        assert!(
            matches!(err, StorageError::SnapshotVersionNotMonotonic { .. }),
            "expected SnapshotVersionNotMonotonic for equal version, got {err:?}"
        );

        // Try inserting at N-1 (lower) — must fail.
        let err2 = store
            .insert_snapshot(make_snapshot(
                i64::try_from(N - 1).unwrap(),
                None,
                r#"{"v":"lower"}"#,
            ))
            .await
            .expect_err("lower version must be rejected");
        assert!(
            matches!(err2, StorageError::SnapshotVersionNotMonotonic { .. }),
            "expected SnapshotVersionNotMonotonic for lower version, got {err2:?}"
        );

        // Phase 3: A new write at N+1 must still succeed after the failed
        // attempts (failed transactions must not corrupt state).
        store
            .insert_snapshot(make_snapshot(
                i64::try_from(N + 1).unwrap(),
                None,
                r#"{"v":"next"}"#,
            ))
            .await
            .expect("insert at N+1 must succeed after failed attempts");
    }
}
