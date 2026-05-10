//! Slice 8.5 — tick is skipped when the apply mutex is held.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::disallowed_methods
)]
// reason: integration test

use std::sync::Arc;

use async_trait::async_trait;
use tempfile::TempDir;
use trilithon_adapters::{
    audit_writer::AuditWriter,
    drift::{DriftDetector, DriftDetectorConfig, TickOutcome},
    migrate::apply_migrations,
    sqlite_storage::SqliteStorage,
};
use trilithon_core::{
    audit::redactor::SecretsRedactor,
    caddy::{
        CaddyClient, CaddyConfig, CaddyError, CaddyJsonPointer, HealthState, JsonPatch,
        LoadedModules, TlsCertificate, UpstreamHealth,
    },
    clock::Clock,
    diff::DefaultDiffEngine,
    schema::SchemaRegistry,
    storage::trait_def::Storage,
};

// ── Fakes ────────────────────────────────────────────────────────────────────

struct UnreachableCaddyClient;

#[async_trait]
impl CaddyClient for UnreachableCaddyClient {
    async fn load_config(&self, _: CaddyConfig) -> Result<(), CaddyError> {
        unimplemented!()
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
        panic!("should not be called when apply is in flight");
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

// ── Test ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn drift_skip_when_apply_in_flight() {
    let dir = TempDir::new().unwrap();
    let store = SqliteStorage::open(dir.path()).await.unwrap();
    apply_migrations(store.pool()).await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(store);

    let registry = Box::leak(Box::new(SchemaRegistry::with_tier1_secrets()));
    let hasher = Box::leak(Box::new(ZeroHasher));
    let redactor = SecretsRedactor::new(registry, hasher);
    let audit = Arc::new(AuditWriter::new(
        storage.clone(),
        Arc::new(FixedClock(1_700_000_000_000)),
        redactor,
    ));

    let apply_mutex = Arc::new(tokio::sync::Mutex::new(()));

    let detector = Arc::new(DriftDetector {
        config: DriftDetectorConfig::default(),
        client: Arc::new(UnreachableCaddyClient),
        diff_engine: Arc::new(DefaultDiffEngine),
        storage,
        audit,
        apply_mutex: Arc::clone(&apply_mutex),
    });

    // Hold the mutex to simulate an in-flight apply.
    let _guard = apply_mutex.lock().await;

    let outcome = detector.tick_once().await.expect("tick should succeed");
    assert_eq!(outcome, TickOutcome::SkippedApplyInFlight);
}
