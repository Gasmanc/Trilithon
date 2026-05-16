//! `GET /api/v1/routes?hostname_filter=…` — case-insensitive substring match.

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
    model::{
        desired_state::DesiredState,
        header::HeaderRules,
        identifiers::RouteId,
        matcher::MatcherSet,
        route::{HostPattern, Route},
    },
    schema::SchemaRegistry,
    storage::{
        trait_def::Storage,
        types::{Snapshot, SnapshotId},
    },
};

async fn seed_state(storage: &Arc<dyn Storage>) {
    let mut state = DesiredState::empty();
    state.version = 1;

    // Three routes: two on "alpha.example.com", one on "beta.other.org".
    for (id, hostname) in [
        ("ROUTE0000000000000000ALPHA1", "alpha.example.com"),
        ("ROUTE0000000000000000ALPHA2", "ALPHA2.example.com"),
        ("ROUTE0000000000000000BETA00", "beta.other.org"),
    ] {
        state.routes.insert(
            RouteId(id.to_owned()),
            Route {
                id: RouteId(id.to_owned()),
                hostnames: vec![HostPattern::Exact(hostname.to_owned())],
                upstreams: vec![],
                matchers: MatcherSet::default(),
                headers: HeaderRules::default(),
                redirects: None,
                policy_attachment: None,
                enabled: true,
                created_at: 0,
                updated_at: 0,
            },
        );
    }

    let bytes = to_canonical_bytes(&state).expect("canonical");
    let hash = content_address_bytes(&bytes);
    let json = String::from_utf8(bytes).expect("utf8");
    storage
        .insert_snapshot(Snapshot {
            snapshot_id: SnapshotId(hash),
            parent_id: None,
            config_version: 1,
            actor: "test".to_owned(),
            intent: "seed".to_owned(),
            correlation_id: "corr-seed".to_owned(),
            caddy_version: "2.8.0".to_owned(),
            trilithon_version: "0.1.0".to_owned(),
            created_at_unix_seconds: 1_700_000_000,
            created_at_monotonic_nanos: 1_700_000_000_000_000_000,
            canonical_json_version: CANONICAL_JSON_VERSION,
            desired_state_json: json,
        })
        .await
        .expect("insert snapshot");
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
    seed_state(&storage_arc).await;

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

    let client = reqwest::Client::new();
    let login = client
        .post(format!("http://{addr}/api/v1/auth/login"))
        .json(&serde_json::json!({"username": "alice", "password": "correct-horse-battery"}))
        .send()
        .await
        .expect("login");
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
async fn routes_list_hostname_filter_matches_two() {
    let (_dir, addr, tx, cookie) = setup().await;

    let client = reqwest::Client::new();
    // "alpha" should match both alpha routes (case-insensitive).
    let resp = client
        .get(format!("http://{addr}/api/v1/routes?hostname_filter=alpha"))
        .header(COOKIE, &cookie)
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");
    let items = body.as_array().expect("array");
    assert_eq!(items.len(), 2, "filter=alpha must match 2 routes");

    for item in items {
        let hostnames: Vec<&str> = item["hostnames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let matches = hostnames.iter().any(|h| h.to_lowercase().contains("alpha"));
        assert!(matches, "all returned routes must match the filter");
    }

    let _ = tx.send(());
}

#[tokio::test]
async fn routes_list_hostname_filter_case_insensitive() {
    let (_dir, addr, tx, cookie) = setup().await;

    let client = reqwest::Client::new();
    // "ALPHA" upper-case should still match.
    let resp = client
        .get(format!("http://{addr}/api/v1/routes?hostname_filter=ALPHA"))
        .header(COOKIE, &cookie)
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");
    let items = body.as_array().expect("array");
    assert_eq!(
        items.len(),
        2,
        "case-insensitive filter must match 2 routes"
    );

    let _ = tx.send(());
}

#[tokio::test]
async fn routes_list_hostname_filter_no_match() {
    let (_dir, addr, tx, cookie) = setup().await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "http://{addr}/api/v1/routes?hostname_filter=notfound"
        ))
        .header(COOKIE, &cookie)
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");
    let items = body.as_array().expect("array");
    assert!(items.is_empty(), "no-match filter must return empty list");

    let _ = tx.send(());
}
