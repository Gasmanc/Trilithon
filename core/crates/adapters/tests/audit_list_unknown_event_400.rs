//! `GET /api/v1/audit?event=not.a.kind` — must return 400.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: integration test — panics are the correct failure mode

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

#[allow(clippy::too_many_lines)]
// reason: integration test setup is inherently verbose; no logic duplication
async fn setup() -> (
    TempDir,
    SocketAddr,
    tokio::sync::oneshot::Sender<()>,
    String,
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
        .create_user("eve", "correct-horse-battery-staple", UserRole::Owner)
        .await
        .expect("create user");

    let storage_arc: Arc<dyn Storage> = Arc::new(storage);
    let storage_for_state = Arc::clone(&storage_arc);
    let audit_writer = Arc::new(AuditWriter::new_with_arcs(
        storage_arc,
        Arc::new(SystemClock),
        Arc::new(SchemaRegistry::with_tier1_secrets()),
        Arc::new(Sha256AuditHasher),
    ));

    let (user, _) = user_store
        .find_by_username("eve")
        .await
        .expect("find user")
        .expect("user exists");
    let session = session_store
        .create(&user.id, 3600, None, None)
        .await
        .expect("create session");

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
        schema_registry: Arc::new(SchemaRegistry::with_tier1_secrets()),
        hasher: Arc::new(trilithon_adapters::Sha256AuditHasher),
        drift_detector: trilithon_adapters::http_axum::stubs::make_stub_drift_detector(Arc::clone(
            &storage_for_state,
        )),
        capability_cache: Arc::new(trilithon_adapters::caddy::cache::CapabilityCache::default()),
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

    (dir, addr, tx, session.id)
}

#[tokio::test]
async fn audit_list_unknown_event_400() {
    let (_dir, addr, tx, session_id) = setup().await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/api/v1/audit?event=not.a.kind"))
        .header("Cookie", format!("trilithon_session={session_id}"))
        .send()
        .await
        .expect("request");

    assert_eq!(
        resp.status(),
        400,
        "unknown event kind must return 400 Bad Request"
    );

    let _ = tx.send(());
}
