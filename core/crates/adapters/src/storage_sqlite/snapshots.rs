//! CAS helpers for `config_version` — optimistic concurrency (Slice 7.5).
//!
//! These functions operate on a single `SqliteConnection` acquired by the
//! caller so they can participate in an outer `BEGIN IMMEDIATE` transaction.
//!
//! # Design
//!
//! The *applied* `config_version` is tracked in `caddy_instances.applied_config_version`
//! (migration 0008).  Snapshots are inserted by the mutation pipeline before
//! `apply()` is called, so `MAX(snapshots.config_version)` is always >= the
//! applied version and cannot serve as the CAS read.
//!
//! `advance_config_version_if_eq` is a CAS gate: it reads `applied_config_version`,
//! compares it to `expected_version`, and on match updates it to `expected_version + 1`.
//! The caller is responsible for opening the `BEGIN IMMEDIATE` transaction before
//! calling this function (`SQLite` read-check-write pattern, ADR-0012).

use sqlx::SqliteConnection;

use trilithon_core::storage::{error::StorageError, types::SnapshotId};

use crate::db_errors::sqlx_err;

/// Read the current *applied* `config_version` for `instance_id`.
///
/// Reads `caddy_instances.applied_config_version` (migration 0008).
/// Returns `0` when the instance row has never had a successful apply.
///
/// # Errors
///
/// Returns [`StorageError`] on any database failure.
pub async fn current_config_version(
    conn: &mut SqliteConnection,
    instance_id: &str,
) -> Result<i64, StorageError> {
    let ver: Option<i64> =
        sqlx::query_scalar("SELECT applied_config_version FROM caddy_instances WHERE id = ?")
            .bind(instance_id)
            .fetch_optional(conn)
            .await
            .map_err(sqlx_err)?;

    Ok(ver.unwrap_or(0))
}

/// CAS-style advance gate.
///
/// Reads `caddy_instances.applied_config_version`, checks it equals
/// `expected_version`, verifies `new_snapshot_id` exists in the DB, then
/// updates `applied_config_version` to `expected_version + 1`.
///
/// Returns `Ok(expected_version + 1)` when the CAS succeeds.
/// Returns `Err(StorageError::OptimisticConflict { observed, expected })` when
/// the applied version does not match `expected_version`.
///
/// The caller **must** hold a `BEGIN IMMEDIATE` transaction before calling
/// this function to prevent TOCTOU races (`SQLite` read-check-write rule).
///
/// # Errors
///
/// Returns [`StorageError::OptimisticConflict`] on version mismatch.
/// Returns [`StorageError::Integrity`] when `new_snapshot_id` is not found.
/// Returns other [`StorageError`] variants on database failure.
pub async fn advance_config_version_if_eq(
    conn: &mut SqliteConnection,
    instance_id: &str,
    expected_version: i64,
    new_snapshot_id: &SnapshotId,
) -> Result<i64, StorageError> {
    let observed = current_config_version(conn, instance_id).await?;

    if observed != expected_version {
        return Err(StorageError::OptimisticConflict {
            observed,
            expected: expected_version,
        });
    }

    let new_version = expected_version + 1;

    // Verify the target snapshot exists in the DB.
    let exists: bool =
        sqlx::query_scalar("SELECT COUNT(*) FROM snapshots WHERE id = ? AND caddy_instance_id = ?")
            .bind(&new_snapshot_id.0)
            .bind(instance_id)
            .fetch_one(&mut *conn)
            .await
            .map(|c: i64| c > 0)
            .map_err(sqlx_err)?;

    if !exists {
        return Err(StorageError::Integrity {
            detail: format!(
                "advance_config_version_if_eq: snapshot {} not found for instance {}",
                new_snapshot_id.0, instance_id
            ),
        });
    }

    // Advance the applied pointer.
    let update_result =
        sqlx::query("UPDATE caddy_instances SET applied_config_version = ? WHERE id = ?")
            .bind(new_version)
            .bind(instance_id)
            .execute(conn)
            .await
            .map_err(sqlx_err)?;

    // Guard against a missing instance row — the UPDATE would silently affect
    // 0 rows and return Ok, producing a phantom version advance.
    if update_result.rows_affected() != 1 {
        return Err(StorageError::Integrity {
            detail: format!(
                "advance_config_version_if_eq: caddy_instances row missing for instance {instance_id}"
            ),
        });
    }

    Ok(new_version)
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
    use tempfile::TempDir;
    use trilithon_core::{
        canonical_json::{CANONICAL_JSON_VERSION, content_address_bytes},
        storage::{error::StorageError, trait_def::Storage, types::Snapshot},
    };

    use crate::{migrate::apply_migrations, sqlite_storage::SqliteStorage};

    use super::*;

    async fn open_store(dir: &TempDir) -> SqliteStorage {
        let store = SqliteStorage::open(dir.path())
            .await
            .expect("SqliteStorage::open should succeed");
        apply_migrations(store.pool())
            .await
            .expect("apply_migrations should succeed");
        store
    }

    fn make_snapshot(config_version: i64) -> Snapshot {
        // Use a version-stamped JSON body so each version produces a unique id.
        let state_json = format!("{{\"_v\":{config_version}}}");
        let id = SnapshotId(content_address_bytes(state_json.as_bytes()));
        Snapshot {
            snapshot_id: id,
            parent_id: None,
            config_version,
            actor: "test".to_owned(),
            intent: format!("test v{config_version}"),
            correlation_id: "test-corr".to_owned(),
            caddy_version: "2.8.0".to_owned(),
            trilithon_version: "0.1.0".to_owned(),
            created_at_unix_seconds: 1_700_000_000 + config_version,
            #[allow(clippy::cast_sign_loss)]
            // reason: test-only; config_version is always positive in these fixtures
            created_at_monotonic_nanos: (1_700_000_000_u64 + config_version as u64)
                * 1_000_000_000,
            canonical_json_version: CANONICAL_JSON_VERSION,
            desired_state_json: state_json,
        }
    }

    #[tokio::test]
    async fn current_version_is_zero_on_empty_db() {
        let dir = TempDir::new().unwrap();
        let store = open_store(&dir).await;
        let mut conn = store.pool().acquire().await.unwrap();

        let v = current_config_version(&mut conn, "local")
            .await
            .expect("should succeed");
        assert_eq!(v, 0);
    }

    #[tokio::test]
    async fn current_version_reflects_applied_pointer() {
        let dir = TempDir::new().unwrap();
        let store = open_store(&dir).await;

        // Inserting snapshots does NOT advance the applied pointer.
        // The pointer only advances via advance_config_version_if_eq.
        let s1 = make_snapshot(1);
        let s2 = make_snapshot(2);
        store.insert_snapshot(s1).await.expect("insert v1");
        store.insert_snapshot(s2).await.expect("insert v2");

        let mut conn = store.pool().acquire().await.unwrap();
        // Pointer is still 0 — nothing applied yet.
        let v = current_config_version(&mut conn, "local")
            .await
            .expect("should succeed");
        assert_eq!(v, 0);

        // Directly update the applied pointer (as advance_config_version_if_eq does).
        sqlx::query("UPDATE caddy_instances SET applied_config_version = 2 WHERE id = 'local'")
            .execute(&mut *conn)
            .await
            .expect("set pointer");

        let v = current_config_version(&mut conn, "local")
            .await
            .expect("should succeed");
        assert_eq!(v, 2);
    }

    #[tokio::test]
    async fn advance_succeeds_when_versions_match() {
        let dir = TempDir::new().unwrap();
        let store = open_store(&dir).await;

        // Simulate: applied_config_version = 1 (s1 was previously applied).
        // s2 is the pending snapshot to be applied next.
        let s1 = make_snapshot(1);
        store.insert_snapshot(s1).await.expect("insert v1");
        let s2 = make_snapshot(2);
        let s2_id = s2.snapshot_id.clone();
        store.insert_snapshot(s2).await.expect("insert v2");

        // Mark v1 as the current applied version.
        let mut setup_conn = store.pool().acquire().await.unwrap();
        sqlx::query("UPDATE caddy_instances SET applied_config_version = 1 WHERE id = 'local'")
            .execute(&mut *setup_conn)
            .await
            .expect("set applied_config_version = 1");
        drop(setup_conn);

        let mut conn = store.pool().acquire().await.unwrap();

        let new_ver = advance_config_version_if_eq(&mut conn, "local", 1, &s2_id)
            .await
            .expect("CAS should succeed when expected == current applied version");
        assert_eq!(new_ver, 2);
    }

    #[tokio::test]
    async fn advance_returns_conflict_when_versions_mismatch() {
        let dir = TempDir::new().unwrap();
        let store = open_store(&dir).await;

        // Simulate: applied_config_version = 2 (s2 was most recently applied).
        // Caller mistakenly passes expected=1 (stale).
        let s1 = make_snapshot(1);
        store.insert_snapshot(s1).await.expect("insert v1");
        let s2 = make_snapshot(2);
        store.insert_snapshot(s2).await.expect("insert v2");
        let s3 = make_snapshot(3);
        let s3_id = s3.snapshot_id.clone();
        store.insert_snapshot(s3).await.expect("insert v3");

        // Mark v2 as the current applied version.
        let mut setup_conn = store.pool().acquire().await.unwrap();
        sqlx::query("UPDATE caddy_instances SET applied_config_version = 2 WHERE id = 'local'")
            .execute(&mut *setup_conn)
            .await
            .expect("set applied_config_version = 2");
        drop(setup_conn);

        let mut conn = store.pool().acquire().await.unwrap();

        let err = advance_config_version_if_eq(&mut conn, "local", 1, &s3_id)
            .await
            .expect_err("should conflict: observed=2, expected=1");

        assert!(
            matches!(
                err,
                StorageError::OptimisticConflict {
                    observed: 2,
                    expected: 1
                }
            ),
            "wrong error: {err:?}"
        );
    }
}
