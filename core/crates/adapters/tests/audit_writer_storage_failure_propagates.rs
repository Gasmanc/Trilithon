//! `AuditWriter::record` — storage failure propagation test.
//!
//! Injects a `Storage` double that always returns `StorageError::SqliteBusy`
//! from `record_audit_event`. Asserts that `AuditWriter::record` surfaces an
//! `AuditWriteError::Storage(_)` error.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use std::sync::Arc;

use async_trait::async_trait;
use trilithon_adapters::{
    AuditWriter,
    audit_writer::{ActorRef, AuditAppend, AuditWriteError},
};
use trilithon_core::{
    audit::AuditEvent,
    clock::Clock,
    schema::SchemaRegistry,
    storage::{
        Storage, StorageError,
        types::{
            AuditEventRow, AuditOutcome, AuditRowId, AuditSelector, DriftEventRow, DriftRowId,
            ParentChain, ProposalId, ProposalRow, Snapshot, SnapshotId, UnixSeconds,
        },
    },
};
use ulid::Ulid;

// ── Busy storage double ───────────────────────────────────────────────────────

/// A `Storage` double whose `record_audit_event` always returns
/// `StorageError::SqliteBusy`. All other methods panic — they are not needed
/// by this test.
struct BusyStorage;

#[async_trait]
impl Storage for BusyStorage {
    async fn insert_snapshot(&self, _: Snapshot) -> Result<SnapshotId, StorageError> {
        panic!("not needed in this test")
    }

    async fn get_snapshot(&self, _: &SnapshotId) -> Result<Option<Snapshot>, StorageError> {
        panic!("not needed in this test")
    }

    async fn parent_chain(&self, _: &SnapshotId, _: usize) -> Result<ParentChain, StorageError> {
        panic!("not needed in this test")
    }

    async fn latest_desired_state(&self) -> Result<Option<Snapshot>, StorageError> {
        panic!("not needed in this test")
    }

    async fn record_audit_event(&self, _: AuditEventRow) -> Result<AuditRowId, StorageError> {
        Err(StorageError::SqliteBusy)
    }

    async fn tail_audit_log(
        &self,
        _: AuditSelector,
        _: u32,
    ) -> Result<Vec<AuditEventRow>, StorageError> {
        panic!("not needed in this test")
    }

    async fn record_drift_event(&self, _: DriftEventRow) -> Result<DriftRowId, StorageError> {
        panic!("not needed in this test")
    }

    async fn latest_drift_event(&self) -> Result<Option<DriftEventRow>, StorageError> {
        panic!("not needed in this test")
    }

    async fn latest_unresolved_drift_event(&self) -> Result<Option<DriftEventRow>, StorageError> {
        panic!("not needed in this test")
    }

    async fn resolve_drift_event(
        &self,
        _: &str,
        _: trilithon_core::storage::types::DriftResolution,
        _: UnixSeconds,
    ) -> Result<(), StorageError> {
        panic!("not needed in this test")
    }

    async fn enqueue_proposal(&self, _: ProposalRow) -> Result<ProposalId, StorageError> {
        panic!("not needed in this test")
    }

    async fn dequeue_proposal(&self) -> Result<Option<ProposalRow>, StorageError> {
        panic!("not needed in this test")
    }

    async fn expire_proposals(&self, _: UnixSeconds) -> Result<u32, StorageError> {
        panic!("not needed in this test")
    }

    async fn current_config_version(&self, _: &str) -> Result<i64, StorageError> {
        panic!("not needed in this test")
    }

    async fn cas_advance_config_version(
        &self,
        _: &str,
        _: i64,
        _: &SnapshotId,
    ) -> Result<i64, StorageError> {
        panic!("not needed in this test")
    }
}

// ── Test clock ────────────────────────────────────────────────────────────────

struct FixedClock(i64);

impl Clock for FixedClock {
    fn now_unix_ms(&self) -> i64 {
        self.0
    }
}

// ── Hasher ────────────────────────────────────────────────────────────────────

struct ZeroHasher;

impl trilithon_core::audit::redactor::CiphertextHasher for ZeroHasher {
    fn hash_for_value(&self, _: &str) -> String {
        "000000000000".to_owned()
    }
}

// ── Test ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn storage_busy_surfaces_as_audit_write_error_storage() {
    let store = Arc::new(BusyStorage);
    let clock = Arc::new(FixedClock(1_700_000_000_000));

    let registry = Box::leak(Box::new(SchemaRegistry::with_tier1_secrets()));
    let hasher = Box::leak(Box::new(ZeroHasher));
    let redactor = trilithon_core::audit::redactor::SecretsRedactor::new(registry, hasher);

    let writer = AuditWriter::new(store, clock, redactor);

    let append = AuditAppend {
        correlation_id: Ulid::new(),
        actor: ActorRef::System {
            component: "test".to_owned(),
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

    let err = writer.record(append).await.expect_err("must fail");

    assert!(
        matches!(err, AuditWriteError::Storage(_)),
        "expected AuditWriteError::Storage, got: {err:?}"
    );
}
