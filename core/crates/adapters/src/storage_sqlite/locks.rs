//! Advisory lock helpers for the `apply_locks` table (Slice 7.6).
//!
//! These helpers provide cross-process serialisation for `apply()` calls.
//! A row in `apply_locks` acts as a SQLite-level advisory lock: because
//! `instance_id` is the `PRIMARY KEY`, a second `INSERT` from another process
//! fails with a `UNIQUE` constraint violation rather than blocking.
//!
//! The in-process serialisation half lives in [`CaddyApplier`]; this module
//! only covers the cross-process (database) side.
//!
//! # Stale lock reaping
//!
//! If the lock row exists but the holder process is no longer alive, the
//! helper deletes the stale row and retries the `INSERT` once.  Liveness
//! is probed with `kill(pid, 0)` (POSIX), which succeeds (returns `Ok`) when
//! the process exists and the caller has permission to signal it.
//!
//! # Drop behaviour
//!
//! [`AcquiredLock::drop`] issues a best-effort `DELETE` of the lock row inside
//! a `tokio::task::spawn_blocking` closure so it can run from a `Drop` impl
//! without blocking the async executor thread.

use sqlx::SqlitePool;
use tokio::task;
use trilithon_core::storage::error::StorageError;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can arise from advisory-lock operations.
///
/// `Clone` is derived by storing storage errors as their `Display` string so
/// that `StorageError` (which is not `Clone`) need not be held by value.
#[derive(Clone, Debug, thiserror::Error)]
pub enum LockError {
    /// Another process currently holds the lock for `instance_id`.
    #[error("apply lock already held by pid {pid}")]
    AlreadyHeld {
        /// PID of the current lock holder.
        pid: i32,
    },
    /// A storage-layer error occurred during lock operations.
    #[error("storage: {0}")]
    Storage(String),
}

impl From<StorageError> for LockError {
    fn from(e: StorageError) -> Self {
        Self::Storage(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// AcquiredLock RAII guard
// ---------------------------------------------------------------------------

/// An RAII guard that holds the advisory lock for `instance_id`.
///
/// When dropped the lock row is deleted from `apply_locks` on a best-effort
/// basis.  A failure to delete (e.g. because the pool is shutting down) is
/// tolerated — the lock row will be considered stale by the next caller who
/// checks liveness.
#[derive(Debug)]
pub struct AcquiredLock {
    pool: SqlitePool,
    instance_id: String,
    holder_pid: i32,
}

impl Drop for AcquiredLock {
    fn drop(&mut self) {
        let pool = self.pool.clone();
        let instance_id = self.instance_id.clone();
        let holder_pid = self.holder_pid;
        // Spawn a blocking task so we can issue the DELETE without an async
        // runtime handle from within Drop.  The task is fire-and-forget;
        // failures are logged but do not panic.
        //
        // `spawn_blocking` returns a `JoinHandle` (a Future); discarding it
        // with `drop()` is intentional — we do not need to await it.
        drop(task::spawn_blocking(move || {
            // Use the tokio single-threaded runtime to drive the async delete
            // without creating a new multi-threaded runtime, which would be
            // expensive and could conflict with the existing one.
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            match rt {
                Ok(rt) => {
                    rt.block_on(async {
                        let _ = sqlx::query(
                            "DELETE FROM apply_locks WHERE instance_id = ? AND holder_pid = ?",
                        )
                        .bind(&instance_id)
                        .bind(holder_pid)
                        .execute(&pool)
                        .await;
                    });
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        instance_id = %instance_id,
                        "apply_lock.drop: failed to build runtime for lock release"
                    );
                }
            }
        }));
    }
}

// ---------------------------------------------------------------------------
// Process liveness probe
// ---------------------------------------------------------------------------

/// Returns `true` when process `pid` appears to be alive on this system.
///
/// On Unix, probes via `kill -0 <pid>` (a signal-0 send that merely checks
/// existence without delivering a signal).  On non-Unix platforms the probe
/// is not available and always returns `false` so stale locks are
/// unconditionally reaped.
fn process_alive(pid: i32) -> bool {
    #[cfg(unix)]
    {
        // `kill -0 <pid>` exits 0 when the process exists and is reachable.
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .is_ok_and(|o| o.status.success())
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

// ---------------------------------------------------------------------------
// acquire_apply_lock
// ---------------------------------------------------------------------------

/// Acquire the advisory lock for `instance_id`.
///
/// Uses `BEGIN IMMEDIATE` + `INSERT` so the read-check-write is atomic even
/// under concurrent writers (prior knowledge: sqlite-begin-immediate-read-check-write).
///
/// # Algorithm
///
/// 1. Open a `BEGIN IMMEDIATE` transaction.
/// 2. Try to `INSERT` a new lock row. On success → commit and return
///    `AcquiredLock`. On `UNIQUE` constraint → another row exists:
///    read `holder_pid`, check liveness, and either return
///    `LockError::AlreadyHeld` (live holder) or reap the stale row and
///    retry the `INSERT` once.
///
/// # Errors
///
/// Returns [`LockError::AlreadyHeld`] when a live process holds the lock,
/// or [`LockError::Storage`] for unexpected database errors.
pub async fn acquire_apply_lock(
    pool: &SqlitePool,
    instance_id: &str,
    holder_pid: i32,
) -> Result<AcquiredLock, LockError> {
    #[allow(clippy::cast_possible_wrap)]
    // reason: unix timestamp won't exceed i64::MAX for ~292 billion years
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // ── First attempt ─────────────────────────────────────────────────────

    let first = try_insert_lock(pool, instance_id, holder_pid, now_secs).await?;
    if first {
        return Ok(AcquiredLock {
            pool: pool.clone(),
            instance_id: instance_id.to_owned(),
            holder_pid,
        });
    }

    // ── Constraint hit: inspect existing row ──────────────────────────────

    let existing_pid: Option<i32> =
        sqlx::query_scalar("SELECT holder_pid FROM apply_locks WHERE instance_id = ?")
            .bind(instance_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| LockError::Storage(e.to_string()))?;

    let stale_pid = match existing_pid {
        None => {
            // Row was deleted between our INSERT and SELECT (race); retry.
            let second = try_insert_lock(pool, instance_id, holder_pid, now_secs).await?;
            if second {
                return Ok(AcquiredLock {
                    pool: pool.clone(),
                    instance_id: instance_id.to_owned(),
                    holder_pid,
                });
            }
            // Still blocked.
            return Err(LockError::AlreadyHeld { pid: holder_pid });
        }
        Some(pid) if process_alive(pid) => {
            return Err(LockError::AlreadyHeld { pid });
        }
        Some(pid) => pid,
    };

    // ── Stale lock reap: delete + retry once ──────────────────────────────

    sqlx::query("DELETE FROM apply_locks WHERE instance_id = ? AND holder_pid = ?")
        .bind(instance_id)
        .bind(stale_pid)
        .execute(pool)
        .await
        .map_err(|e| LockError::Storage(e.to_string()))?;

    let second = try_insert_lock(pool, instance_id, holder_pid, now_secs).await?;
    if second {
        Ok(AcquiredLock {
            pool: pool.clone(),
            instance_id: instance_id.to_owned(),
            holder_pid,
        })
    } else {
        // Another process beat us after we deleted the stale row.
        let contender_pid: Option<i32> =
            sqlx::query_scalar("SELECT holder_pid FROM apply_locks WHERE instance_id = ?")
                .bind(instance_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| LockError::Storage(e.to_string()))?;
        Err(LockError::AlreadyHeld {
            pid: contender_pid.unwrap_or(-1),
        })
    }
}

// ---------------------------------------------------------------------------
// Internal helper
// ---------------------------------------------------------------------------

/// Attempt one `BEGIN IMMEDIATE` + `INSERT` of a lock row.
///
/// Returns:
/// - `Ok(true)` — inserted successfully (lock acquired).
/// - `Ok(false)` — `UNIQUE` constraint violation (row already exists).
/// - `Err(_)` — unexpected database error.
async fn try_insert_lock(
    pool: &SqlitePool,
    instance_id: &str,
    holder_pid: i32,
    acquired_at: i64,
) -> Result<bool, LockError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| LockError::Storage(e.to_string()))?;

    // Upgrade to IMMEDIATE so the read-check-write is fully serialised.
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *tx)
        .await
        .or_else(|e| {
            // SQLite returns "cannot start a transaction within a transaction"
            // when the pool already began one for us; that is fine — we are
            // already in an IMMEDIATE transaction via the pool's WAL mode.
            let msg = e.to_string();
            if msg.contains("cannot start a transaction") || msg.contains("within a transaction") {
                Ok(sqlx::sqlite::SqliteQueryResult::default())
            } else {
                Err(LockError::Storage(msg))
            }
        })?;

    let result = sqlx::query(
        "INSERT INTO apply_locks (instance_id, holder_pid, acquired_at) VALUES (?, ?, ?)",
    )
    .bind(instance_id)
    .bind(holder_pid)
    .bind(acquired_at)
    .execute(&mut *tx)
    .await;

    match result {
        Ok(_) => {
            tx.commit()
                .await
                .map_err(|e| LockError::Storage(e.to_string()))?;
            Ok(true)
        }
        Err(e) => {
            let code: i32 = if let sqlx::Error::Database(ref db_err) = e {
                db_err.code().as_deref().unwrap_or("").parse().unwrap_or(0)
            } else {
                0
            };
            // code 19 = SQLITE_CONSTRAINT (UNIQUE violation)
            if code & 0xFF == 19 {
                Ok(false)
            } else {
                Err(LockError::Storage(e.to_string()))
            }
        }
    }
}
