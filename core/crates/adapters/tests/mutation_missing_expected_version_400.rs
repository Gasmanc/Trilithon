//! `POST /api/v1/mutations` with missing `expected_version` → 400.

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
    http_axum::{AppState, AxumServer, AxumServerConfig, stubs::NoopApplier},
    migrate::apply_migrations,
    rng::ThreadRng,
    sqlite_storage::SqliteStorage,
};
use trilithon_core::{
    clock::SystemClock,
    config::types::ServerConfig,
    http::HttpServer,
    schema::SchemaRegistry,
    storage::{trait_def::Storage, types::AuditSelector},
};

async fn setup() -> (
    TempDir,
    SocketAddr,
    tokio::sync::oneshot::Sender<()>,
    Arc<dyn Storage>,
    String, // session cookie
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
    let storage_for_state = Arc::clone(&storage_arc);
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
        applier: Arc::new(NoopApplier),
        storage: storage_for_state,
        diff_engine: Arc::new(trilithon_core::diff::DefaultDiffEngine),
        schema_registry: Arc::new(trilithon_core::schema::SchemaRegistry::with_tier1_secrets()),
        hasher: Arc::new(trilithon_adapters::Sha256AuditHasher),
    });

    let cfg = AxumServerConfig {
        bind_port: 0,
        ..AxumServerConfig::default()
    };
    let mut server = AxumServer::new(cfg, Arc::clone(&state));
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

    // Login to get a session cookie.
    let client = reqwest::Client::new();
    let login_resp = client
        .post(format!("http://{addr}/api/v1/auth/login"))
        .json(&serde_json::json!({"username": "alice", "password": "correct-horse-battery"}))
        .send()
        .await
        .expect("login");
    assert_eq!(login_resp.status(), 200);
    let cookie = login_resp
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();

    (dir, addr, tx, storage_arc, cookie)
}

#[tokio::test]
async fn mutation_missing_expected_version_400() {
    let (_dir, addr, tx, storage, cookie) = setup().await;

    let client = reqwest::Client::new();

    // Body without expected_version.
    let resp = client
        .post(format!("http://{addr}/api/v1/mutations"))
        .header("Cookie", &cookie)
        .json(&serde_json::json!({
            "body": {"kind": "SetGlobalConfig", "expected_version": 0, "patch": {}}
        }))
        .send()
        .await
        .expect("request");

    assert_eq!(
        resp.status(),
        400,
        "missing expected_version must return 400"
    );
    let body: serde_json::Value = resp.json().await.expect("json body");
    assert_eq!(
        body["code"], "missing-expected-version",
        "body must carry the correct code"
    );

    // An audit row for mutation.rejected.missing-expected-version must exist.
    let rows = storage
        .tail_audit_log(AuditSelector::default(), 10)
        .await
        .expect("tail");
    let found = rows
        .iter()
        .any(|r| r.kind == "mutation.rejected.missing-expected-version");
    assert!(
        found,
        "expected mutation.rejected.missing-expected-version audit row; got: {rows:?}"
    );

    let _ = tx.send(());
}
