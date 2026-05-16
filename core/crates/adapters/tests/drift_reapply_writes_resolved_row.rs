//! `POST /api/v1/drift/{event_id}/reapply` — asserts exactly one `config.drift-resolved` audit row.

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
    http_axum::{AppState, AxumServer, AxumServerConfig, stubs},
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
        types::{AuditSelector, DriftEventRow, DriftRowId, Snapshot, SnapshotId},
    },
};

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
        applier: Arc::new(SucceedingApplier),
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
    assert_eq!(resp.status(), 200);
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

async fn seed_drift(storage: &Arc<dyn Storage>) -> DriftRowId {
    use trilithon_core::{
        canonical_json::{CANONICAL_JSON_VERSION, content_address_bytes, to_canonical_bytes},
        model::desired_state::DesiredState,
    };

    let state = DesiredState::default();
    let json_bytes = to_canonical_bytes(&state).unwrap();
    let json_str = String::from_utf8(json_bytes.clone()).unwrap();
    let snapshot_id = SnapshotId(content_address_bytes(&json_bytes));
    let snap = Snapshot {
        snapshot_id: snapshot_id.clone(),
        parent_id: None,
        config_version: 1,
        actor: "test".to_owned(),
        intent: "seed".to_owned(),
        correlation_id: "01ARZ3NDEKTSV4RRFFQ69G5FAE".to_owned(),
        caddy_version: "2.8.0".to_owned(),
        trilithon_version: "0.1.0".to_owned(),
        created_at_unix_seconds: 1_700_000_000,
        created_at_monotonic_nanos: 0,
        canonical_json_version: CANONICAL_JSON_VERSION,
        desired_state_json: json_str,
    };
    storage.insert_snapshot(snap).await.unwrap();

    let row_id = DriftRowId("01JDRIFTROW000000000000002".to_owned());
    let drift_row = DriftEventRow {
        id: row_id.clone(),
        correlation_id: "01ARZ3NDEKTSV4RRFFQ69G5FAB".to_owned(),
        detected_at: 1_700_000_000,
        snapshot_id,
        diff_json: r#"{"entries":[]}"#.to_owned(),
        running_state_hash: "def456".to_owned(),
        resolution: None,
        resolved_at: None,
    };
    storage.record_drift_event(drift_row).await.unwrap();
    row_id
}

#[tokio::test]
async fn drift_reapply_writes_resolved_row() {
    let (_dir, storage, addr, tx) = setup().await;
    let row_id = seed_drift(&storage).await;
    let cookie = login(addr).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/api/v1/drift/{}/reapply", row_id.0))
        .header("cookie", &cookie)
        .send()
        .await
        .expect("POST reapply");

    assert_eq!(
        resp.status(),
        200,
        "reapply should return 200; body: {:?}",
        resp.text().await
    );

    let rows = storage
        .tail_audit_log(
            AuditSelector {
                kind_glob: Some("config.drift-resolved".to_owned()),
                ..Default::default()
            },
            100,
        )
        .await
        .expect("tail_audit_log");

    assert_eq!(
        rows.len(),
        1,
        "expected exactly 1 config.drift-resolved row"
    );
    let notes = rows[0].notes.as_deref().unwrap_or("");
    assert!(
        notes.contains("reapply"),
        "notes should contain 'reapply', got: {notes}"
    );

    let _ = tx.send(());
}
