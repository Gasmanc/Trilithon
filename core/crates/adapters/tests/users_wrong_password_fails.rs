//! Verifying with the wrong password returns `Ok(false)`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use tempfile::TempDir;
use trilithon_adapters::{
    auth::{UserRole, UserStore as _, users::SqliteUserStore},
    migrate::apply_migrations,
    sqlite_storage::SqliteStorage,
};

async fn open_store(dir: &TempDir) -> SqliteUserStore {
    let storage = SqliteStorage::open(dir.path())
        .await
        .expect("SqliteStorage::open should succeed");
    apply_migrations(storage.pool())
        .await
        .expect("apply_migrations should succeed");
    SqliteUserStore::new(storage.pool().clone())
}

#[tokio::test]
async fn users_wrong_password_fails() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    store
        .create_user("bob", "correct-password", UserRole::Reader)
        .await
        .expect("create_user must succeed");

    let (_, hash) = store
        .find_by_username("bob")
        .await
        .expect("find_by_username must succeed")
        .expect("user must be present");

    let ok = trilithon_adapters::auth::verify_password("wrong-password", &hash)
        .expect("verify_password must not error on wrong password");

    assert!(!ok, "wrong password must return false");
}
