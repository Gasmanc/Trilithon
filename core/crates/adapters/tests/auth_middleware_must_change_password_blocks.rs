//! Session with `must_change_pw = true`:
//!   * public route (`GET /api/v1/health`) → passes through (200 or 503).
//!   * protected route (`POST /api/v1/auth/logout`) → 403 `{"code":"must-change-password"}`.

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

    let user = user_store
        .create_user("force_change", "ForceChange123!", UserRole::Owner)
        .await
        .expect("create user");
    user_store
        .set_must_change_pw(&user.id, true)
        .await
        .expect("set must_change_pw");

    let storage_arc: Arc<dyn Storage> = Arc::new(storage);
    let storage_for_state = Arc::clone(&storage_arc);
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
        applier: Arc::new(trilithon_adapters::http_axum::stubs::NoopApplier),
        storage: Arc::clone(&storage_for_state),
        diff_engine: Arc::new(trilithon_core::diff::DefaultDiffEngine),
        schema_registry: Arc::new(trilithon_core::schema::SchemaRegistry::with_tier1_secrets()),
        hasher: Arc::new(trilithon_adapters::Sha256AuditHasher),
        drift_detector: trilithon_adapters::http_axum::stubs::make_stub_drift_detector(Arc::clone(
            &storage_for_state,
        )),
        capability_cache: Arc::new(trilithon_adapters::caddy::cache::CapabilityCache::default()),
        secure_cookies: false,
        trusted_proxy: false,
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
async fn auth_middleware_must_change_password_blocks() {
    let (_dir, addr, tx, session_store, user_store) = setup().await;

    let (user, _) = user_store
        .find_by_username("force_change")
        .await
        .expect("find user")
        .expect("user exists");

    let session = session_store
        .create(&user.id, 3600, None, None)
        .await
        .expect("create session");

    let client = reqwest::Client::new();
    let cookie = format!("trilithon_session={}", session.id);

    // Public route must pass through regardless.
    let health = client
        .get(format!("http://{addr}/api/v1/health"))
        .send()
        .await
        .expect("health request");
    assert!(
        health.status().is_success() || health.status().as_u16() == 503,
        "public route must not be blocked by must_change_pw (got {})",
        health.status()
    );

    // Protected route must be blocked with 403.
    let resp = client
        .post(format!("http://{addr}/api/v1/auth/logout"))
        .header("Cookie", &cookie)
        .send()
        .await
        .expect("logout request");

    assert_eq!(
        resp.status(),
        403,
        "must_change_pw session must be blocked with 403 on protected routes"
    );
    let body: serde_json::Value = resp.json().await.expect("JSON body");
    assert_eq!(
        body.get("code").and_then(|v| v.as_str()),
        Some("must-change-password"),
        "body must carry must-change-password code"
    );

    let _ = tx.send(());
}
