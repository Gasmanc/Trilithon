//! `POST /api/v1/mutations` happy path — 200 with `snapshot_id` and `config_version`.

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

use async_trait::async_trait;
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
    clock::SystemClock,
    config::types::ServerConfig,
    http::HttpServer,
    reconciler::{AppliedState, Applier, ApplyError, ApplyOutcome, ReloadKind, ValidationReport},
    schema::SchemaRegistry,
    storage::{
        trait_def::Storage,
        types::{Snapshot, SnapshotId},
    },
};

// ── SucceedingApplier ─────────────────────────────────────────────────────────

/// An [`Applier`] that always returns a successful outcome using the snapshot's
/// own `snapshot_id` and `config_version`.
struct SucceedingApplier;

#[async_trait]
impl Applier for SucceedingApplier {
    async fn apply(
        &self,
        snapshot: &Snapshot,
        _expected_version: i64,
    ) -> Result<ApplyOutcome, ApplyError> {
        Ok(ApplyOutcome::Succeeded {
            snapshot_id: snapshot.snapshot_id.clone(),
            config_version: snapshot.config_version,
            applied_state: AppliedState::Applied,
            reload_kind: ReloadKind::Graceful {
                drain_window_ms: None,
            },
            latency_ms: 0,
        })
    }

    async fn validate(&self, _snapshot: &Snapshot) -> Result<ValidationReport, ApplyError> {
        Ok(ValidationReport::default())
    }

    async fn rollback(&self, _target: &SnapshotId) -> Result<ApplyOutcome, ApplyError> {
        Err(ApplyError::Storage("noop".to_owned()))
    }
}

// ── Test setup ────────────────────────────────────────────────────────────────

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
        applier: Arc::new(SucceedingApplier),
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

    (dir, addr, tx, cookie)
}

#[tokio::test]
async fn mutation_happy_path() {
    let (_dir, addr, tx, cookie) = setup().await;

    let client = reqwest::Client::new();

    // SetGlobalConfig with a real change to apply to an empty state.
    let resp = client
        .post(format!("http://{addr}/api/v1/mutations"))
        .header("Cookie", &cookie)
        .json(&serde_json::json!({
            "expected_version": 0,
            "body": {
                "kind": "SetGlobalConfig",
                "expected_version": 0,
                "patch": {"log_level": "info"}
            }
        }))
        .send()
        .await
        .expect("request");

    let status = resp.status();
    let body_text = resp.text().await.expect("body text");
    assert_eq!(status, 200, "happy path must return 200; body: {body_text}");

    let body: serde_json::Value = serde_json::from_str(&body_text).expect("json body");
    assert!(
        body["snapshot_id"].is_string(),
        "response must include snapshot_id; got: {body}"
    );
    assert!(
        body["config_version"].is_number(),
        "response must include config_version; got: {body}"
    );
    assert_eq!(
        body["config_version"], 1,
        "config_version must be 1 after first mutation"
    );

    let _ = tx.send(());
}
