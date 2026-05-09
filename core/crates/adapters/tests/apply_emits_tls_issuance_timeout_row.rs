//! Slice 7.8 — observer emits a `config.apply-failed` row on TLS timeout.
//!
//! The fake `CaddyClient` always returns an empty cert list.  The observer is
//! configured with a short 1-second timeout.  After advancing time past the
//! timeout, one `config.apply-failed` row with
//! `error_kind = "TlsIssuanceTimeout"` must appear in the audit log.

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
    storage::{
        trait_def::Storage,
        types::{AuditOutcome, AuditSelector},
    },
};
use ulid::Ulid;

// ── Fakes ──────────────────────────────────────────────────────────────────────

/// Always returns an empty certificate list.
struct NoCertClient;

#[async_trait]
impl CaddyClient for NoCertClient {
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

// ── Test ───────────────────────────────────────────────────────────────────────

/// Observer emits a `config.apply-failed` row with `error_kind =
/// "TlsIssuanceTimeout"` when certs never appear within the timeout window.
///
/// Opens the `SQLite` store before pausing time so that connection setup is not
/// disrupted.  Then pauses time and advances it past the timeout.
#[tokio::test]
async fn apply_emits_tls_issuance_timeout_row() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    // Pause time after the store is open so that the observer's sleep does
    // not consume real wall-clock time during the test.
    tokio::time::pause();
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
        client: Arc::new(NoCertClient),
        audit,
        // Short timeout so the test completes quickly under paused time.
        timeout: Duration::from_secs(1),
    };

    let correlation_id = Ulid::new();

    // Spawn the observer so time can be advanced concurrently.
    // Pass snapshot_id = None to avoid FK constraint on the snapshots table.
    let handle = tokio::spawn({
        let observer = observer;
        let cid = correlation_id;
        async move {
            observer
                .observe(cid, vec!["example.com".to_owned()], None)
                .await;
        }
    });

    // Advance time past the timeout so the observer exits its polling loop.
    tokio::time::advance(Duration::from_secs(10)).await;

    // Let the spawned task run to completion.
    handle.await.expect("observer task must not panic");

    // Verify a config.apply-failed row was written.
    let failed_rows = storage
        .tail_audit_log(
            AuditSelector {
                kind_glob: Some("config.apply-failed".to_owned()),
                ..Default::default()
            },
            10,
        )
        .await
        .expect("tail_audit_log");

    assert_eq!(
        failed_rows.len(),
        1,
        "exactly one config.apply-failed row must be written on timeout"
    );
    assert_eq!(
        failed_rows[0].outcome,
        AuditOutcome::Error,
        "row must carry Error outcome"
    );
    assert_eq!(
        failed_rows[0].error_kind.as_deref(),
        Some("TlsIssuanceTimeout"),
        "error_kind must be TlsIssuanceTimeout"
    );

    // Also verify the notes carry the error kind.
    let notes_str = failed_rows[0].notes.as_deref().unwrap_or("{}");
    let notes: serde_json::Value =
        serde_json::from_str(notes_str).expect("notes must be valid JSON");
    assert_eq!(
        notes.get("error_kind").and_then(|v| v.as_str()),
        Some("TlsIssuanceTimeout"),
        "notes.error_kind must be 'TlsIssuanceTimeout'; got: {notes_str}"
    );
}
