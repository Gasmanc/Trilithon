//! Slice 7.7 — exactly one terminal audit row per apply call.
//!
//! For each of 4 scenarios (happy path, 400, unreachable, conflict) asserts
//! that exactly one terminal row from the set
//! `{ config.applied, config.apply-failed, mutation.conflicted }` is written
//! to the audit log per `correlation_id` after the apply call.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::disallowed_methods,
    clippy::too_many_lines
)]
// reason: integration test — panics, unimplemented, and unwrap are the correct failure mode;
//         too_many_lines allowed because the conflict scenario must inline a local CaddyClient impl

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
    reconciler::{Applier, DefaultCaddyJsonRenderer},
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

struct BadStatusClient;

#[async_trait]
impl CaddyClient for BadStatusClient {
    async fn load_config(&self, _: CaddyConfig) -> Result<(), CaddyError> {
        Err(CaddyError::BadStatus {
            status: 400,
            body: "bad config".to_owned(),
        })
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

struct UnreachableClient;

#[async_trait]
impl CaddyClient for UnreachableClient {
    async fn load_config(&self, _: CaddyConfig) -> Result<(), CaddyError> {
        Err(CaddyError::Unreachable {
            detail: "unix socket not found".to_owned(),
        })
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

/// Client that panics if `load_config` is called — used in conflict tests where
/// the CAS check fires before any Caddy request is issued.
struct NeverCalledClient;

#[async_trait]
impl CaddyClient for NeverCalledClient {
    async fn load_config(&self, _: CaddyConfig) -> Result<(), CaddyError> {
        panic!("load_config must not be called on conflict")
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

// ── Setup helpers ──────────────────────────────────────────────────────────────

async fn open_store(dir: &TempDir) -> SqliteStorage {
    let store = SqliteStorage::open(dir.path())
        .await
        .expect("SqliteStorage::open");
    apply_migrations(store.pool())
        .await
        .expect("apply_migrations");
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

fn make_applier(
    client: Arc<dyn CaddyClient>,
    storage: Arc<dyn Storage>,
    lock_pool: SqlitePool,
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
        client,
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

/// Count terminal audit rows (config.applied | config.apply-failed |
/// mutation.conflicted) across all kinds in one query-per-kind then sum.
async fn count_terminal_rows(storage: &Arc<dyn Storage>) -> usize {
    let terminal_kinds = [
        "config.applied",
        "config.apply-failed",
        "mutation.conflicted",
    ];
    let mut total = 0usize;
    for kind in terminal_kinds {
        let rows = storage
            .tail_audit_log(
                AuditSelector {
                    kind_glob: Some(kind.to_owned()),
                    ..Default::default()
                },
                100,
            )
            .await
            .expect("tail_audit_log");
        total += rows.len();
    }
    total
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Happy path: exactly one `config.applied` terminal row.
#[tokio::test]
async fn exactly_one_terminal_row_happy_path() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let pool = store.pool().clone();
    let storage: Arc<dyn Storage> = Arc::new(store);
    let applier = make_applier(Arc::new(OkCaddyClient), storage.clone(), pool);
    let snapshot = stored_snapshot(&storage, 1).await;

    let _ = applier.apply(&snapshot, 0).await;

    let count = count_terminal_rows(&storage).await;
    assert_eq!(
        count, 1,
        "happy path: exactly one terminal row, got {count}"
    );
}

/// Caddy 400: exactly one `config.apply-failed` terminal row.
#[tokio::test]
async fn exactly_one_terminal_row_caddy_400() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let pool = store.pool().clone();
    let storage: Arc<dyn Storage> = Arc::new(store);
    let applier = make_applier(Arc::new(BadStatusClient), storage.clone(), pool);
    let snapshot = stored_snapshot(&storage, 1).await;

    let _ = applier.apply(&snapshot, 0).await;

    let count = count_terminal_rows(&storage).await;
    assert_eq!(count, 1, "caddy 400: exactly one terminal row, got {count}");
}

/// Caddy unreachable: the `caddy.unreachable` row is the terminal row for this
/// error path.  The test verifies exactly one *terminal-class* row is written.
/// Note: unreachable writes `caddy.unreachable`, not `config.apply-failed`, so
/// there must be zero config.applied/config.apply-failed/mutation.conflicted rows.
#[tokio::test]
async fn exactly_one_terminal_row_unreachable() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let pool = store.pool().clone();
    let storage: Arc<dyn Storage> = Arc::new(store);
    let applier = make_applier(Arc::new(UnreachableClient), storage.clone(), pool);
    let snapshot = stored_snapshot(&storage, 1).await;

    let _ = applier.apply(&snapshot, 0).await;

    // caddy.unreachable is NOT in the terminal set above (config.applied,
    // config.apply-failed, mutation.conflicted) — verify none of those are written.
    let terminal_count = count_terminal_rows(&storage).await;
    assert_eq!(
        terminal_count, 0,
        "unreachable: no config.applied/apply-failed/conflicted rows, got {terminal_count}"
    );

    // Exactly one caddy.unreachable row must exist.
    let unreachable_rows = storage
        .tail_audit_log(
            AuditSelector {
                kind_glob: Some("caddy.unreachable".to_owned()),
                ..Default::default()
            },
            10,
        )
        .await
        .expect("tail_audit_log");
    assert_eq!(
        unreachable_rows.len(),
        1,
        "unreachable: exactly one caddy.unreachable row"
    );
}

/// Optimistic conflict: exactly one `mutation.conflicted` terminal row.
#[tokio::test]
async fn exactly_one_terminal_row_conflict() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let pool = store.pool().clone();
    let storage: Arc<dyn Storage> = Arc::new(store);

    // Prime the DB with versions 1-5, mark v5 as applied.
    let body = "{\"_v\":1}";
    let id = SnapshotId(content_address_bytes(body.as_bytes()));
    let base_snap = Snapshot {
        snapshot_id: id,
        parent_id: None,
        config_version: 1,
        actor: "test".to_owned(),
        intent: "v1".to_owned(),
        correlation_id: "01HCORRELATION0000000000AB".to_owned(),
        caddy_version: "2.8.0".to_owned(),
        trilithon_version: "0.1.0".to_owned(),
        created_at_unix_seconds: 1_700_000_000,
        created_at_monotonic_nanos: 1_700_000_000_u64 * 1_000_000_000,
        canonical_json_version: CANONICAL_JSON_VERSION,
        desired_state_json: body.to_owned(),
    };
    storage
        .insert_snapshot(base_snap)
        .await
        .expect("insert base snapshot");

    sqlx::query("UPDATE caddy_instances SET applied_config_version = 1 WHERE id = 'local'")
        .execute(&pool)
        .await
        .expect("set applied_config_version");

    // Create a second snapshot at v2 and attempt apply with stale expected=0.
    let body2 = "{\"_v\":2}";
    let id2 = SnapshotId(content_address_bytes(body2.as_bytes()));
    let snap_v2 = Snapshot {
        snapshot_id: id2,
        parent_id: None,
        config_version: 2,
        actor: "test".to_owned(),
        intent: "v2".to_owned(),
        correlation_id: "01HCORRELATION0000000000CD".to_owned(),
        caddy_version: "2.8.0".to_owned(),
        trilithon_version: "0.1.0".to_owned(),
        created_at_unix_seconds: 1_700_000_001,
        created_at_monotonic_nanos: 1_700_000_001_u64 * 1_000_000_000,
        canonical_json_version: CANONICAL_JSON_VERSION,
        desired_state_json: body2.to_owned(),
    };
    storage
        .insert_snapshot(snap_v2.clone())
        .await
        .expect("insert v2");

    // Conflict is detected before any Caddy request — use NeverCalledClient.
    let applier = make_applier(Arc::new(NeverCalledClient), storage.clone(), pool);
    // Pass expected_version = 0 which is stale (actual is 1).
    let _ = applier.apply(&snap_v2, 0).await;

    let count = count_terminal_rows(&storage).await;
    assert_eq!(
        count, 1,
        "conflict: exactly one terminal row (mutation.conflicted), got {count}"
    );
}
