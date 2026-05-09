//! Slice 7.5 — two concurrent `apply()` calls with identical `expected_version`.
//!
//! Both actors call `apply(&snapshot_v6, expected_version = 5)` concurrently
//! against a DB that is at `config_version = 5`.  The `BEGIN IMMEDIATE`
//! transaction in `cas_advance_config_version` serialises the two reads; the
//! second caller sees `observed = 6` (already advanced by the first) and must
//! receive `ApplyOutcome::Conflicted`.
//!
//! Assertions:
//! - Exactly one `Succeeded` outcome among the two calls.
//! - Exactly one `Conflicted` outcome among the two calls.
//! - Exactly one `mutation.conflicted` audit row.

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
    let body = format!("{{\"_v\":{config_version}}}");
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

/// Two concurrent `apply()` calls with the same `expected_version` produce exactly
/// one Succeeded and one Conflicted.
#[tokio::test]
async fn two_concurrent_applies_one_wins() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let pool = store.pool().clone();
    let storage: Arc<dyn Storage> = Arc::new(store);

    // Prime the DB: versions 1 through 5 (current = 5).
    for v in 1..=5_i64 {
        storage
            .insert_snapshot(make_snapshot(v))
            .await
            .expect("insert snapshot");
    }

    // Version 6 is the snapshot both actors want to apply.
    let snapshot_v6 = make_snapshot(6);
    storage
        .insert_snapshot(snapshot_v6.clone())
        .await
        .expect("insert v6");

    // Build two independent appliers sharing the same storage and pool but
    // with separate in-process mutexes (simulating two independent actor
    // threads that share a DB but have no shared lock state).
    let applier_a = Arc::new(build_applier(storage.clone(), pool.clone()));
    let applier_b = Arc::new(build_applier(storage.clone(), pool));

    let snap_a = snapshot_v6.clone();
    let snap_b = snapshot_v6.clone();

    // Launch both tasks concurrently.
    let (res_a, res_b) = tokio::join!(
        async move { applier_a.apply(&snap_a, 5).await },
        async move { applier_b.apply(&snap_b, 5).await },
    );

    let outcome_a = res_a.expect("actor A must return Ok");
    let outcome_b = res_b.expect("actor B must return Ok");

    let succeeded = [&outcome_a, &outcome_b]
        .iter()
        .filter(|o| matches!(o, ApplyOutcome::Succeeded { .. }))
        .count();
    let conflicted = [&outcome_a, &outcome_b]
        .iter()
        .filter(|o| matches!(o, ApplyOutcome::Conflicted { .. }))
        .count();

    assert_eq!(
        succeeded, 1,
        "exactly one actor must succeed; got outcomes: {outcome_a:?}, {outcome_b:?}"
    );
    assert_eq!(
        conflicted, 1,
        "exactly one actor must be conflicted; got outcomes: {outcome_a:?}, {outcome_b:?}"
    );

    // Exactly one mutation.conflicted audit row.
    let conflict_rows = storage
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
        conflict_rows.len(),
        1,
        "exactly one mutation.conflicted audit row must exist"
    );
}
