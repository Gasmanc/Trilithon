//! `AuditWriter::record` — happy-path test with a secret-containing diff.
//!
//! Records a `MutationApplied` event whose diff includes a `password` field.
//! Asserts that:
//! - the row is persisted (storage returns an id),
//! - `redacted_diff_json` contains `***` (the redaction marker),
//! - `redaction_sites > 0`,
//! - the plaintext secret does not appear in the stored JSON.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use std::sync::Arc;

use serde_json::json;
use tempfile::TempDir;
use trilithon_adapters::{
    AuditWriter,
    audit_writer::{ActorRef, AuditAppend},
    migrate::apply_migrations,
    sqlite_storage::SqliteStorage,
};
use trilithon_core::{
    audit::AuditEvent,
    clock::Clock,
    schema::SchemaRegistry,
    storage::{
        trait_def::Storage,
        types::{AuditOutcome, AuditSelector},
    },
};
use ulid::Ulid;

// ── Test clock ────────────────────────────────────────────────────────────────

struct FixedClock(i64);

impl Clock for FixedClock {
    fn now_unix_ms(&self) -> i64 {
        self.0
    }
}

// ── Hasher that returns twelve zeros ─────────────────────────────────────────

struct ZeroHasher;

impl trilithon_core::audit::redactor::CiphertextHasher for ZeroHasher {
    fn hash_for_value(&self, _: &str) -> String {
        "000000000000".to_owned()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn open(dir: &TempDir) -> SqliteStorage {
    let store = SqliteStorage::open(dir.path())
        .await
        .expect("SqliteStorage::open should succeed");
    apply_migrations(store.pool())
        .await
        .expect("apply_migrations should succeed");
    store
}

#[tokio::test]
async fn happy_path_diff_with_secret_is_redacted() {
    let dir = TempDir::new().unwrap();
    let store = Arc::new(open(&dir).await);
    let clock = Arc::new(FixedClock(1_700_000_000_000));

    let registry = Box::leak(Box::new(SchemaRegistry::with_tier1_secrets()));
    let hasher = Box::leak(Box::new(ZeroHasher));
    let redactor = trilithon_core::audit::redactor::SecretsRedactor::new(registry, hasher);

    let writer = AuditWriter::new(store.clone(), clock, redactor);

    let plaintext = "hunter2";
    let diff = json!({
        "auth": {
            "basic": {
                "users": [{"username": "alice", "password": plaintext}]
            }
        }
    });

    let append = AuditAppend {
        correlation_id: Ulid::new(),
        actor: ActorRef::System {
            component: "test".to_owned(),
        },
        event: AuditEvent::MutationApplied,
        target_kind: Some("route".to_owned()),
        target_id: Some("route-01".to_owned()),
        snapshot_id: None,
        diff: Some(diff),
        outcome: AuditOutcome::Ok,
        error_kind: None,
        notes: None,
    };

    let id = writer.record(append).await.expect("record must succeed");
    assert!(!id.0.is_empty(), "returned id must be non-empty");

    // Fetch all rows and inspect.
    let rows = store
        .tail_audit_log(AuditSelector::default(), 10)
        .await
        .expect("tail_audit_log must succeed");

    assert_eq!(rows.len(), 1, "exactly one row must exist");
    let row = &rows[0];

    // redacted_diff_json must be Some and contain the marker.
    let rdj = row
        .redacted_diff_json
        .as_deref()
        .expect("redacted_diff_json must be Some");
    assert!(
        rdj.contains("***"),
        "redacted_diff_json must contain '***': {rdj}"
    );

    // redaction_sites must be > 0.
    assert!(
        row.redaction_sites > 0,
        "redaction_sites must be > 0, got {}",
        row.redaction_sites
    );

    // The plaintext must not appear anywhere in the stored diff.
    assert!(
        !rdj.contains(plaintext),
        "plaintext must not survive in redacted_diff_json: {rdj}"
    );
}
