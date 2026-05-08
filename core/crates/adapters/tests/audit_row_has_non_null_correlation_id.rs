//! Verify that every audit row written through `AuditWriter::record` under a
//! correlation span carries a non-null `correlation_id`.
//!
//! Performs N=50 writes, each inside a `with_correlation_span` wrapper.
//! After all writes, queries all rows and asserts none has an empty
//! `correlation_id`.

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
    with_correlation_span,
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

// ── Helpers ───────────────────────────────────────────────────────────────────

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

fn make_writer(store: Arc<dyn Storage>) -> AuditWriter {
    let clock = Arc::new(FixedClock(1_700_000_000_000));
    let registry = Box::leak(Box::new(SchemaRegistry::with_tier1_secrets()));
    let hasher = Box::leak(Box::new(ZeroHasher));
    let redactor = trilithon_core::audit::redactor::SecretsRedactor::new(registry, hasher);
    AuditWriter::new(store, clock, redactor)
}

// ── Test ──────────────────────────────────────────────────────────────────────

const N: u32 = 50;

#[tokio::test]
async fn all_rows_have_non_null_correlation_id() {
    let dir = TempDir::new().unwrap();
    let store: Arc<dyn Storage> = Arc::new(open(&dir).await);
    let writer = Arc::new(make_writer(Arc::clone(&store)));

    for _ in 0..N {
        let cid = Ulid::new();
        let w = Arc::clone(&writer);
        with_correlation_span(cid, "test", "audit-row-test", async move {
            let append = AuditAppend {
                correlation_id: cid,
                actor: ActorRef::System {
                    component: "test-component".to_owned(),
                },
                event: AuditEvent::MutationApplied,
                target_kind: None,
                target_id: None,
                snapshot_id: None,
                diff: None,
                outcome: AuditOutcome::Ok,
                error_kind: None,
                notes: None,
            };
            w.record(append).await.expect("record must succeed");
        })
        .await;
    }

    let rows = store
        .tail_audit_log(AuditSelector::default(), N + 10)
        .await
        .expect("tail_audit_log must succeed");

    assert_eq!(
        rows.len(),
        usize::try_from(N).expect("N fits in usize"),
        "must have exactly {N} rows, found {}",
        rows.len()
    );

    for row in &rows {
        assert!(
            !row.correlation_id.is_empty(),
            "row {} must have a non-empty correlation_id",
            row.id.0
        );
    }
}
