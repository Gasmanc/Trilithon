//! A session with `expires_at` in the past returns None from touch.
//! Uses a manually backdated row — no real sleeping required.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only; panics are the correct failure mode

use std::sync::Arc;

use tempfile::TempDir;
use trilithon_adapters::{
    auth::{SessionStore as _, SqliteSessionStore},
    migrate::apply_migrations,
    rng::ThreadRng,
    sqlite_storage::SqliteStorage,
};

/// Insert a session then backdate `expires_at`, assert touch returns None.
#[tokio::test]
async fn sessions_expiry_honoured_past_expiry_returns_none() {
    let dir = TempDir::new().unwrap();
    let storage = SqliteStorage::open(dir.path()).await.expect("open");
    apply_migrations(storage.pool()).await.expect("migrate");
    let pool = storage.pool().clone();

    let store = SqliteSessionStore::new(pool.clone(), Arc::new(ThreadRng));

    let session = store
        .create("user-expiry", 3600, None, None)
        .await
        .expect("create");

    // Backdate expires_at to 10 seconds before created_at.
    let past = session.created_at - 10;
    sqlx::query("UPDATE sessions SET expires_at = ?1 WHERE id = ?2")
        .bind(past)
        .bind(&session.id)
        .execute(&pool)
        .await
        .expect("backdate expires_at");

    let result = store
        .touch(&session.id)
        .await
        .expect("touch should not DB-error");
    assert!(
        result.is_none(),
        "expired session must return None from touch"
    );
}
