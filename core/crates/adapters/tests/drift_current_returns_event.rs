//! `GET /api/v1/drift/current` — with an unresolved drift event → 200 + event body.

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
    auth::{LoginRateLimiter, SqliteSessionStore, SqliteUserStore, UserRole, UserStore as _},
    http_axum::{AppState, AxumServer, AxumServerConfig, stubs},
    migrate::apply_migrations,
    rng::ThreadRng,
    sqlite_storage::SqliteStorage,
};
use trilithon_core::{
    clock::SystemClock,
    config::types::ServerConfig,
    http::HttpServer,
    schema::SchemaRegistry,
    storage::{
        trait_def::Storage,
        types::{DriftEventRow, DriftRowId, SnapshotId},
    },
};

async fn setup() -> (
    TempDir,
    Arc<dyn Storage>,
    SocketAddr,
    tokio::sync::oneshot::Sender<()>,
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

    (dir, storage_arc, addr, tx)
}

async fn login(addr: SocketAddr) -> String {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/api/v1/auth/login"))
        .json(&serde_json::json!({"username": "alice", "password": "correct-horse-battery"}))
        .send()
        .await
        .expect("login request");
    assert_eq!(resp.status(), 200, "login should succeed");
    resp.headers()
        .get("set-cookie")
        .expect("set-cookie header")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_owned()
}

#[tokio::test]
async fn drift_current_returns_event() {
    let (_dir, storage, addr, tx) = setup().await;

    // Insert a drift event row directly so we can assert it comes back.
    let row_id = DriftRowId("01JDRIFTROW000000000000001".to_owned());
    let correlation_id = "01JCORRELATION0000000000AA".to_owned();
    let drift_row = DriftEventRow {
        id: row_id.clone(),
        correlation_id: correlation_id.clone(),
        detected_at: 1_700_000_000,
        snapshot_id: SnapshotId("deadbeef".to_owned()),
        diff_json: r#"{"entries":[]}"#.to_owned(),
        running_state_hash: "abc123".to_owned(),
        resolution: None,
        resolved_at: None,
    };
    storage
        .record_drift_event(drift_row)
        .await
        .expect("record drift event");

    let cookie = login(addr).await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/api/v1/drift/current"))
        .header("cookie", &cookie)
        .send()
        .await
        .expect("GET /api/v1/drift/current");

    assert_eq!(
        resp.status(),
        200,
        "expected 200 OK when drift event exists"
    );

    let body: serde_json::Value = resp.json().await.expect("JSON body");
    assert_eq!(
        body["event_id"], row_id.0,
        "event_id should match the inserted row"
    );
    assert_eq!(
        body["correlation_id"], correlation_id,
        "correlation_id should match"
    );
    assert_eq!(body["running_state_hash"], "abc123");

    let _ = tx.send(());
}
