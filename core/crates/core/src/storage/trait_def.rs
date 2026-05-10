//! The `Storage` trait — persistent store boundary.

use async_trait::async_trait;

use crate::storage::{
    error::StorageError,
    types::{
        AuditEventRow, AuditRowId, AuditSelector, DriftEventRow, DriftRowId, ParentChain,
        ProposalId, ProposalRow, Snapshot, SnapshotId, UnixSeconds,
    },
};

/// The persistent store boundary for Trilithon.
///
/// All implementations must be [`Send`] + [`Sync`] + `'static` so that they
/// can be stored behind `Arc<dyn Storage>` for the daemon's lifetime.
///
/// Every write method records exactly one row; transactional grouping happens
/// through dedicated `with_transaction` helpers on the concrete adapter.
#[async_trait]
pub trait Storage: Send + Sync + 'static {
    /// Insert a new immutable snapshot.
    ///
    /// Returns the inserted `SnapshotId`.  Rejects the row if `snapshot.id`
    /// already exists or the content hash does not match the canonical-JSON
    /// SHA-256.
    async fn insert_snapshot(&self, snapshot: Snapshot) -> Result<SnapshotId, StorageError>;

    /// Fetch a snapshot by id.
    ///
    /// Returns `None` only when the id is unknown.  Never returns a partial
    /// row; integrity checks fail fast.
    async fn get_snapshot(&self, id: &SnapshotId) -> Result<Option<Snapshot>, StorageError>;

    /// Walk the parent chain of a snapshot, oldest first.
    ///
    /// Terminates at the genesis snapshot or at a missing parent pointer,
    /// returning the chain seen so far and a `truncated` flag.
    async fn parent_chain(
        &self,
        leaf: &SnapshotId,
        max_depth: usize,
    ) -> Result<ParentChain, StorageError>;

    /// Return the latest desired-state snapshot.
    ///
    /// Returns `None` only on first run, before bootstrap.
    async fn latest_desired_state(&self) -> Result<Option<Snapshot>, StorageError>;

    /// Append a single audit event row.
    ///
    /// The `kind` field MUST be in the architecture §6.6 vocabulary; rejected
    /// with [`StorageError::AuditKindUnknown`] otherwise.
    async fn record_audit_event(&self, event: AuditEventRow) -> Result<AuditRowId, StorageError>;

    /// Return audit rows in reverse chronological order, filtered by `selector`.
    ///
    /// Used by the audit viewer and forensic queries.
    async fn tail_audit_log(
        &self,
        selector: AuditSelector,
        limit: u32,
    ) -> Result<Vec<AuditEventRow>, StorageError>;

    /// Append a drift detection row.
    async fn record_drift_event(&self, event: DriftEventRow) -> Result<DriftRowId, StorageError>;

    /// Return the latest drift event for the current desired-state snapshot.
    async fn latest_drift_event(&self) -> Result<Option<DriftEventRow>, StorageError>;

    /// Return the latest unresolved drift event for a given instance.
    ///
    /// Used at daemon startup to initialise the deduplication hash, preventing
    /// duplicate `config.drift-detected` rows across restarts.
    async fn latest_unresolved_drift_event(
        &self,
        instance_id: &str,
    ) -> Result<Option<DriftEventRow>, StorageError>;

    /// Mark a drift event as resolved.
    async fn resolve_drift_event(
        &self,
        correlation_id: &str,
        resolution: crate::storage::types::DriftResolution,
        resolved_at: UnixSeconds,
    ) -> Result<(), StorageError>;

    /// Insert a proposal into the queue.
    ///
    /// Returns [`StorageError::ProposalDuplicate`] if an open proposal with
    /// the same `(source, source_ref)` already exists.
    async fn enqueue_proposal(&self, proposal: ProposalRow) -> Result<ProposalId, StorageError>;

    /// Atomically claim and return the next pending proposal.
    ///
    /// Returns `None` if no proposal is currently pending.
    async fn dequeue_proposal(&self) -> Result<Option<ProposalRow>, StorageError>;

    /// Sweep proposals whose expiry has passed; transition them to `expired`.
    ///
    /// Returns the count of proposals that were expired.
    async fn expire_proposals(&self, now: UnixSeconds) -> Result<u32, StorageError>;

    /// Return the current `config_version` for `instance_id`.
    ///
    /// Returns `0` when no snapshot has been inserted yet (virgin database).
    ///
    /// This is a point-in-time read.  Callers that need a TOCTOU-safe
    /// read-check-write sequence should use [`Self::cas_advance_config_version`]
    /// instead.
    async fn current_config_version(&self, instance_id: &str) -> Result<i64, StorageError>;

    /// Compare-and-swap advance on `config_version`.
    ///
    /// Atomically:
    /// 1. Read `MAX(config_version)` for `instance_id`.
    /// 2. If the observed value does not equal `expected_version`, return
    ///    `Err(StorageError::OptimisticConflict { observed, expected })`.
    /// 3. Otherwise, verify that `new_snapshot_id` is recorded in storage with
    ///    `config_version = expected_version + 1`.
    /// 4. Return `Ok(expected_version + 1)`.
    ///
    /// The implementation is responsible for issuing `BEGIN IMMEDIATE` (or
    /// equivalent) to prevent TOCTOU races on the version read.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::OptimisticConflict`] on version mismatch.
    /// Returns [`StorageError::Integrity`] when `new_snapshot_id` is missing
    /// or has a mismatched `config_version`.
    async fn cas_advance_config_version(
        &self,
        instance_id: &str,
        expected_version: i64,
        new_snapshot_id: &SnapshotId,
    ) -> Result<i64, StorageError>;
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::diverging_sub_expression
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use super::Storage;

    /// Compile-time check: `Storage` is object-safe and its impls are `Send + Sync`.
    #[allow(unreachable_code)]
    fn _check() {
        let _: Box<dyn Storage> = panic!("compile-only");
    }

    /// The real check is the compile-time `_check()` above; this test
    /// function exists so the test runner has a named result to report.
    #[test]
    fn trait_is_pure() {}
}
