//! Slice 7.5 — two concurrent `apply()` calls with identical `expected_version`.
//!
//! Both actors share one `instance_mutex` so they serialise at the Tokio-mutex
//! level.  Actor A acquires the mutex first, does the CAS advance (5 → 6), then
//! releases.  Actor B then acquires the mutex and attempts the same CAS; it sees
//! `observed = 6` (already advanced) with `expected = 5` and gets
//! `ApplyOutcome::Conflicted`.
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
    canonical_json::{CANONICAL_JSON_VERSION, content_address_bytes, to_canonical_bytes},
    clock::Clock,
    diff::NoOpDiffEngine,
    model::desired_state::DesiredState,
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

/// Build a history snapshot (not applied directly — stub JSON is fine).
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

/// Build an apply-ready snapshot with a valid `DesiredState` body.
fn make_apply_snapshot(config_version: i64) -> Snapshot {
    let state = DesiredState::empty();
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
        created_at_monotonic_nanos: (1_700_000_000_u64 + config_version as u64) * 1_000_000_000,
        canonical_json_version: CANONICAL_JSON_VERSION,
        desired_state_json: body,
    }
}

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
        client: Arc::new(OkCaddyClient),
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

/// Two concurrent `apply()` calls with the same `expected_version` produce exactly
/// one Succeeded and one Conflicted.
#[tokio::test]
async fn two_concurrent_applies_one_wins() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let pool = store.pool().clone();
    let storage: Arc<dyn Storage> = Arc::new(store);

    // Prime the DB: versions 1 through 5, mark v5 as applied.
    for v in 1..=5_i64 {
        storage
            .insert_snapshot(make_snapshot(v))
            .await
            .expect("insert snapshot");
    }

    // Mark version 5 as currently applied so CAS reads observed=5.
    sqlx::query("UPDATE caddy_instances SET applied_config_version = 5 WHERE id = 'local'")
        .execute(&pool)
        .await
        .expect("set applied_config_version");

    // Version 6 is the snapshot both actors want to apply — needs valid DesiredState.
    let snapshot_v6 = make_apply_snapshot(6);
    storage
        .insert_snapshot(snapshot_v6.clone())
        .await
        .expect("insert v6");

    // Both appliers share the same instance_mutex so they serialise at the
    // Tokio level.  The first to acquire the mutex wins the CAS; the second
    // sees the already-advanced version and returns Conflicted.
    let shared_mutex = Arc::new(tokio::sync::Mutex::new(()));
    let applier_a = Arc::new(build_applier(
        storage.clone(),
        pool.clone(),
        shared_mutex.clone(),
    ));
    let applier_b = Arc::new(build_applier(storage.clone(), pool, shared_mutex));

    let snap_a = snapshot_v6.clone();
    let snap_b = snapshot_v6;

    // The shared mutex serialises the two calls: A fully completes (including
    // advisory lock cleanup) before B runs.  B then attempts the same CAS and
    // gets Conflicted because the version was already advanced to 6.
    let outcome_a = applier_a
        .apply(&snap_a, 5)
        .await
        .expect("actor A must return Ok");
    let outcome_b = applier_b
        .apply(&snap_b, 5)
        .await
        .expect("actor B must return Ok");

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
