//! Slice 7.6 — second `acquire_apply_lock` call on same instance returns
//! `LockError::AlreadyHeld` when the first lock is still held.
//!
//! Opens two `SqlitePool` instances on the same database file and calls
//! `acquire_apply_lock` from each.  While the first `AcquiredLock` guard is
//! alive, the second call must return `LockError::AlreadyHeld`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: integration test — panics and unwrap are the correct failure mode here

use std::time::Duration;

use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};
use tempfile::TempDir;
use trilithon_adapters::{
    migrate::apply_migrations,
    sqlite_storage::SqliteStorage,
    storage_sqlite::locks::{LockError, acquire_apply_lock},
};

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn open_store_and_migrate(dir: &TempDir) -> SqliteStorage {
    let store = SqliteStorage::open(dir.path())
        .await
        .expect("SqliteStorage::open");
    apply_migrations(store.pool())
        .await
        .expect("apply_migrations");
    store
}

/// Open a second independent pool on the same `trilithon.db` file.
async fn open_second_pool(dir: &TempDir) -> SqlitePool {
    let opts = SqliteConnectOptions::new()
        .filename(dir.path().join("trilithon.db"))
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await
        .expect("second pool connect")
}

// ── Test ──────────────────────────────────────────────────────────────────────

/// While the first `AcquiredLock` guard is live, a second pool's
/// `acquire_apply_lock` call must return `LockError::AlreadyHeld`.
#[tokio::test]
async fn second_acquire_returns_already_held_while_first_is_live() {
    let dir = TempDir::new().unwrap();
    let store = open_store_and_migrate(&dir).await;
    let pool_a = store.pool().clone();
    let pool_b = open_second_pool(&dir).await;

    let pid_a = i32::try_from(std::process::id()).unwrap_or(i32::MAX);
    // Use a different PID for pool_b to simulate a second process.  Any
    // non-existent PID works; we just need to be different from pool_a.
    let pid_b = pid_a.saturating_add(1);

    // First process acquires the lock.
    let lock_a = acquire_apply_lock(&pool_a, "local", pid_a)
        .await
        .expect("first acquire must succeed");

    // Second process (different pool, different PID) should be blocked.
    let result_b = acquire_apply_lock(&pool_b, "local", pid_b).await;

    assert!(
        matches!(result_b, Err(LockError::AlreadyHeld { .. })),
        "expected AlreadyHeld, got {result_b:?}"
    );

    // Drop the first lock.
    drop(lock_a);

    // Give the drop handler time to delete the lock row.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Now the second pool should be able to acquire the lock.
    let lock_b = acquire_apply_lock(&pool_b, "local", pid_b)
        .await
        .expect("acquire must succeed after first lock is released");

    drop(lock_b);
}
