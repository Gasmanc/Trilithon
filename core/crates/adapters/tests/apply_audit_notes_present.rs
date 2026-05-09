//! Slice 7.7 — audit notes present on successful apply.
//!
//! Asserts that a successful apply writes a `config.applied` row whose `notes`
//! column parses to a well-formed [`ApplyAuditNotes`] with the expected fields:
//! - `reload_kind = Graceful { drain_window_ms: None }`
//! - `applied_state = Applied`
//! - `error_kind = None`

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::disallowed_methods
)]
// reason: integration test — panics, unimplemented, and unwrap are the correct failure mode here

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
    reconciler::{AppliedStateTag, Applier, ApplyAuditNotes, DefaultCaddyJsonRenderer, ReloadKind},
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

// ── Tests ─────────────────────────────────────────────────────────────────────

/// A successful apply must write a `config.applied` row whose `notes` field
/// deserialises to `ApplyAuditNotes` with the expected values.
#[tokio::test]
async fn audit_notes_present_on_successful_apply() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let pool = store.pool().clone();
    let storage: Arc<dyn Storage> = Arc::new(store);
    let applier = build_applier(storage.clone(), pool);
    let snapshot = stored_snapshot(&storage, 1).await;

    applier
        .apply(&snapshot, 0)
        .await
        .expect("apply must succeed");

    let rows = storage
        .tail_audit_log(
            AuditSelector {
                kind_glob: Some("config.applied".to_owned()),
                ..Default::default()
            },
            10,
        )
        .await
        .expect("tail_audit_log");

    assert_eq!(rows.len(), 1, "exactly one config.applied row");

    let notes_str = rows[0]
        .notes
        .as_deref()
        .expect("notes must be Some on config.applied");

    let notes: ApplyAuditNotes =
        serde_json::from_str(notes_str).expect("notes must parse as ApplyAuditNotes");

    assert_eq!(
        notes.reload_kind,
        ReloadKind::Graceful {
            drain_window_ms: None
        },
        "reload_kind must be Graceful with no drain window"
    );
    assert_eq!(
        notes.applied_state,
        AppliedStateTag::Applied,
        "applied_state must be Applied"
    );
    assert!(
        notes.error_kind.is_none(),
        "error_kind must be None on success"
    );
    assert!(
        notes.error_detail.is_none(),
        "error_detail must be None on success"
    );
    assert!(
        notes.caddy_status.is_none(),
        "caddy_status must be None on success"
    );
}
