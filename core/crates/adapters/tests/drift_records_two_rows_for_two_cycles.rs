//! Slice 8.6 — induce drift, resolve, induce different drift;
//! assert two `config.drift-detected` rows separated by one `config.drift-resolved`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::disallowed_methods
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use async_trait::async_trait;
use tempfile::TempDir;
use trilithon_adapters::{
    audit_writer::AuditWriter,
    drift::{DriftDetector, DriftDetectorConfig, ResolutionKind, TickOutcome},
    migrate::apply_migrations,
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
    model::desired_state::DesiredState,
    reconciler::DefaultCaddyJsonRenderer,
    schema::SchemaRegistry,
    storage::{
        trait_def::Storage,
        types::{AuditSelector, Snapshot, SnapshotId},
    },
};

// ── Fakes ────────────────────────────────────────────────────────────────────

/// Returns a `DesiredState` with a version that increments to simulate different drifts.
struct MutatingCaddyClient {
    version: AtomicU32,
}

#[async_trait]
impl CaddyClient for MutatingCaddyClient {
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
        // Use the counter value in the Caddy JSON so each tick produces a distinct hash.
        let ver = self.version.load(Ordering::Relaxed);
        Ok(CaddyConfig(serde_json::json!({"__drift_marker": ver})))
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
    let store = SqliteStorage::open(dir.path()).await.unwrap();
    apply_migrations(store.pool()).await.unwrap();
    store
}

async fn stored_empty_snapshot(storage: &Arc<dyn Storage>) -> Snapshot {
    let state = DesiredState::empty();
    let json = to_canonical_bytes(&state)
        .map(|b| String::from_utf8(b).unwrap())
        .unwrap();
    let snapshot_id = SnapshotId(content_address_bytes(json.as_bytes()));
    let snapshot = Snapshot {
        snapshot_id,
        parent_id: None,
        config_version: 1,
        actor: "test".to_owned(),
        intent: "test".to_owned(),
        correlation_id: "01HCORRELATION0000000000AB".to_owned(),
        caddy_version: "2.8.0".to_owned(),
        trilithon_version: "0.1.0".to_owned(),
        created_at_unix_seconds: 1_700_000_000,
        created_at_monotonic_nanos: 1_700_000_000_u64 * 1_000_000_000,
        canonical_json_version: CANONICAL_JSON_VERSION,
        desired_state_json: json,
    };
    storage.insert_snapshot(snapshot.clone()).await.unwrap();
    snapshot
}

// ── Test ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn drift_records_two_rows_for_two_cycles() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let storage: Arc<dyn Storage> = Arc::new(store);
    let _snap = stored_empty_snapshot(&storage).await;

    let registry = Box::leak(Box::new(SchemaRegistry::with_tier1_secrets()));
    let hasher = Box::leak(Box::new(ZeroHasher));
    let redactor = SecretsRedactor::new(registry, hasher);
    let clock: Arc<dyn Clock> = Arc::new(FixedClock(1_700_000_000_000));
    let audit = Arc::new(AuditWriter::new(storage.clone(), clock.clone(), redactor));

    let caddy_client = Arc::new(MutatingCaddyClient {
        version: AtomicU32::new(999),
    });

    let detector = Arc::new(DriftDetector {
        config: DriftDetectorConfig::default(),
        client: caddy_client.clone(),
        renderer: Arc::new(DefaultCaddyJsonRenderer),
        storage: storage.clone(),
        audit,
        clock,
        apply_mutex: Arc::new(tokio::sync::Mutex::new(())),
        last_running_hash: tokio::sync::Mutex::new(None),
    });

    // First drift cycle.
    let outcome = detector.tick_once().await.unwrap();
    if let TickOutcome::Drifted { event } = outcome {
        let cid = event.correlation_id;
        detector.record(event).await.unwrap();
        // Resolve it.
        detector
            .mark_resolved(cid, ResolutionKind::Reapply)
            .await
            .unwrap();
    } else {
        panic!("expected Drifted, got {outcome:?}");
    }

    // Change the running config to create a different drift.
    caddy_client.version.store(1000, Ordering::Relaxed);

    // Second drift cycle.
    let outcome = detector.tick_once().await.unwrap();
    if let TickOutcome::Drifted { event } = outcome {
        detector.record(event).await.unwrap();
    } else {
        panic!("expected Drifted, got {outcome:?}");
    }

    // Assert two drift-detected rows.
    let detected = storage
        .tail_audit_log(
            AuditSelector {
                kind_glob: Some("config.drift-detected".to_owned()),
                ..Default::default()
            },
            100,
        )
        .await
        .unwrap();
    assert_eq!(
        detected.len(),
        2,
        "expected 2 drift-detected rows, got {}",
        detected.len()
    );

    // Assert one drift-resolved row between them.
    let resolved = storage
        .tail_audit_log(
            AuditSelector {
                kind_glob: Some("config.drift-resolved".to_owned()),
                ..Default::default()
            },
            100,
        )
        .await
        .unwrap();
    assert_eq!(
        resolved.len(),
        1,
        "expected 1 drift-resolved row, got {}",
        resolved.len()
    );
}
