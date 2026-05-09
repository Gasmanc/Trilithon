//! Slice 7.6 — advisory lock is released when the apply body panics.
//!
//! A fake Caddy client that panics on `load_config` is used to simulate an
//! apply body panic.  After catching the panic, a second normal apply must
//! succeed — proving that the advisory lock row was deleted by `AcquiredLock::drop`.
//!
//! Note: the in-process `Mutex` is also released on panic via the standard
//! Rust poisoning mechanism.  A `Mutex::lock()` on a poisoned mutex returns
//! `Err(PoisonError)`.  To avoid that complication this test gives each call
//! its own fresh mutex, simulating two independent process-level apply attempts
//! that both target the same `SQLite` advisory lock row.

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
    canonical_json::{CANONICAL_JSON_VERSION, content_address_bytes, to_canonical_bytes},
    clock::Clock,
    diff::NoOpDiffEngine,
    model::desired_state::DesiredState,
    reconciler::{Applier, ApplyOutcome, DefaultCaddyJsonRenderer},
    schema::SchemaRegistry,
    storage::{
        trait_def::Storage,
        types::{Snapshot, SnapshotId},
    },
};

// ── Fakes ─────────────────────────────────────────────────────────────────────

struct PanicCaddyClient;

#[async_trait]
impl CaddyClient for PanicCaddyClient {
    async fn load_config(&self, _: CaddyConfig) -> Result<(), CaddyError> {
        panic!("simulated panic in load_config");
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

struct OkCaddyClient;

#[async_trait]
impl CaddyClient for OkCaddyClient {
    async fn load_config(&self, _: CaddyConfig) -> Result<(), CaddyError> {
        Ok(())
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
        Ok(CaddyConfig(serde_json::json!({})))
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
        .expect("SqliteStorage::open");
    apply_migrations(store.pool())
        .await
        .expect("apply_migrations");
    store
}

fn make_snapshot(config_version: i64) -> Snapshot {
    let mut state = DesiredState::empty();
    state.version = config_version;
    let body = to_canonical_bytes(&state)
        .map(|b| String::from_utf8(b).expect("canonical JSON is UTF-8"))
        .expect("serialise DesiredState");
    let id = SnapshotId(content_address_bytes(body.as_bytes()));
    Snapshot {
        snapshot_id: id,
        parent_id: None,
        config_version,
        actor: "test".to_owned(),
        intent: format!("v{config_version}"),
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

fn build_panicking_applier(storage: Arc<dyn Storage>, lock_pool: SqlitePool) -> CaddyApplier {
    let registry = Box::leak(Box::new(SchemaRegistry::with_tier1_secrets()));
    let hasher = Box::leak(Box::new(ZeroHasher));
    let redactor = SecretsRedactor::new(registry, hasher);
    let audit = Arc::new(AuditWriter::new(
        storage.clone(),
        Arc::new(FixedClock(1_700_000_000_000)),
        redactor,
    ));
    CaddyApplier {
        client: Arc::new(PanicCaddyClient),
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

fn build_ok_applier(storage: Arc<dyn Storage>, lock_pool: SqlitePool) -> CaddyApplier {
    let registry = Box::leak(Box::new(SchemaRegistry::with_tier1_secrets()));
    let hasher = Box::leak(Box::new(ZeroHasher));
    let redactor = SecretsRedactor::new(registry, hasher);
    let audit = Arc::new(AuditWriter::new(
        storage.clone(),
        Arc::new(FixedClock(1_700_000_000_000)),
        redactor,
    ));
    CaddyApplier {
        client: Arc::new(OkCaddyClient),
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

// ── Test ──────────────────────────────────────────────────────────────────────

/// A panicking apply releases the advisory lock so a subsequent apply can succeed.
#[tokio::test]
async fn lock_released_after_panic_allows_next_apply() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let pool = store.pool().clone();
    let storage: Arc<dyn Storage> = Arc::new(store);

    // Insert two snapshots so both applies have something to work with.
    for v in 1..=2_i64 {
        storage
            .insert_snapshot(make_snapshot(v))
            .await
            .expect("insert snapshot");
    }

    // First apply: panics inside load_config.  Spawn in a task so the panic
    // is caught by tokio and returned as a JoinError.
    let storage_c = storage.clone();
    let pool_c = pool.clone();
    let panicking = tokio::spawn(async move {
        let applier = build_panicking_applier(storage_c, pool_c);
        let snap = make_snapshot(1);
        applier.apply(&snap, 0).await
    });

    let join_result = panicking.await;
    // The task panicked; that is expected.
    assert!(
        join_result.is_err(),
        "expected the task to panic, got Ok instead"
    );

    // Give the drop handler a moment to run inside its spawn_blocking task.
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Second apply on a fresh applier with a fresh mutex — must succeed,
    // proving the advisory lock row was cleaned up.
    let applier2 = build_ok_applier(storage.clone(), pool);
    let snap2 = make_snapshot(2);
    // Note: config_version is now 1 (the panicking apply did CAS-advance before
    // panicking in load_config).  If the lock row is not cleaned up this would
    // return LockContested instead of Succeeded.
    let outcome = applier2
        .apply(&snap2, 1)
        .await
        .expect("second apply must not return Err");

    assert!(
        matches!(outcome, ApplyOutcome::Succeeded { .. }),
        "second apply must succeed after lock cleanup; got {outcome:?}"
    );
}
