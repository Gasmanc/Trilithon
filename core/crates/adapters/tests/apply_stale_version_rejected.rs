//! Slice 7.5 — stale `expected_version` is rejected with `Conflicted` outcome.
//!
//! Scenario: DB is at `config_version = 10`; the caller passes
//! `expected_version = 9`.  The apply must return
//! `ApplyOutcome::Conflicted { stale_version: 9, current_version: 10 }`
//! without touching Caddy.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::disallowed_methods
)]
// reason: integration test — panics and unwrap are the correct failure mode here

use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;
use tempfile::TempDir;
use trilithon_adapters::caddy::cache::CapabilityCache;
use trilithon_adapters::{
    CaddyApplier, audit_writer::AuditWriter, migrate::apply_migrations,
    sqlite_storage::SqliteStorage,
};
use trilithon_core::{
    audit::redactor::SecretsRedactor,
    caddy::{
        CaddyClient, CaddyConfig, CaddyError, CaddyJsonPointer, HealthState, JsonPatch,
        LoadedModules, TlsCertificate, UpstreamHealth,
    },
    canonical_json::{CANONICAL_JSON_VERSION, content_address_bytes},
    clock::Clock,
    diff::NoOpDiffEngine,
    reconciler::{Applier, ApplyOutcome, DefaultCaddyJsonRenderer},
    schema::SchemaRegistry,
    storage::{
        trait_def::Storage,
        types::{AuditSelector, Snapshot, SnapshotId},
    },
};

// ── Fakes ─────────────────────────────────────────────────────────────────────

/// A Caddy client that must never be called (asserts if it is).
struct NeverCalledCaddyClient;

#[async_trait]
impl CaddyClient for NeverCalledCaddyClient {
    async fn load_config(&self, _: CaddyConfig) -> Result<(), CaddyError> {
        panic!("load_config must not be called on a conflicted apply");
    }

    async fn patch_config(&self, _: CaddyJsonPointer, _: JsonPatch) -> Result<(), CaddyError> {
        unimplemented!()
    }

    async fn put_config(
        &self,
        _: CaddyJsonPointer,
        _: serde_json::Value,
    ) -> Result<(), CaddyError> {
        unimplemented!()
    }

    async fn get_running_config(&self) -> Result<CaddyConfig, CaddyError> {
        unimplemented!()
    }

    async fn get_loaded_modules(&self) -> Result<LoadedModules, CaddyError> {
        unimplemented!()
    }

    async fn get_upstream_health(&self) -> Result<Vec<UpstreamHealth>, CaddyError> {
        unimplemented!()
    }

    async fn get_certificates(&self) -> Result<Vec<TlsCertificate>, CaddyError> {
        unimplemented!()
    }

    async fn health_check(&self) -> Result<HealthState, CaddyError> {
        unimplemented!()
    }
}

struct FixedClock(i64);
impl Clock for FixedClock {
    fn now_unix_ms(&self) -> i64 {
        self.0
    }
}

struct ZeroHasher;
impl trilithon_core::audit::redactor::CiphertextHasher for ZeroHasher {
    fn hash_for_value(&self, _: &str) -> String {
        "000000000000".to_owned()
    }
}

async fn open_store(dir: &TempDir) -> SqliteStorage {
    let store = SqliteStorage::open(dir.path())
        .await
        .expect("SqliteStorage::open should succeed");
    apply_migrations(store.pool())
        .await
        .expect("apply_migrations should succeed");
    store
}

/// Build a content-addressed snapshot with the given `config_version`.
fn make_snapshot(config_version: i64) -> Snapshot {
    let body = format!("{{\"_v\":{config_version}}}");
    let id = SnapshotId(content_address_bytes(body.as_bytes()));
    Snapshot {
        snapshot_id: id,
        parent_id: None,
        config_version,
        actor: "test".to_owned(),
        intent: format!("test v{config_version}"),
        correlation_id: "01HCORRELATION0000000000AB".to_owned(),
        caddy_version: "2.8.0".to_owned(),
        trilithon_version: "0.1.0".to_owned(),
        created_at_unix_seconds: 1_700_000_000 + config_version,
        #[allow(clippy::cast_sign_loss)]
        // reason: test fixture; config_version is always positive
        created_at_monotonic_nanos: (1_700_000_000_u64 + config_version as u64) * 1_000_000_000,
        canonical_json_version: CANONICAL_JSON_VERSION,
        desired_state_json: body,
    }
}

fn build_applier(storage: Arc<dyn Storage>, lock_pool: SqlitePool) -> CaddyApplier {
    let registry = Box::leak(Box::new(SchemaRegistry::with_tier1_secrets()));
    let hasher = Box::leak(Box::new(ZeroHasher));
    let redactor = SecretsRedactor::new(registry, hasher);
    let audit = Arc::new(AuditWriter::new(
        storage.clone(),
        Arc::new(FixedClock(1_700_000_000_000)),
        redactor,
    ));
    CaddyApplier {
        client: Arc::new(NeverCalledCaddyClient),
        renderer: Arc::new(DefaultCaddyJsonRenderer),
        diff_engine: Arc::new(NoOpDiffEngine),
        capabilities: Arc::new(CapabilityCache::default()),
        audit,
        storage,
        instance_id: "local".to_owned(),
        clock: Arc::new(FixedClock(1_700_000_000_000)),
        instance_mutex: Arc::new(tokio::sync::Mutex::new(())),
        lock_pool,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// DB at version 10; `expected_version` = 9 → Conflicted.
#[tokio::test]
async fn stale_expected_version_returns_conflicted() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let pool = store.pool().clone();
    let storage: Arc<dyn Storage> = Arc::new(store);

    // Build versions 1 through 10.
    for v in 1..=10_i64 {
        storage
            .insert_snapshot(make_snapshot(v))
            .await
            .expect("insert snapshot");
    }

    // Mark version 10 as currently applied so CAS reads observed=10.
    sqlx::query("UPDATE caddy_instances SET applied_config_version = 10 WHERE id = 'local'")
        .execute(&pool)
        .await
        .expect("set applied_config_version");

    // The snapshot we "want to apply" has version 10 but we claim expected=9.
    let snapshot_v10 = make_snapshot(10);
    let applier = build_applier(storage.clone(), pool);

    let outcome = applier
        .apply(&snapshot_v10, 9)
        .await
        .expect("apply must return Ok");

    assert!(
        matches!(
            outcome,
            ApplyOutcome::Conflicted {
                stale_version: 9,
                current_version: 10
            }
        ),
        "expected Conflicted(stale=9, current=10), got {outcome:?}"
    );
}

/// Conflict produces exactly one `mutation.conflicted` audit row.
#[tokio::test]
async fn stale_expected_version_writes_conflict_audit_row() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let pool = store.pool().clone();
    let storage: Arc<dyn Storage> = Arc::new(store);

    for v in 1..=10_i64 {
        storage
            .insert_snapshot(make_snapshot(v))
            .await
            .expect("insert snapshot");
    }

    // Mark version 10 as currently applied so CAS reads observed=10.
    sqlx::query("UPDATE caddy_instances SET applied_config_version = 10 WHERE id = 'local'")
        .execute(&pool)
        .await
        .expect("set applied_config_version");

    let snapshot_v10 = make_snapshot(10);
    let applier = build_applier(storage.clone(), pool);
    applier
        .apply(&snapshot_v10, 9)
        .await
        .expect("apply must return Ok");

    let rows = storage
        .tail_audit_log(
            AuditSelector {
                kind_glob: Some("mutation.conflicted".to_owned()),
                ..Default::default()
            },
            10,
        )
        .await
        .expect("tail_audit_log");

    assert_eq!(
        rows.len(),
        1,
        "exactly one mutation.conflicted row must be written"
    );
    assert_eq!(rows[0].kind, "mutation.conflicted");
}
