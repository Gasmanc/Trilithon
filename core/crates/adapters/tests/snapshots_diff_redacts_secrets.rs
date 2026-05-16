//! `GET /api/v1/snapshots/{a}/diff/{b}` — secrets are redacted in the response.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::items_after_statements
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
    audit::redactor::REDACTION_PREFIX,
    canonical_json::{CANONICAL_JSON_VERSION, content_address_bytes, to_canonical_bytes},
    clock::SystemClock,
    config::types::ServerConfig,
    http::HttpServer,
    model::{
        desired_state::DesiredState,
        header::HeaderRules,
        identifiers::RouteId,
        matcher::MatcherSet,
        primitive::JsonPointer,
        route::{HostPattern, Route},
    },
    schema::SchemaRegistry,
    storage::{
        trait_def::Storage,
        types::{Snapshot, SnapshotId},
    },
};

fn make_snapshot_from_state(state: &DesiredState) -> Snapshot {
    let bytes = to_canonical_bytes(state).expect("canonical");
    let hash = content_address_bytes(&bytes);
    let json = String::from_utf8(bytes).expect("utf8");
    Snapshot {
        snapshot_id: SnapshotId(hash),
        parent_id: None,
        config_version: state.version,
        actor: "test-actor".to_owned(),
        intent: "test".to_owned(),
        correlation_id: format!("corr-{}", state.version),
        caddy_version: "2.8.0".to_owned(),
        trilithon_version: "0.1.0".to_owned(),
        created_at_unix_seconds: 1_700_000_000 + state.version,
        created_at_monotonic_nanos: (1_700_000_000_u64 + state.version as u64) * 1_000_000_000,
        canonical_json_version: CANONICAL_JSON_VERSION,
        desired_state_json: json,
    }
}

async fn setup() -> (
    TempDir,
    SocketAddr,
    tokio::sync::oneshot::Sender<()>,
    String,
    String,
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

    // Build two desired states that differ in route hostnames.
    // The diff will contain added/removed hostname strings — not secret fields —
    // so we embed a "secret-looking" field using the unknown_extensions map
    // at a path that the redactor treats as secret.
    // Per the schema, /forward_auth/*/secret is a Tier-1 secret field.
    // We inject it via unknown_extensions so both states differ there.
    let mut state_a = DesiredState::empty();
    state_a.version = 1;
    // Add a route with a hostname.
    state_a.routes.insert(
        RouteId("01ROUTEAAA00000000000000A1".to_owned()),
        Route {
            id: RouteId("01ROUTEAAA00000000000000A1".to_owned()),
            hostnames: vec![HostPattern::Exact("a.example.com".to_owned())],
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
    state_a.unknown_extensions.insert(
        JsonPointer("/forward_auth/myapp/secret".to_owned()),
        serde_json::json!("supersecretvalue"),
    );

    let mut state_b = DesiredState::empty();
    state_b.version = 2;
    state_b.routes.insert(
        RouteId("01ROUTEAAA00000000000000A1".to_owned()),
        Route {
            id: RouteId("01ROUTEAAA00000000000000A1".to_owned()),
            hostnames: vec![HostPattern::Exact("b.example.com".to_owned())],
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
    state_b.unknown_extensions.insert(
        JsonPointer("/forward_auth/myapp/secret".to_owned()),
        serde_json::json!("newsecretvalue"),
    );

    let snap_a = make_snapshot_from_state(&state_a);
    let snap_b = make_snapshot_from_state(&state_b);
    let id_a = snap_a.snapshot_id.0.clone();
    let id_b = snap_b.snapshot_id.0.clone();

    let storage_arc: Arc<dyn Storage> = Arc::new(storage);
    storage_arc
        .insert_snapshot(snap_a)
        .await
        .expect("insert snap_a");
    storage_arc
        .insert_snapshot(snap_b)
        .await
        .expect("insert snap_b");

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

    (dir, addr, tx, cookie, id_a, id_b)
}

#[tokio::test]
async fn snapshots_diff_redacts_secrets() {
    let (_dir, addr, tx, cookie, id_a, id_b) = setup().await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/api/v1/snapshots/{id_a}/diff/{id_b}"))
        .header(COOKIE, &cookie)
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");

    // Verify the response contains the expected fields.
    assert!(
        body.get("redacted_diff_json").is_some(),
        "must have redacted_diff_json"
    );
    assert!(
        body.get("redaction_sites").is_some(),
        "must have redaction_sites"
    );

    // Plaintext secret values must NOT appear anywhere in the response.
    let body_str = body.to_string();
    assert!(
        !body_str.contains("supersecretvalue"),
        "plaintext secret must not appear: {body_str}"
    );
    assert!(
        !body_str.contains("newsecretvalue"),
        "plaintext new secret must not appear: {body_str}"
    );

    // The redaction marker must appear (at least one site).
    assert!(
        body_str.contains(REDACTION_PREFIX),
        "redaction marker must appear in response"
    );

    let sites = body["redaction_sites"].as_u64().expect("u64");
    assert!(sites > 0, "redaction_sites must be > 0");

    let _ = tx.send(());
}
