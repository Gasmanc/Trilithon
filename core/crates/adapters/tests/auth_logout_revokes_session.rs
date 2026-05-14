//! After logout the session cookie value no longer admits requests (session is revoked).

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

    user_store
        .create_user("diana", "LogoutTestPassword123", UserRole::Owner)
        .await
        .expect("create user");

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
async fn auth_logout_revokes_session() {
    let (_dir, addr, tx, session_store, user_store) = setup().await;

    // Create a session directly (bypassing login endpoint since auth middleware
    // is from slice 9.6 and the stub uses X-Session-Id / X-User-Id headers).
    let (user, _hash) = user_store
        .find_by_username("diana")
        .await
        .expect("find user")
        .expect("user exists");

    let session = session_store
        .create(&user.id, 3600, None, None)
        .await
        .expect("create session");

    // Verify the session is live.
    let live = session_store.touch(&session.id).await.expect("touch");
    assert!(live.is_some(), "session must be live before logout");

    // Call logout with a real session cookie.
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/api/v1/auth/logout"))
        .header("Cookie", format!("trilithon_session={}", session.id))
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 204, "logout must return 204");

    // Verify cookie is cleared.
    let cookies: Vec<_> = resp.headers().get_all("set-cookie").iter().collect();
    let cleared = cookies.iter().any(|v| {
        let s = v.to_str().unwrap_or("");
        s.contains("trilithon_session=") && s.contains("Max-Age=0")
    });
    assert!(cleared, "logout must clear the session cookie (Max-Age=0)");

    // Verify session is revoked in the store.
    let after = session_store.touch(&session.id).await.expect("touch");
    assert!(after.is_none(), "session must be invalid after logout");

    let _ = tx.send(());
}
