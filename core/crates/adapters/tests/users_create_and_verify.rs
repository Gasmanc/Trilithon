//! Create a user then verify the correct password succeeds.

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
async fn users_create_and_verify_correct_password() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    let user = store
        .create_user("alice", "correct-horse-battery-staple", UserRole::Operator)
        .await
        .expect("create_user must succeed");

    assert_eq!(user.username, "alice");
    assert_eq!(user.role, UserRole::Operator);
    assert!(!user.must_change_pw);

    let found = store
        .find_by_username("alice")
        .await
        .expect("find_by_username must succeed");

    let (fetched_user, hash) = found.expect("user must be present");
    assert_eq!(fetched_user.id, user.id);

    // Verify the correct password.
    let ok = trilithon_adapters::auth::verify_password("correct-horse-battery-staple", &hash)
        .expect("verify_password must not error");

    assert!(ok, "correct password must verify successfully");
}
