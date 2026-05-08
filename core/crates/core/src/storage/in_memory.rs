//! In-memory `Storage` test double.
//!
//! This module is compiled only in test builds (`#![cfg(test)]`).  It
//! satisfies the `Storage` trait using `std::sync::Mutex`-backed collections
//! so that `core` remains free of Tokio in production builds.
#![cfg(test)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::significant_drop_tightening
)]
// reason: test-only code; panics are the correct failure mode in tests.
// significant_drop_tightening: mutex guards intentionally span the whole
// function body to keep the double simple and prevent TOCTOU inside tests.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::storage::{
    audit_vocab::AUDIT_KINDS,
    error::StorageError,
    helpers::{audit_prev_hash_seed, canonical_json_for_audit_hash, compute_audit_chain_hash},
    trait_def::Storage,
    types::{
        AuditEventRow, AuditRowId, AuditSelector, DriftEventRow, DriftRowId, ParentChain,
        ProposalId, ProposalRow, ProposalState, Snapshot, SnapshotId, UnixSeconds,
    },
};

/// In-memory implementation of [`Storage`] for use in contract tests.
///
/// Thread-safe via `Mutex`; intentionally uses `std::sync` primitives to
/// keep `core` free of Tokio in production builds.  Every method is async
/// to satisfy the trait contract but never actually yields.
pub struct InMemoryStorage {
    snapshots: Mutex<HashMap<SnapshotId, Snapshot>>,
    audit: Mutex<Vec<AuditEventRow>>,
    drift: Mutex<Vec<DriftEventRow>>,
    proposals: Mutex<Vec<ProposalRow>>,
    latest_ptr: Mutex<Option<SnapshotId>>,
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryStorage {
    /// Create an empty `InMemoryStorage`.
    pub fn new() -> Self {
        Self {
            snapshots: Mutex::new(HashMap::new()),
            audit: Mutex::new(Vec::new()),
            drift: Mutex::new(Vec::new()),
            proposals: Mutex::new(Vec::new()),
            latest_ptr: Mutex::new(None),
        }
    }
}

#[async_trait]
impl Storage for InMemoryStorage {
    async fn insert_snapshot(&self, snapshot: Snapshot) -> Result<SnapshotId, StorageError> {
        let mut snapshots = self.snapshots.lock().expect("snapshots lock poisoned");
        let mut latest_ptr = self.latest_ptr.lock().expect("latest_ptr lock poisoned");

        if let Some(existing) = snapshots.get(&snapshot.snapshot_id) {
            if existing.desired_state_json == snapshot.desired_state_json {
                // Byte-equal body — idempotent duplicate; return the existing id.
                return Ok(snapshot.snapshot_id);
            }
            // Same hash, different body — SHA-256 collision; treat as fatal.
            return Err(StorageError::SnapshotHashCollision {
                id: snapshot.snapshot_id,
            });
        }

        // Update latest_ptr if this snapshot has a higher config_version.
        let should_update = latest_ptr.as_ref().is_none_or(|current_id| {
            let current_version = snapshots
                .get(current_id)
                .map_or(i64::MIN, |s| s.config_version);
            snapshot.config_version > current_version
        });

        let id = snapshot.snapshot_id.clone();
        snapshots.insert(id.clone(), snapshot);

        if should_update {
            *latest_ptr = Some(id.clone());
        }

        Ok(id)
    }

    async fn get_snapshot(&self, id: &SnapshotId) -> Result<Option<Snapshot>, StorageError> {
        let snapshots = self.snapshots.lock().expect("snapshots lock poisoned");
        Ok(snapshots.get(id).cloned())
    }

    async fn parent_chain(
        &self,
        leaf: &SnapshotId,
        max_depth: usize,
    ) -> Result<ParentChain, StorageError> {
        let snapshots = self.snapshots.lock().expect("snapshots lock poisoned");

        let mut chain: Vec<Snapshot> = Vec::new();
        let mut current_id = leaf.clone();
        let mut truncated = false;

        loop {
            if chain.len() >= max_depth {
                truncated = true;
                break;
            }

            let Some(snapshot) = snapshots.get(&current_id) else {
                break;
            };

            chain.push(snapshot.clone());

            if let Some(parent_id) = snapshot.parent_id.clone() {
                current_id = parent_id;
            } else {
                break;
            }
        }

        // Reverse so that oldest is first.
        chain.reverse();

        Ok(ParentChain {
            snapshots: chain,
            truncated,
        })
    }

    async fn latest_desired_state(&self) -> Result<Option<Snapshot>, StorageError> {
        // Acquire in the same order as insert_snapshot (snapshots → latest_ptr)
        // to prevent ABBA deadlock when both methods run concurrently.
        let snapshots = self.snapshots.lock().expect("snapshots lock poisoned");
        let latest_ptr = self.latest_ptr.lock().expect("latest_ptr lock poisoned");

        let result = latest_ptr
            .as_ref()
            .and_then(|id| snapshots.get(id))
            .cloned();
        Ok(result)
    }

    async fn record_audit_event(
        &self,
        mut event: AuditEventRow,
    ) -> Result<AuditRowId, StorageError> {
        if !AUDIT_KINDS.contains(&event.kind.as_str()) {
            return Err(StorageError::AuditKindUnknown { kind: event.kind });
        }

        let id = event.id.clone();
        let mut audit = self.audit.lock().expect("audit lock poisoned");

        // Compute prev_hash from the last row, or use the seed if empty.
        event.prev_hash = audit.last().map_or_else(
            || audit_prev_hash_seed().to_string(),
            |last| compute_audit_chain_hash(&canonical_json_for_audit_hash(last)),
        );

        audit.push(event);
        Ok(id)
    }

    async fn tail_audit_log(
        &self,
        selector: AuditSelector,
        limit: u32,
    ) -> Result<Vec<AuditEventRow>, StorageError> {
        let audit = self.audit.lock().expect("audit lock poisoned");

        let result: Vec<AuditEventRow> = audit
            .iter()
            .filter(|row| {
                if let Some(ref kind_glob) = selector.kind_glob {
                    match crate::storage::glob_prefix(kind_glob) {
                        Some(prefix) => {
                            if !row.kind.starts_with(prefix) {
                                return false;
                            }
                        }
                        None => {
                            if row.kind != *kind_glob {
                                return false;
                            }
                        }
                    }
                }
                if let Some(ref actor_id) = selector.actor_id {
                    if row.actor_id != *actor_id {
                        return false;
                    }
                }
                if let Some(ref correlation_id) = selector.correlation_id {
                    if row.correlation_id != *correlation_id {
                        return false;
                    }
                }
                if let Some(since) = selector.since {
                    if row.occurred_at < since {
                        return false;
                    }
                }
                if let Some(until) = selector.until {
                    if row.occurred_at > until {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .rev()
            .take(limit as usize)
            .collect();

        Ok(result)
    }

    async fn record_drift_event(&self, event: DriftEventRow) -> Result<DriftRowId, StorageError> {
        let id = event.id.clone();
        let mut drift = self.drift.lock().expect("drift lock poisoned");
        drift.push(event);
        Ok(id)
    }

    async fn latest_drift_event(&self) -> Result<Option<DriftEventRow>, StorageError> {
        let drift = self.drift.lock().expect("drift lock poisoned");
        Ok(drift.last().cloned())
    }

    async fn enqueue_proposal(&self, proposal: ProposalRow) -> Result<ProposalId, StorageError> {
        let mut proposals = self.proposals.lock().expect("proposals lock poisoned");

        // Reject if an open (Pending) proposal with the same (source, source_ref) exists.
        let duplicate_exists = proposals.iter().any(|p| {
            p.state == ProposalState::Pending
                && p.source == proposal.source
                && p.source_ref == proposal.source_ref
        });

        if duplicate_exists {
            return Err(StorageError::ProposalDuplicate {
                proposal_source: format!("{:?}", proposal.source),
                source_ref: proposal.source_ref.unwrap_or_default(),
            });
        }

        let id = proposal.id.clone();
        proposals.push(proposal);
        Ok(id)
    }

    async fn dequeue_proposal(&self) -> Result<Option<ProposalRow>, StorageError> {
        let mut proposals = self.proposals.lock().expect("proposals lock poisoned");

        // Find the oldest pending proposal (lowest index = oldest in insertion order).
        let pos = proposals
            .iter()
            .position(|p| p.state == ProposalState::Pending);

        Ok(pos.map(|idx| proposals.remove(idx)))
    }

    async fn expire_proposals(&self, now: UnixSeconds) -> Result<u32, StorageError> {
        let mut proposals = self.proposals.lock().expect("proposals lock poisoned");

        let mut count: u32 = 0;
        for p in proposals.iter_mut() {
            if p.state == ProposalState::Pending && p.expires_at <= now {
                p.state = ProposalState::Expired;
                count += 1;
            }
        }
        Ok(count)
    }
}

// ---------------------------------------------------------------------------
// Contract tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
mod tests {
    mod contract {
        use crate::storage::{
            audit_vocab::AUDIT_KINDS,
            error::StorageError,
            in_memory::InMemoryStorage,
            trait_def::Storage,
            types::{
                ActorKind, AuditEventRow, AuditOutcome, AuditRowId, AuditSelector, ProposalId,
                ProposalRow, ProposalSource, ProposalState, Snapshot, SnapshotId,
            },
        };

        fn make_snapshot(id: &str, version: i64, parent: Option<&str>) -> Snapshot {
            use crate::canonical_json::CANONICAL_JSON_VERSION;
            Snapshot {
                snapshot_id: SnapshotId(id.to_owned()),
                parent_id: parent.map(|p| SnapshotId(p.to_owned())),
                config_version: version,
                actor: "test".to_owned(),
                intent: "test snapshot".to_owned(),
                correlation_id: "corr-01".to_owned(),
                caddy_version: "2.8.0".to_owned(),
                trilithon_version: "0.1.0".to_owned(),
                created_at_unix_seconds: 1_700_000_000,
                created_at_monotonic_nanos: 0,
                canonical_json_version: CANONICAL_JSON_VERSION,
                desired_state_json: "{}".to_owned(),
            }
        }

        fn make_audit_event(kind: &str) -> AuditEventRow {
            AuditEventRow {
                id: AuditRowId(ulid::Ulid::new().to_string()),
                prev_hash: "0000000000000000000000000000000000000000000000000000000000000000"
                    .to_owned(),
                caddy_instance_id: "local".to_owned(),
                correlation_id: "corr-01".to_owned(),
                occurred_at: 1_700_000_000,
                occurred_at_ms: 1_700_000_000_000,
                actor_kind: ActorKind::System,
                actor_id: "test".to_owned(),
                kind: kind.to_owned(),
                target_kind: None,
                target_id: None,
                snapshot_id: None,
                redacted_diff_json: None,
                redaction_sites: 0,
                outcome: AuditOutcome::Ok,
                error_kind: None,
                notes: None,
            }
        }

        fn make_proposal(
            id: &str,
            source: ProposalSource,
            source_ref: Option<&str>,
            expires_at: i64,
        ) -> ProposalRow {
            ProposalRow {
                id: ProposalId(id.to_owned()),
                correlation_id: "corr-01".to_owned(),
                source,
                source_ref: source_ref.map(str::to_owned),
                payload_json: "{}".to_owned(),
                rationale: None,
                submitted_at: 1_700_000_000,
                expires_at,
                state: ProposalState::Pending,
                wildcard_callout: false,
            }
        }

        #[tokio::test]
        async fn insert_then_get_snapshot_round_trip() {
            let store = InMemoryStorage::new();
            let snap = make_snapshot("aabbcc", 1, None);
            let id = store
                .insert_snapshot(snap.clone())
                .await
                .expect("insert should succeed");

            assert_eq!(id, SnapshotId("aabbcc".to_owned()));

            let fetched = store
                .get_snapshot(&id)
                .await
                .expect("get should succeed")
                .expect("should be Some");
            assert_eq!(fetched.config_version, 1);
        }

        #[tokio::test]
        async fn duplicate_snapshot_byte_equal_is_idempotent() {
            let store = InMemoryStorage::new();
            let snap = make_snapshot("aabbcc", 1, None);
            let id1 = store
                .insert_snapshot(snap.clone())
                .await
                .expect("first insert should succeed");

            // Byte-equal duplicate must succeed (idempotent), not error.
            let id2 = store
                .insert_snapshot(snap)
                .await
                .expect("byte-equal duplicate should succeed");

            assert_eq!(id1, id2, "both inserts must return the same id");
        }

        #[tokio::test]
        async fn duplicate_snapshot_different_body_is_collision() {
            use crate::canonical_json::CANONICAL_JSON_VERSION;
            let store = InMemoryStorage::new();
            let snap = make_snapshot("aabbcc", 1, None);
            store
                .insert_snapshot(snap.clone())
                .await
                .expect("first insert should succeed");

            // Same id, different body → SnapshotHashCollision.
            let collision = Snapshot {
                snapshot_id: SnapshotId("aabbcc".to_owned()),
                desired_state_json: r#"{"different":"body"}"#.to_owned(),
                parent_id: None,
                config_version: 1,
                actor: "test".to_owned(),
                intent: "test snapshot".to_owned(),
                correlation_id: "corr-01".to_owned(),
                caddy_version: "2.8.0".to_owned(),
                trilithon_version: "0.1.0".to_owned(),
                created_at_unix_seconds: 1_700_000_000,
                created_at_monotonic_nanos: 0,
                canonical_json_version: CANONICAL_JSON_VERSION,
            };
            let err = store
                .insert_snapshot(collision)
                .await
                .expect_err("different body should fail");

            assert!(
                matches!(err, StorageError::SnapshotHashCollision { .. }),
                "expected SnapshotHashCollision, got {err:?}"
            );
        }

        #[tokio::test]
        async fn audit_kind_unknown_rejected() {
            let store = InMemoryStorage::new();
            let event = make_audit_event("made.up");
            let err = store
                .record_audit_event(event)
                .await
                .expect_err("unknown kind should fail");

            assert!(
                matches!(err, StorageError::AuditKindUnknown { .. }),
                "expected AuditKindUnknown, got {err:?}"
            );
        }

        #[tokio::test]
        async fn audit_kind_known_accepted() {
            let store = InMemoryStorage::new();
            // Verify this kind is indeed in the vocabulary.
            assert!(
                AUDIT_KINDS.contains(&"config.applied"),
                "config.applied must be in AUDIT_KINDS"
            );

            let event = make_audit_event("config.applied");
            store
                .record_audit_event(event)
                .await
                .expect("known kind should be accepted");
        }

        #[tokio::test]
        async fn tail_audit_log_filters_correctly() {
            let store = InMemoryStorage::new();

            // Insert several events with different actors and times.
            let mut e1 = make_audit_event("config.applied");
            e1.actor_id = "alice".to_owned();
            e1.occurred_at = 1_000;

            let mut e2 = make_audit_event("mutation.submitted");
            e2.actor_id = "bob".to_owned();
            e2.occurred_at = 2_000;

            let mut e3 = make_audit_event("config.applied");
            e3.actor_id = "alice".to_owned();
            e3.occurred_at = 3_000;

            store.record_audit_event(e1).await.expect("e1");
            store.record_audit_event(e2).await.expect("e2");
            store.record_audit_event(e3).await.expect("e3");

            // Filter by actor_id = "alice"; expect 2 rows in reverse-chron order.
            let rows = store
                .tail_audit_log(
                    AuditSelector {
                        actor_id: Some("alice".to_owned()),
                        ..Default::default()
                    },
                    10,
                )
                .await
                .expect("tail should succeed");

            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].occurred_at, 3_000, "newest first");
            assert_eq!(rows[1].occurred_at, 1_000);

            // Filter by kind_glob = "config.*"; expect 2 rows.
            let rows = store
                .tail_audit_log(
                    AuditSelector {
                        kind_glob: Some("config.*".to_owned()),
                        ..Default::default()
                    },
                    10,
                )
                .await
                .expect("kind_glob filter");
            assert_eq!(rows.len(), 2);

            // Limit to 1 row.
            let rows = store
                .tail_audit_log(AuditSelector::default(), 1)
                .await
                .expect("limit");
            assert_eq!(rows.len(), 1);
        }

        #[tokio::test]
        async fn proposal_dedup_on_source_pair() {
            let store = InMemoryStorage::new();
            let p1 = make_proposal("p1", ProposalSource::Docker, Some("container-1"), 9_999_999);
            let p2 = make_proposal("p2", ProposalSource::Docker, Some("container-1"), 9_999_999);

            store
                .enqueue_proposal(p1)
                .await
                .expect("first enqueue should succeed");

            let err = store
                .enqueue_proposal(p2)
                .await
                .expect_err("duplicate enqueue should fail");

            assert!(
                matches!(err, StorageError::ProposalDuplicate { .. }),
                "expected ProposalDuplicate, got {err:?}"
            );
        }

        #[tokio::test]
        async fn expire_proposals_counts() {
            let store = InMemoryStorage::new();

            // One already-expired proposal and one still-live proposal.
            let p_expired = make_proposal("p-exp", ProposalSource::Llm, None, 500);
            let p_live = make_proposal("p-live", ProposalSource::Import, None, 9_999_999);

            store
                .enqueue_proposal(p_expired)
                .await
                .expect("enqueue expired");
            store.enqueue_proposal(p_live).await.expect("enqueue live");

            let count = store
                .expire_proposals(1_000)
                .await
                .expect("expire_proposals should succeed");

            assert_eq!(count, 1, "exactly one proposal should have been expired");
        }

        #[tokio::test]
        async fn audit_chain_first_row_uses_seed() {
            use crate::storage::helpers::audit_prev_hash_seed;

            let store = InMemoryStorage::new();
            let event = make_audit_event("config.applied");
            store
                .record_audit_event(event)
                .await
                .expect("insert should succeed");

            let rows = store
                .tail_audit_log(AuditSelector::default(), 10)
                .await
                .expect("tail should succeed");

            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0].prev_hash,
                audit_prev_hash_seed(),
                "first row must use the all-zero seed"
            );
        }

        #[tokio::test]
        async fn audit_chain_prev_hash_links_rows() {
            use crate::storage::helpers::{
                canonical_json_for_audit_hash, compute_audit_chain_hash,
            };

            let store = InMemoryStorage::new();
            let e1 = make_audit_event("config.applied");
            store
                .record_audit_event(e1)
                .await
                .expect("e1 insert should succeed");

            let e2 = make_audit_event("mutation.submitted");
            store
                .record_audit_event(e2)
                .await
                .expect("e2 insert should succeed");

            // tail_audit_log returns newest-first; reverse to get oldest-first.
            let mut rows = store
                .tail_audit_log(AuditSelector::default(), 10)
                .await
                .expect("tail should succeed");
            rows.reverse();

            assert_eq!(rows.len(), 2);
            let expected = compute_audit_chain_hash(&canonical_json_for_audit_hash(&rows[0]));
            assert_eq!(
                rows[1].prev_hash, expected,
                "second row's prev_hash must equal sha256(canonical_json(first row))"
            );
        }
    }
}
