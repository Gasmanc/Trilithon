//! After change-password: `must_change_pw` is cleared and other sessions revoked.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: integration test

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::Duration;

use tempfile::TempDir;
use trilithon_adapters::{
    AuditWriter, Sha256AuditHasher,
    auth::{
        LoginRateLimiter, SessionStore as _, SqliteSessionStore, SqliteUserStore, UserRole,
        UserStore as _,
    },
    http_axum::{AppState, AxumServer, AxumServerConfig},
    migrate::apply_migrations,
    rng::ThreadRng,
    sqlite_storage::SqliteStorage,
};
use trilithon_core::{
    clock::SystemClock, config::types::ServerConfig, http::HttpServer, schema::SchemaRegistry,
    storage::trait_def::Storage,
};

async fn setup() -> (
    TempDir,
    SocketAddr,
    tokio::sync::oneshot::Sender<()>,
    Arc<SqliteSessionStore>,
    Arc<SqliteUserStore>,
) {
    let dir = TempDir::new().unwrap();
    let storage = SqliteStorage::open(dir.path())
        .await
        .expect("SqliteStorage::open");
    apply_migrations(storage.pool()).await.expect("migrations");

    let pool = storage.pool().clone();
    let user_store = Arc::new(SqliteUserStore::new(pool.clone()));
    let session_store = Arc::new(SqliteSessionStore::new(pool.clone(), Arc::new(ThreadRng)));

    // Create a user with must_change_pw set.
    let user = user_store
        .create_user("eve", "OldPassword12345", UserRole::Owner)
        .await
        .expect("create user");
    user_store
        .set_must_change_pw(&user.id, true)
        .await
        .expect("set must_change_pw");

    let storage_arc: Arc<dyn Storage> = Arc::new(storage);
    let audit_writer = Arc::new(AuditWriter::new_with_arcs(
        storage_arc,
        Arc::new(SystemClock),
        Arc::new(SchemaRegistry::with_tier1_secrets()),
        Arc::new(Sha256AuditHasher),
    ));

    let us_clone = Arc::clone(&user_store);
    let ss_clone = Arc::clone(&session_store);

    let state = Arc::new(AppState {
        apply_in_flight: Arc::new(AtomicBool::new(false)),
        ready_since_unix_ms: Arc::new(AtomicU64::new(1)),
        rate_limiter: Arc::new(LoginRateLimiter::new()),
        session_store: session_store as Arc<dyn trilithon_adapters::auth::SessionStore>,
        user_store: user_store as Arc<dyn trilithon_adapters::auth::UserStore>,
        audit_writer,
        session_cookie_name: "trilithon_session".to_owned(),
        session_ttl_seconds: 3600,
        token_pool: None,
    });

    let cfg = AxumServerConfig {
        bind_port: 0,
        ..AxumServerConfig::default()
    };
    let mut server = AxumServer::new(cfg, state);
    let server_cfg = ServerConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        allow_remote: false,
    };
    let addr = server.bind(&server_cfg).await.expect("bind");

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let shutdown = Box::pin(async move {
        let _ = rx.await;
    });
    tokio::spawn(async move {
        server.run(shutdown).await.ok();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    (dir, addr, tx, ss_clone, us_clone)
}

#[tokio::test]
async fn auth_change_password_clears_flag() {
    let (_dir, addr, tx, session_store, user_store) = setup().await;

    // Look up the user.
    let (user, _) = user_store
        .find_by_username("eve")
        .await
        .expect("find user")
        .expect("user exists");

    // Create two sessions (simulate "other sessions").
    let s1 = session_store
        .create(&user.id, 3600, None, None)
        .await
        .unwrap();
    let s2 = session_store
        .create(&user.id, 3600, None, None)
        .await
        .unwrap();

    // Verify must_change_pw is true before the request.
    let (before, _) = user_store
        .find_by_username("eve")
        .await
        .expect("find user")
        .expect("user exists");
    assert!(before.must_change_pw, "must_change_pw must be true before");

    // Call change-password with a real session cookie.
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/api/v1/auth/change-password"))
        .header("Cookie", format!("trilithon_session={}", s1.id))
        .json(&serde_json::json!({
            "old_password": "OldPassword12345",
            "new_password": "NewPassword67890!"
        }))
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 204, "change-password must return 204");

    // must_change_pw must be cleared.
    let (after, _) = user_store
        .find_by_username("eve")
        .await
        .expect("find user")
        .expect("user exists");
    assert!(!after.must_change_pw, "must_change_pw must be cleared");

    // All prior sessions must be revoked.
    let s1_live = session_store.touch(&s1.id).await.expect("touch");
    let s2_live = session_store.touch(&s2.id).await.expect("touch");
    assert!(s1_live.is_none(), "session s1 must be revoked");
    assert!(s2_live.is_none(), "session s2 must be revoked");

    // New password must work for login.
    let resp2 = client
        .post(format!("http://{addr}/api/v1/auth/login"))
        .json(&serde_json::json!({
            "username": "eve",
            "password": "NewPassword67890!"
        }))
        .send()
        .await
        .expect("request");
    assert_eq!(resp2.status(), 200, "new password must allow login");

    let _ = tx.send(());
}
