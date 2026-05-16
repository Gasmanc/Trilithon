//! Valid bearer token seeded into the tokens table → middleware lets the request
//! through. We call the logout endpoint with a bearer token and assert the
//! middleware admitted the request (response is not a middleware-level 401 with
//! `{"code":"unauthenticated"}`). The handler may still reject with its own 401.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: integration test

use sha2::{Digest as _, Sha256};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::Duration;

use tempfile::TempDir;
use trilithon_adapters::{
    AuditWriter, Sha256AuditHasher,
    auth::{LoginRateLimiter, SqliteSessionStore, SqliteUserStore},
    http_axum::{AppState, AxumServer, AxumServerConfig},
    migrate::apply_migrations,
    rng::ThreadRng,
    sqlite_storage::SqliteStorage,
};
use trilithon_core::{
    clock::SystemClock, config::types::ServerConfig, http::HttpServer, schema::SchemaRegistry,
    storage::trait_def::Storage,
};

async fn setup() -> (TempDir, SocketAddr, tokio::sync::oneshot::Sender<()>) {
    let dir = TempDir::new().unwrap();
    let storage = SqliteStorage::open(dir.path())
        .await
        .expect("SqliteStorage::open");
    apply_migrations(storage.pool()).await.expect("migrations");

    let pool = storage.pool().clone();
    let user_store = Arc::new(SqliteUserStore::new(pool.clone()));
    let session_store = Arc::new(SqliteSessionStore::new(pool.clone(), Arc::new(ThreadRng)));

    // Seed a token row. The raw token is "test-token-abc123"; SHA-256 hex is computed here.
    let raw_token = "test-token-abc123";
    let hash = format!("{:x}", Sha256::digest(raw_token.as_bytes()));
    let now: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .try_into()
        .unwrap_or(0);
    sqlx::query(
        "INSERT INTO tokens (token_id, token_hash, permissions, rate_limit_qps, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind("tok_01")
    .bind(&hash)
    .bind("{}")
    .bind(10i64)
    .bind(now)
    .execute(&pool)
    .await
    .expect("seed token");

    let storage_arc: Arc<dyn Storage> = Arc::new(storage);
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
        token_pool: Some(pool),
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

    (dir, addr, tx)
}

#[tokio::test]
async fn auth_middleware_token_admits() {
    let (_dir, addr, tx) = setup().await;

    // POST to a protected endpoint with the bearer token.
    // The middleware should admit the request (no "unauthenticated" 401).
    // The logout handler will return 401 because it requires session context,
    // but the body should say "unauthorized" (handler rejection), not
    // "unauthenticated" (middleware rejection).
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/api/v1/auth/logout"))
        .header("Authorization", "Bearer test-token-abc123")
        .send()
        .await
        .expect("request");

    // The middleware admitted the request (token is valid).
    // Handler rejects with 401 because token context has no session_id.
    // What matters is that the response is NOT the middleware's "unauthenticated" 401.
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.expect("JSON body");

    // The middleware "unauthenticated" 401 produces {"code":"unauthenticated"}.
    // A handler rejection produces {"error":"unauthorized"}.
    // Either way, we should NOT see code == "unauthenticated" from the middleware.
    assert_ne!(
        body.get("code").and_then(|v| v.as_str()),
        Some("unauthenticated"),
        "middleware must not reject a valid bearer token (status={status}, body={body})"
    );

    let _ = tx.send(());
}
