//! Resolution APIs for drift events.
//!
//! Three pure, synchronous resolvers translate a [`DriftEvent`] into exactly
//! one [`Mutation`]:
//!
//! - [`adopt_running_state`] — replace desired state with the running state.
//! - [`reapply_desired_state`] — re-push the existing desired state.
//! - [`defer_for_manual_reconciliation`] — no-op marker for Phase 15 editor.

use crate::diff::DriftEvent;
use crate::model::desired_state::DesiredState;
use crate::model::primitive::JsonPointer;
use crate::mutation::types::Mutation;
use crate::storage::types::SnapshotId;

/// The JSON pointer path where the ownership sentinel lives in Caddy config.
const OWNERSHIP_SENTINEL_PATH: &str = "/storage/trilithon-owner";

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors returned by resolution functions.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ResolveError {
    /// The running state could not be parsed as desired state at the given path.
    #[error("running state could not be parsed as desired state at {path}")]
    UnparseableRunning {
        /// Pointer to the problematic location.
        path: JsonPointer,
    },
    /// The drift event references a snapshot that does not exist.
    #[error("drift event references missing snapshot {0:?}")]
    MissingSnapshot(SnapshotId),
    /// The running state lacks the ownership sentinel (Constraint 12).
    #[error("running state lacks ownership sentinel at {path}")]
    SentinelAbsent {
        /// Pointer to the expected sentinel location.
        path: JsonPointer,
    },
}

// ---------------------------------------------------------------------------
// Source tag for mutations produced by resolvers
// ---------------------------------------------------------------------------

/// Identifies the resolution strategy that produced a mutation.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ResolveSource {
    /// Produced by [`adopt_running_state`].
    DriftAdopt(ulid::Ulid),
    /// Produced by [`reapply_desired_state`].
    DriftReapply(ulid::Ulid),
}

// ---------------------------------------------------------------------------
// Resolvers
// ---------------------------------------------------------------------------

/// Adopt the running state as the new desired state.
///
/// Produces a [`Mutation::ReplaceDesiredState`] that, when applied, overwrites
/// the desired state with `running_state`.
///
/// # Errors
///
/// Returns [`ResolveError::SentinelAbsent`] if `running_state` does not
/// contain the ownership sentinel at `/storage/trilithon-owner`.
pub fn adopt_running_state(
    event: &DriftEvent,
    running_state: &DesiredState,
    desired_version: i64,
) -> Result<Mutation, ResolveError> {
    // Constraint 12: reject running states without ownership sentinel.
    let sentinel_ptr = JsonPointer(OWNERSHIP_SENTINEL_PATH.to_owned());
    if !running_state.unknown_extensions.contains_key(&sentinel_ptr) {
        return Err(ResolveError::SentinelAbsent { path: sentinel_ptr });
    }

    Ok(Mutation::ReplaceDesiredState {
        expected_version: desired_version,
        new_state: Box::new(running_state.clone()),
        source: ResolveSource::DriftAdopt(event.correlation_id),
    })
}

/// Re-apply the current desired state to Caddy.
///
/// Produces a [`Mutation::ReapplySnapshot`] that instructs the applier to
/// re-push the snapshot identified by the event's `before_snapshot_id`.
///
/// # Errors
///
/// Returns [`ResolveError::MissingSnapshot`] if `desired_state.version` is 0,
/// indicating no snapshot has ever been taken.
pub fn reapply_desired_state(
    event: &DriftEvent,
    desired_state: &DesiredState,
) -> Result<Mutation, ResolveError> {
    if desired_state.version == 0 {
        return Err(ResolveError::MissingSnapshot(
            event.before_snapshot_id.clone(),
        ));
    }

    Ok(Mutation::ReapplySnapshot {
        expected_version: desired_state.version,
        snapshot_id: event.before_snapshot_id.clone(),
        source: ResolveSource::DriftReapply(event.correlation_id),
    })
}

/// Defer resolution for manual reconciliation (Phase 15 dual-pane editor).
///
/// Produces a [`Mutation::DriftDeferred`] that is a no-op at the apply path.
/// It records an audit row with `notes.resolution = "deferred"` (Slice 8.6).
pub const fn defer_for_manual_reconciliation(event: &DriftEvent) -> Mutation {
    Mutation::DriftDeferred {
        expected_version: 0,
        event_correlation: event.correlation_id,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    missing_docs
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make_drift_event() -> DriftEvent {
        use crate::diff::{DiffCounts, ObjectKind};

        let mut summary = BTreeMap::new();
        summary.insert(
            ObjectKind::Route,
            DiffCounts {
                added: 1,
                removed: 0,
                modified: 0,
            },
        );

        DriftEvent {
            before_snapshot_id: SnapshotId("snap-before-001".to_owned()),
            running_state_hash: "a".repeat(64),
            diff_summary: summary,
            detected_at: 1_700_000_000,
            correlation_id: ulid::Ulid::from_string("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap(),
            redacted_diff_json: r#"{"redacted":true}"#.to_owned(),
            redaction_sites: 0,
        }
    }

    fn running_state_with_sentinel() -> DesiredState {
        let mut state = DesiredState::empty();
        state.version = 5;
        state.unknown_extensions.insert(
            JsonPointer(OWNERSHIP_SENTINEL_PATH.to_owned()),
            serde_json::json!("trilithon-instance-001"),
        );
        state
    }

    fn running_state_without_sentinel() -> DesiredState {
        let mut state = DesiredState::empty();
        state.version = 5;
        state
    }

    #[test]
    fn adopt_produces_replace_mutation() {
        let event = make_drift_event();
        let running = running_state_with_sentinel();

        let mutation = adopt_running_state(&event, &running, 5).expect("should succeed");

        match mutation {
            Mutation::ReplaceDesiredState {
                new_state, source, ..
            } => {
                assert_eq!(*new_state, running);
                assert_eq!(source, ResolveSource::DriftAdopt(event.correlation_id));
            }
            other => panic!("expected ReplaceDesiredState, got {other:?}"),
        }
    }

    #[test]
    fn reapply_targets_before_snapshot_id() {
        let event = make_drift_event();
        let desired = running_state_with_sentinel();

        let mutation = reapply_desired_state(&event, &desired).expect("should succeed");

        match mutation {
            Mutation::ReapplySnapshot {
                snapshot_id,
                source,
                ..
            } => {
                assert_eq!(snapshot_id, event.before_snapshot_id);
                assert_eq!(source, ResolveSource::DriftReapply(event.correlation_id));
            }
            other => panic!("expected ReapplySnapshot, got {other:?}"),
        }
    }

    #[test]
    fn defer_produces_no_op_marker() {
        let event = make_drift_event();

        let mutation = defer_for_manual_reconciliation(&event);

        match mutation {
            Mutation::DriftDeferred {
                event_correlation, ..
            } => {
                assert_eq!(event_correlation, event.correlation_id);
            }
            other => panic!("expected DriftDeferred, got {other:?}"),
        }
    }

    #[test]
    fn exactly_one_mutation_per_call() {
        let event = make_drift_event();
        let running = running_state_with_sentinel();

        // Each function returns exactly one Result<Mutation> or Mutation.
        let _adopt = adopt_running_state(&event, &running, 5).expect("ok");
        let _reapply = reapply_desired_state(&event, &running).expect("ok");
        let _defer = defer_for_manual_reconciliation(&event);
        // If this compiles and runs, each call produces exactly one mutation.
    }

    #[test]
    fn adopt_rejects_sentinel_absent() {
        let event = make_drift_event();
        let running = running_state_without_sentinel();

        let err = adopt_running_state(&event, &running, 5).expect_err("should fail");

        match err {
            ResolveError::SentinelAbsent { path } => {
                assert_eq!(path.as_str(), OWNERSHIP_SENTINEL_PATH);
            }
            other => panic!("expected SentinelAbsent, got {other:?}"),
        }
    }
}
