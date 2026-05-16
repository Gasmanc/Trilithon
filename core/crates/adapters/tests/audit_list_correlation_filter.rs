//! `GET /api/v1/audit?correlation_id=<id>` — returns only the matching row.

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
    clock::SystemClock,
    config::types::ServerConfig,
    http::HttpServer,
    schema::SchemaRegistry,
    storage::{
        helpers::audit_prev_hash_seed,
        trait_def::Storage,
        types::{ActorKind, AuditEventRow, AuditOutcome, AuditRowId},
    },
};

#[allow(clippy::too_many_lines)]
// reason: integration test setup is inherently verbose; no logic duplication
async fn setup() -> (
    TempDir,
    SocketAddr,
    tokio::sync::oneshot::Sender<()>,
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
        .create_user("dave", "correct-horse-battery-staple", UserRole::Owner)
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

    // The specific correlation id we will filter by.
    let target_correlation = ulid::Ulid::new().to_string();

    // Insert 3 rows with different correlation ids, one with our target.
    for i in 0u32..3 {
        let correlation_id = if i == 1 {
            target_correlation.clone()
        } else {
            ulid::Ulid::new().to_string()
        };
        let row = AuditEventRow {
            id: AuditRowId(ulid::Ulid::new().to_string()),
            prev_hash: audit_prev_hash_seed().to_owned(),
            caddy_instance_id: "local".to_owned(),
            correlation_id,
            occurred_at: 1_700_000_000 + i64::from(i),
            occurred_at_ms: (1_700_000_000 + i64::from(i)) * 1000,
            actor_kind: ActorKind::System,
            actor_id: "test".to_owned(),
            kind: "config.applied".to_owned(),
            target_kind: None,
            target_id: None,
            snapshot_id: None,
            redacted_diff_json: None,
            redaction_sites: 0,
            outcome: AuditOutcome::Ok,
            error_kind: None,
            notes: None,
        };
        storage_arc
            .record_audit_event(row)
            .await
            .expect("record_audit_event");
    }

    let (user, _) = user_store
        .find_by_username("dave")
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

    (dir, addr, tx, session.id, target_correlation)
}

#[tokio::test]
async fn audit_list_correlation_filter() {
    let (_dir, addr, tx, session_id, target_correlation) = setup().await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "http://{addr}/api/v1/audit?correlation_id={target_correlation}"
        ))
        .header("Cookie", format!("trilithon_session={session_id}"))
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 200, "expected 200 OK");

    let body: serde_json::Value = resp.json().await.expect("JSON body");
    let rows = body.as_array().expect("response must be a JSON array");
    assert_eq!(
        rows.len(),
        1,
        "correlation_id filter must return exactly 1 row but got {}",
        rows.len()
    );
    assert_eq!(
        rows[0]["correlation_id"], target_correlation,
        "returned row must have the requested correlation_id"
    );

    let _ = tx.send(());
}
