//! Persistence layer for [`CaddyCapabilities`] probe results.
//!
//! Each call to [`CapabilityStore::record_current`] demotes the previously
//! current row for the given `caddy_instance_id` and inserts a fresh row
//! with `is_current = 1`.

use sqlx::SqlitePool;
use trilithon_core::{
    caddy::capabilities::CaddyCapabilities,
    storage::error::{SqliteErrorKind, StorageError},
};
use ulid::Ulid;

/// Persists capability-probe results to the `capability_probe_results` table.
pub struct CapabilityStore {
    pool: SqlitePool,
}

impl CapabilityStore {
    /// Wrap an existing connection pool.
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Demote any existing current row for `instance_id`, then insert a new
    /// row with `is_current = 1`.
    ///
    /// Both operations run inside a single transaction so there is never a
    /// window where zero or two rows have `is_current = 1` for the same
    /// `caddy_instance_id`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Sqlite`] on any database error.
    pub async fn record_current(
        &self,
        instance_id: &str,
        caps: &CaddyCapabilities,
    ) -> Result<(), StorageError> {
        let capabilities_json = serde_json::to_string(caps).map_err(|e| StorageError::Sqlite {
            kind: SqliteErrorKind::Other(format!("serialise capabilities: {e}")),
        })?;

        let id = Ulid::new().to_string();

        let mut tx = self.pool.begin().await.map_err(sqlx_err)?;

        sqlx::query(
            "UPDATE capability_probe_results \
             SET is_current = 0 \
             WHERE caddy_instance_id = ? AND is_current = 1",
        )
        .bind(instance_id)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_err)?;

        sqlx::query(
            "INSERT INTO capability_probe_results \
             (id, caddy_instance_id, probed_at, caddy_version, capabilities_json, is_current) \
             VALUES (?, ?, ?, ?, ?, 1)",
        )
        .bind(&id)
        .bind(instance_id)
        .bind(caps.probed_at)
        .bind(&caps.caddy_version)
        .bind(&capabilities_json)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_err)?;

        tx.commit().await.map_err(sqlx_err)?;

        Ok(())
    }
}

#[allow(clippy::needless_pass_by_value)]
// reason: `sqlx::Error` is non-Copy; value must be owned to call `.to_string()` in the wildcard arm
fn sqlx_err(e: sqlx::Error) -> StorageError {
    match &e {
        sqlx::Error::Database(db_err) => {
            let code: i32 = db_err.code().as_deref().unwrap_or("").parse().unwrap_or(0);
            // Mask to the primary error code (low 8 bits) so that extended
            // codes such as SQLITE_BUSY_RECOVERY (261) and
            // SQLITE_BUSY_SNAPSHOT (517) are caught alongside the base codes.
            match code & 0xFF {
                5 | 6 => StorageError::SqliteBusy { retries: 0 },
                11 => StorageError::Sqlite {
                    kind: SqliteErrorKind::Corrupt,
                },
                19 => StorageError::Sqlite {
                    kind: SqliteErrorKind::Constraint,
                },
                _ => StorageError::Sqlite {
                    kind: SqliteErrorKind::Other(e.to_string()),
                },
            }
        }
        _ => StorageError::Sqlite {
            kind: SqliteErrorKind::Other(e.to_string()),
        },
    }
}
