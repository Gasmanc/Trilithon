//! Slice 7.8 — `apply()` must return without blocking on TLS issuance.
//!
//! Asserts that when a `TlsIssuanceObserver` is attached and the fake
//! `get_certificates` returns empty, `apply()` still returns immediately and
//! does NOT wait for the observer's polling loop to complete.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::disallowed_methods
)]
// reason: integration test — panics and unwrap are the correct failure mode here

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use sqlx::SqlitePool;
use tempfile::TempDir;
use trilithon_adapters::{
    CaddyApplier, TlsIssuanceObserver, audit_writer::AuditWriter, caddy::cache::CapabilityCache,
    migrate::apply_migrations, sqlite_storage::SqliteStorage,
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

// ── Fakes ──────────────────────────────────────────────────────────────────────

/// A `CaddyClient` that loads OK, returns empty config, and always returns
/// an empty certificate list (simulating cert not yet issued).
struct NoCertClient;

#[async_trait]
impl CaddyClient for NoCertClient {
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
        // Always returns empty — certs not yet issued.
        Ok(vec![])
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

async fn stored_snapshot(storage: &Arc<dyn Storage>, config_version: i64) -> Snapshot {
    let state = DesiredState::empty();
    let desired_state_json = to_canonical_bytes(&state)
        .map(|b| String::from_utf8(b).expect("canonical JSON is UTF-8"))
        .expect("serialise desired state");
    let snapshot_id = SnapshotId(content_address_bytes(desired_state_json.as_bytes()));
    let snapshot = Snapshot {
        snapshot_id,
        parent_id: None,
        config_version,
        actor: "test".to_owned(),
        intent: "test".to_owned(),
        correlation_id: "01HCORRELATION0000000000AB".to_owned(),
        caddy_version: "2.8.0".to_owned(),
        trilithon_version: "0.1.0".to_owned(),
        created_at_unix_seconds: 1_700_000_000,
        created_at_monotonic_nanos: 1_700_000_000_u64 * 1_000_000_000,
        canonical_json_version: CANONICAL_JSON_VERSION,
        desired_state_json,
    };
    storage
        .insert_snapshot(snapshot.clone())
        .await
        .expect("insert_snapshot");
    snapshot
}

fn build_applier(
    storage: Arc<dyn Storage>,
    lock_pool: SqlitePool,
    observer: Option<Arc<TlsIssuanceObserver>>,
) -> CaddyApplier {
    let registry = Box::leak(Box::new(SchemaRegistry::with_tier1_secrets()));
    let hasher = Box::leak(Box::new(ZeroHasher));
    let redactor = SecretsRedactor::new(registry, hasher);
    let audit = Arc::new(AuditWriter::new(
        storage.clone(),
        Arc::new(FixedClock(1_700_000_000_000)),
        redactor,
    ));
    CaddyApplier {
        client: Arc::new(NoCertClient),
        renderer: Arc::new(DefaultCaddyJsonRenderer),
        diff_engine: Arc::new(NoOpDiffEngine),
        capabilities: Arc::new(CapabilityCache::default()),
        audit,
        storage,
        instance_id: "local".to_owned(),
        clock: Arc::new(FixedClock(1_700_000_000_000)),
        instance_mutex: Arc::new(tokio::sync::Mutex::new(())),
        lock_pool,
        tls_observer: observer,
    }
}

// ── Test ───────────────────────────────────────────────────────────────────────

/// `apply()` must return immediately even when the TLS observer is attached and
/// `get_certificates` always returns empty (certs not yet issued).
///
/// Uses `tokio::time::pause()` so the background task's polling sleeps do not
/// consume real wall-clock time.  The store is opened BEFORE pausing so `SQLite`
/// connection setup is not disrupted by paused time.
#[tokio::test]
async fn apply_does_not_block_on_tls_issuance() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    // Pause time after SQLite is open so the observer's sleeps do not
    // block the test on real wall time.
    tokio::time::pause();
    let pool = store.pool().clone();
    let storage: Arc<dyn Storage> = Arc::new(store);

    // Build an audit writer for the observer using the same storage.
    let registry = Box::leak(Box::new(SchemaRegistry::with_tier1_secrets()));
    let hasher = Box::leak(Box::new(ZeroHasher));
    let redactor = SecretsRedactor::new(registry, hasher);
    let audit = Arc::new(AuditWriter::new(
        storage.clone(),
        Arc::new(FixedClock(1_700_000_000_000)),
        redactor,
    ));

    let observer = Arc::new(TlsIssuanceObserver {
        client: Arc::new(NoCertClient),
        audit,
        timeout: Duration::from_secs(120),
    });

    let applier = build_applier(storage.clone(), pool, Some(observer));
    let snapshot = stored_snapshot(&storage, 1).await;

    // apply() must return an Ok(Succeeded) — the observer runs in background.
    let outcome = applier
        .apply(&snapshot, 0)
        .await
        .expect("apply must succeed");

    assert!(
        matches!(outcome, ApplyOutcome::Succeeded { .. }),
        "expected Succeeded, got {outcome:?}"
    );

    // The observer is now running in the background polling for certs.
    // With time paused, it is blocked on the sleep.  apply() returned
    // immediately — the test verifies this by reaching here without hanging.
}
