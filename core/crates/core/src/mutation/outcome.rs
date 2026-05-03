//! Mutation outcome types.

use crate::audit::AuditEvent;
use crate::model::desired_state::DesiredState;

/// The successful result of applying a mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationOutcome {
    /// The desired state after the mutation was applied.
    pub new_state: DesiredState,
    /// Structural diff between the previous and new desired state.
    pub diff: Diff,
    /// Audit event kind that describes this mutation.
    pub kind: AuditEvent,
}

/// Structural diff between two [`DesiredState`] snapshots.
///
/// Phase 8 supplies the full structural-diff shape. For Phase 4 this is an
/// ordered list of changed JSON pointers plus before/after [`serde_json::Value`]s.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diff {
    /// Ordered list of individual field changes.
    pub changes: Vec<DiffChange>,
}

/// A single field change within a [`Diff`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffChange {
    /// JSON Pointer (RFC 6901) to the changed field.
    pub path: crate::model::primitive::JsonPointer,
    /// Value before the mutation (`None` means the field did not exist).
    pub before: Option<serde_json::Value>,
    /// Value after the mutation (`None` means the field was removed).
    pub after: Option<serde_json::Value>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::audit::AuditEvent;
    use crate::model::primitive::JsonPointer;

    #[test]
    fn diff_default_is_empty() {
        let diff = Diff::default();
        assert!(diff.changes.is_empty());
    }

    #[test]
    fn mutation_outcome_round_trips_fields() {
        let state = DesiredState::default();
        let diff = Diff {
            changes: vec![DiffChange {
                path: JsonPointer::root().push("version"),
                before: Some(serde_json::Value::Number(0.into())),
                after: Some(serde_json::Value::Number(1.into())),
            }],
        };
        let outcome = MutationOutcome {
            new_state: state.clone(),
            diff: diff.clone(),
            kind: AuditEvent::MutationApplied,
        };
        assert_eq!(outcome.new_state, state);
        assert_eq!(outcome.diff, diff);
        assert_eq!(outcome.kind, AuditEvent::MutationApplied);
    }
}
