//! `GET /api/v1/snapshots` — pagination: default limit 50, explicit limit, cursor walk.

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

use reqwest::header::COOKIE;
use tempfile::TempDir;
use trilithon_adapters::{
    AuditWriter, Sha256AuditHasher,
    auth::{LoginRateLimiter, SqliteSessionStore, SqliteUserStore, UserRole, UserStore as _},
    http_axum::{AppState, AxumServer, AxumServerConfig},
    migrate::apply_migrations,
    rng::ThreadRng,
    sqlite_storage::SqliteStorage,
};
use trilithon_core::{
    canonical_json::{CANONICAL_JSON_VERSION, content_address_bytes, to_canonical_bytes},
    clock::SystemClock,
    config::types::ServerConfig,
    http::HttpServer,
    model::desired_state::DesiredState,
    schema::SchemaRegistry,
    storage::{
        trait_def::Storage,
        types::{Snapshot, SnapshotId},
    },
};

async fn insert_n_snapshots(storage: &Arc<dyn Storage>, n: u32) {
    for i in 1..=n {
        let mut state = DesiredState::empty();
        state.version = i64::from(i);
        let bytes = to_canonical_bytes(&state).expect("canonical");
        let hash = content_address_bytes(&bytes);
        let json = String::from_utf8(bytes).expect("utf8");
        storage
            .insert_snapshot(Snapshot {
                snapshot_id: SnapshotId(hash),
                parent_id: None,
                config_version: i64::from(i),
                actor: "test-actor".to_owned(),
                intent: format!("insert-{i}"),
                correlation_id: format!("corr-{i}"),
                caddy_version: "2.8.0".to_owned(),
                trilithon_version: "0.1.0".to_owned(),
                created_at_unix_seconds: 1_700_000_000 + i64::from(i),
                created_at_monotonic_nanos: (1_700_000_000_u64 + u64::from(i)) * 1_000_000_000,
                canonical_json_version: CANONICAL_JSON_VERSION,
                desired_state_json: json,
            })
            .await
            .expect("insert snapshot");
    }
}

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
        .create_user("alice", "correct-horse-battery", UserRole::Owner)
        .await
        .expect("create user");

    let storage_arc: Arc<dyn Storage> = Arc::new(storage);
    insert_n_snapshots(&storage_arc, 60).await;

    let storage_for_state = Arc::clone(&storage_arc);
    let audit_writer = Arc::new(AuditWriter::new_with_arcs(
        storage_arc,
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
        applier: Arc::new(trilithon_adapters::http_axum::stubs::NoopApplier),
        storage: Arc::clone(&storage_for_state),
        diff_engine: Arc::new(trilithon_core::diff::DefaultDiffEngine),
        schema_registry: Arc::new(SchemaRegistry::with_tier1_secrets()),
        hasher: Arc::new(Sha256AuditHasher),
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

    // Log in to get a session cookie.
    let client = reqwest::Client::new();
    let login = client
        .post(format!("http://{addr}/api/v1/auth/login"))
        .json(&serde_json::json!({"username": "alice", "password": "correct-horse-battery"}))
        .send()
        .await
        .expect("login request");
    assert_eq!(login.status(), 200);
    let cookie = login
        .headers()
        .get("set-cookie")
        .expect("set-cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();

    (dir, addr, tx, cookie)
}

#[tokio::test]
async fn snapshots_list_default_limit_50() {
    let (_dir, addr, tx, cookie) = setup().await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/api/v1/snapshots"))
        .header(COOKIE, &cookie)
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");
    let items = body.as_array().expect("array");
    assert_eq!(items.len(), 50, "default limit must be 50");

    // Items must be in descending config_version order.
    let versions: Vec<i64> = items
        .iter()
        .map(|v| v["config_version"].as_i64().unwrap())
        .collect();
    assert!(
        versions.windows(2).all(|w| w[0] > w[1]),
        "must be descending"
    );

    let _ = tx.send(());
}

#[tokio::test]
async fn snapshots_list_explicit_limit_60() {
    let (_dir, addr, tx, cookie) = setup().await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/api/v1/snapshots?limit=60"))
        .header(COOKIE, &cookie)
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");
    let items = body.as_array().expect("array");
    assert_eq!(items.len(), 60, "limit=60 must return all 60 snapshots");

    let _ = tx.send(());
}

#[tokio::test]
async fn snapshots_list_cursor_walks() {
    let (_dir, addr, tx, cookie) = setup().await;

    let client = reqwest::Client::new();

    // First page: default 50.
    let resp1 = client
        .get(format!("http://{addr}/api/v1/snapshots"))
        .header(COOKIE, &cookie)
        .send()
        .await
        .expect("first page");
    assert_eq!(resp1.status(), 200);
    let page1: serde_json::Value = resp1.json().await.expect("json");
    let page1_items = page1.as_array().expect("array");
    assert_eq!(page1_items.len(), 50);

    // Cursor is the id of the last item on page 1.
    let last_id = page1_items.last().unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Second page via cursor.
    let resp2 = client
        .get(format!(
            "http://{addr}/api/v1/snapshots?cursor_after={last_id}"
        ))
        .header(COOKIE, &cookie)
        .send()
        .await
        .expect("second page");
    assert_eq!(resp2.status(), 200);
    let page2: serde_json::Value = resp2.json().await.expect("json");
    let page2_items = page2.as_array().expect("array");
    assert_eq!(
        page2_items.len(),
        10,
        "cursor should yield the remaining 10"
    );

    // No overlap between pages.
    let page1_ids: std::collections::HashSet<&str> = page1_items
        .iter()
        .map(|v| v["id"].as_str().unwrap())
        .collect();
    for item in page2_items {
        assert!(
            !page1_ids.contains(item["id"].as_str().unwrap()),
            "pages must not overlap"
        );
    }

    let _ = tx.send(());
}
