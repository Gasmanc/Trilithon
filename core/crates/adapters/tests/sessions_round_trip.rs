//! Create, touch, and revoke a session; assert the full lifecycle.

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

async fn open_store(dir: &TempDir) -> SqliteSessionStore {
    let storage = SqliteStorage::open(dir.path())
        .await
        .expect("SqliteStorage::open");
    apply_migrations(storage.pool())
        .await
        .expect("apply_migrations");
    SqliteSessionStore::new(storage.pool().clone(), Arc::new(ThreadRng))
}

#[tokio::test]
async fn sessions_round_trip_lifecycle() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    // Create
    let session = store
        .create("user-1", 3600, Some("ua".into()), Some("127.0.0.1".into()))
        .await
        .expect("create should succeed");

    assert_eq!(session.user_id, "user-1");
    assert!(session.revoked_at.is_none());
    assert!(session.expires_at > session.created_at);

    // Touch — should return the session
    let touched = store
        .touch(&session.id)
        .await
        .expect("touch should succeed");
    assert!(
        touched.is_some(),
        "touch must return Some for a live session"
    );

    // Revoke
    store
        .revoke(&session.id)
        .await
        .expect("revoke should succeed");

    // Touch after revoke — must return None
    let after_revoke = store
        .touch(&session.id)
        .await
        .expect("touch after revoke should not DB-error");
    assert!(
        after_revoke.is_none(),
        "touch after revoke must return None"
    );
}

#[tokio::test]
async fn sessions_revoke_all_for_user() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    // Create two sessions for the same user.
    let s1 = store.create("user-2", 3600, None, None).await.unwrap();
    let s2 = store.create("user-2", 3600, None, None).await.unwrap();

    let count = store.revoke_all_for_user("user-2").await.unwrap();
    assert_eq!(count, 2, "both sessions should be revoked");

    assert!(store.touch(&s1.id).await.unwrap().is_none());
    assert!(store.touch(&s2.id).await.unwrap().is_none());
}
