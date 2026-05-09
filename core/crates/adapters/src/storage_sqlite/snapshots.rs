//! CAS helpers for `config_version` — optimistic concurrency (Slice 7.5).
//!
//! These functions operate on a single `SqliteConnection` acquired by the
//! caller so they can participate in an outer `BEGIN IMMEDIATE` transaction.
//!
//! # Design
//!
//! `config_version` is a monotonically increasing integer stored in the
//! `snapshots` table.  The current version for an instance is defined as the
//! maximum `config_version` among all rows for that `caddy_instance_id`
//! (ADR-0009, ADR-0012).  There is no separate pointer row; the snapshot with
//! the highest version IS the current desired state.
//!
//! `advance_config_version_if_eq` is a CAS gate: it reads the current max,
//! compares it to `expected_version`, and either confirms the advance or
//! returns `StorageError::OptimisticConflict`.  The caller is responsible for
//! opening the `BEGIN IMMEDIATE` transaction before calling this function
//! (`SQLite` read-check-write pattern).

use sqlx::SqliteConnection;

use trilithon_core::storage::{error::StorageError, types::SnapshotId};

use crate::db_errors::sqlx_err;

/// Read the current `config_version` for `instance_id`.
///
/// Returns `0` when no snapshot exists yet (virgin database).
///
/// # Errors
///
/// Returns [`StorageError`] on any database failure.
pub async fn current_config_version(
    conn: &mut SqliteConnection,
    instance_id: &str,
) -> Result<i64, StorageError> {
    let max: Option<i64> =
        sqlx::query_scalar("SELECT MAX(config_version) FROM snapshots WHERE caddy_instance_id = ?")
            .bind(instance_id)
            .fetch_one(conn)
            .await
            .map_err(sqlx_err)?;

    Ok(max.unwrap_or(0))
}

/// CAS-style advance gate.
///
/// Verifies that `MAX(config_version)` for `instance_id` equals
/// `expected_version`, then confirms that `new_snapshot_id` exists with
/// `config_version = expected_version + 1`.
///
/// Returns `Ok(expected_version + 1)` when the CAS succeeds.
/// Returns `Err(StorageError::OptimisticConflict { observed, expected })` when
/// the current max does not match `expected_version`.
///
/// The caller **must** hold a `BEGIN IMMEDIATE` transaction before calling
/// this function to prevent TOCTOU races (`SQLite` read-check-write rule).
///
/// # Errors
///
/// Returns [`StorageError::OptimisticConflict`] on version mismatch.
/// Returns [`StorageError::Integrity`] when `new_snapshot_id` is not found or
/// its `config_version` does not equal `expected_version + 1`.
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

    // Verify the target snapshot exists with the expected new version.
    let stored_version: Option<i64> = sqlx::query_scalar(
        "SELECT config_version FROM snapshots WHERE id = ? AND caddy_instance_id = ?",
    )
    .bind(&new_snapshot_id.0)
    .bind(instance_id)
    .fetch_optional(conn)
    .await
    .map_err(sqlx_err)?;

    match stored_version {
        None => Err(StorageError::Integrity {
            detail: format!(
                "advance_config_version_if_eq: snapshot {} not found for instance {}",
                new_snapshot_id.0, instance_id
            ),
        }),
        Some(v) if v != new_version => Err(StorageError::Integrity {
            detail: format!(
                "advance_config_version_if_eq: snapshot {} has config_version {v}, \
                 expected {new_version}",
                new_snapshot_id.0
            ),
        }),
        Some(_) => Ok(new_version),
    }
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
    async fn current_version_reflects_max_snapshot() {
        let dir = TempDir::new().unwrap();
        let store = open_store(&dir).await;

        // Insert two snapshots with versions 1 and 2.
        let s1 = make_snapshot(1);
        let s2 = make_snapshot(2);
        store.insert_snapshot(s1).await.expect("insert v1");
        store.insert_snapshot(s2).await.expect("insert v2");

        let mut conn = store.pool().acquire().await.unwrap();
        let v = current_config_version(&mut conn, "local")
            .await
            .expect("should succeed");
        assert_eq!(v, 2);
    }

    #[tokio::test]
    async fn advance_succeeds_when_versions_match() {
        let dir = TempDir::new().unwrap();
        let store = open_store(&dir).await;

        // Current state: version 1 in DB; snapshot at version 2 ready to apply.
        let s1 = make_snapshot(1);
        store.insert_snapshot(s1).await.expect("insert v1");
        let s2 = make_snapshot(2);
        let s2_id = s2.snapshot_id.clone();
        store.insert_snapshot(s2).await.expect("insert v2");

        let mut conn = store.pool().acquire().await.unwrap();
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .expect("BEGIN IMMEDIATE");

        let new_ver = advance_config_version_if_eq(&mut conn, "local", 1, &s2_id)
            .await
            .expect("CAS should succeed when expected == current");
        assert_eq!(new_ver, 2);
    }

    #[tokio::test]
    async fn advance_returns_conflict_when_versions_mismatch() {
        let dir = TempDir::new().unwrap();
        let store = open_store(&dir).await;

        // DB at version 2; caller expects version 1.
        let s1 = make_snapshot(1);
        store.insert_snapshot(s1).await.expect("insert v1");
        let s2 = make_snapshot(2);
        store.insert_snapshot(s2).await.expect("insert v2");
        let s3 = make_snapshot(3);
        let s3_id = s3.snapshot_id.clone();
        store.insert_snapshot(s3).await.expect("insert v3");

        let mut conn = store.pool().acquire().await.unwrap();
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .expect("BEGIN IMMEDIATE");

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
