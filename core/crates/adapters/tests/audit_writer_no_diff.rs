//! `AuditWriter::record` — no-diff path.
//!
//! Records an `AuthLoginSucceeded` event with `diff = None`. Asserts:
//! - the row is persisted,
//! - `redacted_diff_json` IS NULL (`None`),
//! - `redaction_sites = 0`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use std::sync::Arc;

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
async fn no_diff_produces_null_redacted_diff_and_zero_sites() {
    let dir = TempDir::new().unwrap();
    let store = Arc::new(open(&dir).await);
    let clock = Arc::new(FixedClock(1_700_000_000_000));

    let registry = Box::leak(Box::new(SchemaRegistry::with_tier1_secrets()));
    let hasher = Box::leak(Box::new(ZeroHasher));
    let redactor = trilithon_core::audit::redactor::SecretsRedactor::new(registry, hasher);

    let writer = AuditWriter::new(store.clone(), clock, redactor);

    let append = AuditAppend {
        correlation_id: Ulid::new(),
        actor: ActorRef::User {
            id: "user-01".to_owned(),
        },
        event: AuditEvent::AuthLoginSucceeded,
        target_kind: None,
        target_id: None,
        snapshot_id: None,
        diff: None,
        outcome: AuditOutcome::Ok,
        error_kind: None,
        notes: None,
    };

    writer.record(append).await.expect("record must succeed");

    let rows = store
        .tail_audit_log(AuditSelector::default(), 10)
        .await
        .expect("tail_audit_log must succeed");

    assert_eq!(rows.len(), 1);
    let row = &rows[0];

    assert!(
        row.redacted_diff_json.is_none(),
        "redacted_diff_json must be None when diff is None"
    );
    assert_eq!(
        row.redaction_sites, 0,
        "redaction_sites must be 0 when diff is None"
    );
}
