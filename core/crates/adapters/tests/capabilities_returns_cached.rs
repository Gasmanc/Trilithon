//! `GET /api/v1/capabilities` — probe cached → 200 with cached data.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: integration test — panics are the correct failure mode

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::Duration;

use tempfile::TempDir;
use trilithon_adapters::{
    AuditWriter, Sha256AuditHasher,
    auth::{LoginRateLimiter, SqliteSessionStore, SqliteUserStore, UserRole, UserStore as _},
    caddy::cache::CapabilityCache,
    http_axum::{AppState, AxumServer, AxumServerConfig, stubs},
    migrate::apply_migrations,
    rng::ThreadRng,
    sqlite_storage::SqliteStorage,
};
use trilithon_core::{
    caddy::capabilities::CaddyCapabilities, clock::SystemClock, config::types::ServerConfig,
    http::HttpServer, schema::SchemaRegistry, storage::trait_def::Storage,
};

async fn setup() -> (
    TempDir,
    SocketAddr,
    tokio::sync::oneshot::Sender<()>,
    Arc<CapabilityCache>,
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
        .create_user("alice", "correct-horse-battery", UserRole::Owner)
        .await
        .expect("create user");

    let storage_arc: Arc<dyn Storage> = Arc::new(storage);
    let audit_writer = Arc::new(AuditWriter::new_with_arcs(
        Arc::clone(&storage_arc),
        Arc::new(SystemClock),
        Arc::new(SchemaRegistry::with_tier1_secrets()),
        Arc::new(Sha256AuditHasher),
    ));

    let capability_cache = Arc::new(CapabilityCache::default());

    let state = Arc::new(AppState {
        apply_in_flight: Arc::new(AtomicBool::new(false)),
        ready_since_unix_ms: Arc::new(AtomicU64::new(1)),
        rate_limiter: Arc::new(LoginRateLimiter::new()),
        session_store,
        user_store,
        audit_writer,
        session_cookie_name: "trilithon_session".to_owned(),
        session_ttl_seconds: 3600,
        token_pool: None,
        applier: Arc::new(stubs::NoopApplier),
        storage: Arc::clone(&storage_arc),
        diff_engine: Arc::new(trilithon_core::diff::DefaultDiffEngine),
        schema_registry: Arc::new(SchemaRegistry::with_tier1_secrets()),
        hasher: Arc::new(Sha256AuditHasher),
        drift_detector: stubs::make_stub_drift_detector(Arc::clone(&storage_arc)),
        capability_cache: Arc::clone(&capability_cache),
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

    (dir, addr, tx, capability_cache)
}

async fn login(addr: SocketAddr) -> String {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/api/v1/auth/login"))
        .json(&serde_json::json!({"username": "alice", "password": "correct-horse-battery"}))
        .send()
        .await
        .expect("login request");
    assert_eq!(resp.status(), 200);
    let cookie = resp
        .headers()
        .get("set-cookie")
        .expect("set-cookie header")
        .to_str()
        .unwrap()
        .to_owned();
    cookie.split(';').next().unwrap().trim().to_owned()
}

#[tokio::test]
async fn capabilities_returns_cached() {
    let (_dir, addr, tx, cache) = setup().await;

    // Seed the cache with a known probe result.
    let caps = CaddyCapabilities {
        loaded_modules: BTreeSet::from([
            "http.handlers.rate_limit".to_owned(),
            "http.handlers.reverse_proxy".to_owned(),
        ]),
        caddy_version: "v2.8.4".to_owned(),
        probed_at: 1_700_000_000,
    };
    cache.replace(caps);

    let cookie = login(addr).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/api/v1/capabilities"))
        .header("cookie", &cookie)
        .send()
        .await
        .expect("GET /api/v1/capabilities");

    assert_eq!(resp.status(), 200, "expected 200 when cache is populated");
    let body: serde_json::Value = resp.json().await.expect("parse body");

    assert_eq!(body["caddy_version"], "v2.8.4");
    assert_eq!(body["probed_at"], 1_700_000_000i64);
    assert!(body["has_rate_limit"].as_bool().unwrap());
    assert!(!body["has_waf"].as_bool().unwrap());

    let _ = tx.send(());
}
