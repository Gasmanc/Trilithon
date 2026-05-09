//! Slice 7.6 — 32 concurrent `apply()` callers on the same instance.
//!
//! Because all 32 callers share the same `instance_mutex` and `instance_id`,
//! at most one apply can be in-flight at any moment.  The test verifies this
//! by sampling a shared counter that is atomically incremented on entry and
//! decremented on exit; the counter must never exceed 1.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::disallowed_methods
)]
// reason: integration test — panics and unwrap are the correct failure mode here

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

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
    reconciler::{Applier, DefaultCaddyJsonRenderer},
    schema::SchemaRegistry,
    storage::{
        trait_def::Storage,
        types::{Snapshot, SnapshotId},
    },
};

// ── Shared in-flight counter ──────────────────────────────────────────────────

/// Tracks how many applies are in-flight right now.  Incremented before the
/// apply body runs and decremented on completion.
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
/// Highest value ever seen in `IN_FLIGHT` during the test run.
static MAX_IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

// ── Fake Caddy client that probes in-flight concurrency ───────────────────────

struct CountingCaddyClient;

#[async_trait]
impl CaddyClient for CountingCaddyClient {
    async fn load_config(&self, _: CaddyConfig) -> Result<(), CaddyError> {
        let current = IN_FLIGHT.fetch_add(1, Ordering::SeqCst) + 1;
        // Track the max.
        let mut prev = MAX_IN_FLIGHT.load(Ordering::SeqCst);
        loop {
            if current <= prev {
                break;
            }
            match MAX_IN_FLIGHT.compare_exchange(prev, current, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => break,
                Err(actual) => prev = actual,
            }
        }
        // Yield to allow other tasks to run.
        tokio::task::yield_now().await;
        IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
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

/// Build an applier where all 32 callers share the same `instance_mutex` and
/// the same `lock_pool`, simulating 32 goroutine-equivalent tasks within one
/// process that all try to apply the same instance concurrently.
fn build_applier(
    storage: Arc<dyn Storage>,
    lock_pool: SqlitePool,
    instance_mutex: Arc<tokio::sync::Mutex<()>>,
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
        client: Arc::new(CountingCaddyClient),
        renderer: Arc::new(DefaultCaddyJsonRenderer),
        diff_engine: Arc::new(NoOpDiffEngine),
        capabilities: Arc::new(CapabilityCache::default()),
        audit,
        storage,
        instance_id: "local".to_owned(),
        clock: Arc::new(FixedClock(1_700_000_000_000)),
        instance_mutex,
        lock_pool,
        tls_observer: None,
    }
}

// ── Test ──────────────────────────────────────────────────────────────────────

/// 32 concurrent `apply()` calls share one `instance_mutex`; the in-flight
/// counter must never exceed 1 at any sampled point.
#[tokio::test]
async fn at_most_one_apply_in_flight_under_32_concurrent_callers() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let pool = store.pool().clone();
    let storage: Arc<dyn Storage> = Arc::new(store);

    // All 32 versions pre-inserted.
    for v in 1..=32_i64 {
        storage
            .insert_snapshot(make_snapshot(v))
            .await
            .expect("insert snapshot");
    }

    let shared_mutex: Arc<tokio::sync::Mutex<()>> = Arc::new(tokio::sync::Mutex::new(()));

    // Reset global counters (tests may run in-process sequentially).
    IN_FLIGHT.store(0, Ordering::SeqCst);
    MAX_IN_FLIGHT.store(0, Ordering::SeqCst);

    // Launch 32 concurrent tasks; each uses a fresh snapshot but shares the
    // same instance_mutex and lock_pool.
    let handles: Vec<_> = (1_i64..=32)
        .map(|v| {
            let storage_c = storage.clone();
            let pool_c = pool.clone();
            let mutex_c = shared_mutex.clone();
            tokio::spawn(async move {
                let applier = build_applier(storage_c.clone(), pool_c, mutex_c);
                let snap = make_snapshot(v);
                applier.apply(&snap, v - 1).await
            })
        })
        .collect();

    // Wait for all tasks.
    for handle in handles {
        let _ = handle.await.expect("task did not panic");
    }

    let max = MAX_IN_FLIGHT.load(Ordering::SeqCst);
    assert_eq!(
        max, 1,
        "at most 1 apply must be in-flight at any instant; observed max = {max}"
    );
}
