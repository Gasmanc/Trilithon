//! Slice 7.8 — observer emits a `config.applied` follow-up row when certs appear.
//!
//! The fake `CaddyClient` immediately returns a cert for `"example.com"`.
//! After calling `observe()`, one `config.applied` row with
//! `applied_state = "tls-issuing"` must appear in the audit log.

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
use tempfile::TempDir;
use trilithon_adapters::{
    TlsIssuanceObserver, audit_writer::AuditWriter, migrate::apply_migrations,
    sqlite_storage::SqliteStorage,
};
use trilithon_core::{
    audit::redactor::SecretsRedactor,
    caddy::{
        CaddyClient, CaddyConfig, CaddyError, CaddyJsonPointer, HealthState, JsonPatch,
        LoadedModules, TlsCertificate, UpstreamHealth,
    },
    clock::Clock,
    schema::SchemaRegistry,
    storage::{trait_def::Storage, types::AuditSelector},
};
use ulid::Ulid;

// ── Fakes ──────────────────────────────────────────────────────────────────────

/// Always returns a cert for `"example.com"` on the first call.
struct ImmediateCertClient;

#[async_trait]
impl CaddyClient for ImmediateCertClient {
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
        unimplemented!()
    }

    async fn get_loaded_modules(&self) -> Result<LoadedModules, CaddyError> {
        unimplemented!()
    }

    async fn get_upstream_health(&self) -> Result<Vec<UpstreamHealth>, CaddyError> {
        unimplemented!()
    }

    async fn get_certificates(&self) -> Result<Vec<TlsCertificate>, CaddyError> {
        Ok(vec![TlsCertificate {
            names: vec!["example.com".to_owned()],
            not_before: 1_700_000_000,
            not_after: 1_731_536_000,
            issuer: "Let's Encrypt".to_owned(),
        }])
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

// ── Test ───────────────────────────────────────────────────────────────────────

/// Observer emits a `config.applied` follow-up row with `applied_state =
/// "tls-issuing"` when all requested hostnames appear in `get_certificates`.
///
/// Does NOT require paused time because `ImmediateCertClient` returns certs on
/// the first poll so no sleep is reached.
#[tokio::test]
async fn apply_emits_tls_issuance_followup_row() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let storage: Arc<dyn Storage> = Arc::new(store);

    let registry = Box::leak(Box::new(SchemaRegistry::with_tier1_secrets()));
    let hasher = Box::leak(Box::new(ZeroHasher));
    let redactor = SecretsRedactor::new(registry, hasher);
    let audit = Arc::new(AuditWriter::new(
        storage.clone(),
        Arc::new(FixedClock(1_700_000_000_000)),
        redactor,
    ));

    let observer = TlsIssuanceObserver {
        client: Arc::new(ImmediateCertClient),
        audit,
        timeout: Duration::from_secs(120),
    };

    let correlation_id = Ulid::new();

    // The cert for "example.com" is immediately available; the observer should
    // emit a config.applied row on its first poll.
    // Pass snapshot_id = None to avoid FK constraint on the snapshots table.
    observer
        .observe(correlation_id, vec!["example.com".to_owned()], None)
        .await;

    // Verify exactly one follow-up config.applied row exists.
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

    assert_eq!(
        rows.len(),
        1,
        "exactly one config.applied follow-up row must be written"
    );

    // Verify the notes contain applied_state = "tls-issuing".
    let notes_str = rows[0].notes.as_deref().unwrap_or("{}");
    let notes: serde_json::Value =
        serde_json::from_str(notes_str).expect("notes must be valid JSON");
    assert_eq!(
        notes.get("applied_state").and_then(|v| v.as_str()),
        Some("tls-issuing"),
        "notes.applied_state must be 'tls-issuing'; got: {notes_str}"
    );
    assert!(
        notes
            .get("error_kind")
            .is_none_or(serde_json::Value::is_null),
        "notes.error_kind must be null/absent on success; got: {notes_str}"
    );
}
