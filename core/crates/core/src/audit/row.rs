//! Row-level record and query-selector types for the audit log (Slice 6.2).
//!
//! These types define the wire surface exchanged between `core` and the
//! `Storage` trait for `record_audit_event` and `tail_audit_log`.  They are
//! pure-core: no `SQLite` types, no I/O.
//!
//! # Key types
//!
//! - [`AuditRowId`] — ULID-based row identifier.
//! - [`ActorRef`] — structured actor reference (user, token, system, docker).
//! - [`AuditOutcome`] — operation outcome: `Ok`, `Error`, or `Denied`.
//! - [`AuditEventRow`] — the storable record, mirroring `audit_log` columns (§6.6).
//! - [`AuditSelector`] — filter / pagination predicate for `tail_audit_log`.

use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::audit::event::AuditEvent;
use crate::storage::types::SnapshotId;

/// Default page size for [`AuditSelector::limit`].
pub const AUDIT_QUERY_DEFAULT_LIMIT: u32 = 100;

/// Maximum page size for [`AuditSelector::limit`].
pub const AUDIT_QUERY_MAX_LIMIT: u32 = 1000;

// ── Identifiers ─────────────────────────────────────────────────────────────

/// ULID-based identifier for an audit log row.
///
/// The inner [`Ulid`] is monotonically sortable and encodes the creation
/// timestamp, making it suitable as a pagination cursor.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct AuditRowId(pub Ulid);

// ── Actor reference ──────────────────────────────────────────────────────────

/// Structured reference to the actor that triggered an audit event.
///
/// At the storage boundary this projects onto `audit_log.actor_kind` plus
/// `audit_log.actor_id`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActorRef {
    /// A human operator identified by their account id.
    User {
        /// Opaque account identifier.
        id: String,
    },
    /// An automated token (LLM session, CI pipeline, API key).
    Token {
        /// Opaque token identifier.
        id: String,
    },
    /// An internal daemon component.
    System {
        /// Short component name, e.g. `"drift-watcher"`.
        component: String,
    },
    /// The Docker integration (no further identity available).
    Docker,
}

// ── Outcome ──────────────────────────────────────────────────────────────────

/// Operation outcome recorded in an audit event.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    /// The operation completed successfully.
    Ok,
    /// The operation failed with an internal error.
    Error,
    /// The operation was rejected by a policy check.
    Denied,
}

// ── Row ──────────────────────────────────────────────────────────────────────

/// A single audit log row, mirroring the `audit_log` columns (architecture §6.6).
///
/// The [`actor`](AuditEventRow::actor) field projects onto
/// `actor_kind` + `actor_id` at the storage boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditEventRow {
    /// Row identifier (ULID).
    pub id: AuditRowId,
    /// Correlation identifier tying this event to a request or operation.
    pub correlation_id: Ulid,
    /// Event time, whole seconds since the Unix epoch (UTC).
    pub occurred_at: i64,
    /// Event time in milliseconds since the Unix epoch (UTC).
    pub occurred_at_ms: i64,
    /// Actor that triggered the event.
    pub actor: ActorRef,
    /// Typed audit event from the closed §6.6 vocabulary.
    pub event: AuditEvent,
    /// Kind of entity that was the target, if applicable (e.g. `"route"`).
    pub target_kind: Option<String>,
    /// Identity of the target entity, if applicable.
    pub target_id: Option<String>,
    /// Associated snapshot, if the event produced one.
    pub snapshot_id: Option<SnapshotId>,
    /// Canonical JSON of the redacted diff, if the event involved a state change.
    pub redacted_diff_json: Option<String>,
    /// Number of secret fields redacted from the diff.
    pub redaction_sites: u32,
    /// Whether the operation succeeded, errored, or was denied.
    pub outcome: AuditOutcome,
    /// Machine-readable error kind, populated on error or denial.
    pub error_kind: Option<String>,
    /// Free-text notes for operator review.
    pub notes: Option<String>,
}

// ── Selector ─────────────────────────────────────────────────────────────────

/// Filter and pagination predicate for `Storage::tail_audit_log`.
///
/// Call [`AuditSelector::normalised`] at the storage boundary to clamp
/// `limit` to `[1, AUDIT_QUERY_MAX_LIMIT]` and fill in the default.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AuditSelector {
    /// Lower bound on `occurred_at`, inclusive (Unix seconds).
    pub since: Option<i64>,
    /// Upper bound on `occurred_at`, exclusive (Unix seconds).
    pub until: Option<i64>,
    /// Exact match on `correlation_id`.
    pub correlation_id: Option<Ulid>,
    /// Exact match on the actor identity string.
    pub actor_id: Option<String>,
    /// Exact match on the audit event kind.
    pub event: Option<AuditEvent>,
    /// Maximum number of rows to return.
    ///
    /// Clamped to `[1, AUDIT_QUERY_MAX_LIMIT]` by [`AuditSelector::normalised`].
    /// `None` defaults to [`AUDIT_QUERY_DEFAULT_LIMIT`].
    pub limit: Option<u32>,
    /// Cursor for descending pagination; rows older than this id are returned.
    pub cursor_before: Option<AuditRowId>,
}

/// An [`AuditSelector`] with a guaranteed concrete `limit`.
#[derive(Clone, Debug)]
pub struct NormalisedAuditSelector {
    /// The original selector, unchanged except for `limit`.
    pub selector: AuditSelector,
    /// The effective row limit, clamped to `[1, AUDIT_QUERY_MAX_LIMIT]`.
    pub limit: u32,
}

impl AuditSelector {
    /// Return a copy of this selector with `limit` clamped to
    /// `[1, AUDIT_QUERY_MAX_LIMIT]`, defaulting to [`AUDIT_QUERY_DEFAULT_LIMIT`]
    /// when `None`.
    ///
    /// Callers at the storage boundary MUST call this before passing the
    /// limit to a query.
    #[must_use]
    pub fn normalised(self) -> NormalisedAuditSelector {
        let limit = self.limit.map_or(AUDIT_QUERY_DEFAULT_LIMIT, |v| {
            v.clamp(1, AUDIT_QUERY_MAX_LIMIT)
        });
        NormalisedAuditSelector {
            selector: self,
            limit,
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use super::*;

    fn full_row() -> AuditEventRow {
        AuditEventRow {
            id: AuditRowId(Ulid::from_parts(1_700_000_000_000, 42)),
            correlation_id: Ulid::from_parts(1_700_000_000_001, 99),
            occurred_at: 1_700_000_000,
            occurred_at_ms: 1_700_000_000_123,
            actor: ActorRef::User {
                id: "user-abc".to_owned(),
            },
            event: AuditEvent::ApplySucceeded,
            target_kind: Some("route".to_owned()),
            target_id: Some("route-123".to_owned()),
            snapshot_id: Some(SnapshotId("a".repeat(64))),
            redacted_diff_json: Some(r#"{"before":{},"after":{}}"#.to_owned()),
            redaction_sites: 3,
            outcome: AuditOutcome::Ok,
            error_kind: None,
            notes: Some("operator note".to_owned()),
        }
    }

    fn minimal_row() -> AuditEventRow {
        AuditEventRow {
            id: AuditRowId(Ulid::from_parts(1_700_000_000_000, 1)),
            correlation_id: Ulid::from_parts(1_700_000_000_000, 2),
            occurred_at: 1_700_000_000,
            occurred_at_ms: 1_700_000_000_000,
            actor: ActorRef::Docker,
            event: AuditEvent::ApplyFailed,
            target_kind: None,
            target_id: None,
            snapshot_id: None,
            redacted_diff_json: None,
            redaction_sites: 0,
            outcome: AuditOutcome::Error,
            error_kind: None,
            notes: None,
        }
    }

    #[test]
    fn serde_round_trip_full_row() {
        let row = full_row();
        let json = serde_json::to_string(&row).expect("serialise full row");
        let restored: AuditEventRow = serde_json::from_str(&json).expect("deserialise full row");
        assert_eq!(row, restored, "full row round-trip must be byte-stable");
    }

    #[test]
    fn serde_round_trip_minimal_row() {
        let row = minimal_row();
        let json = serde_json::to_string(&row).expect("serialise minimal row");
        let restored: AuditEventRow = serde_json::from_str(&json).expect("deserialise minimal row");
        assert_eq!(row, restored, "minimal row round-trip must be byte-stable");
        assert!(restored.target_kind.is_none(), "target_kind should be None");
        assert!(restored.snapshot_id.is_none(), "snapshot_id should be None");
        assert!(restored.error_kind.is_none(), "error_kind should be None");
    }

    #[test]
    fn selector_normalises_limit() {
        // Over the max → clamped to max.
        let over = AuditSelector {
            limit: Some(9999),
            ..Default::default()
        };
        assert_eq!(over.normalised().limit, AUDIT_QUERY_MAX_LIMIT);

        // None → default.
        let none = AuditSelector::default();
        assert_eq!(none.normalised().limit, AUDIT_QUERY_DEFAULT_LIMIT);

        // Within range → unchanged.
        let mid = AuditSelector {
            limit: Some(50),
            ..Default::default()
        };
        assert_eq!(mid.normalised().limit, 50);

        // Zero → clamped to 1 (minimum).
        let zero = AuditSelector {
            limit: Some(0),
            ..Default::default()
        };
        assert_eq!(zero.normalised().limit, 1);
    }

    #[test]
    fn actor_serialises_externally_tagged() {
        let actor = ActorRef::User {
            id: "u-1".to_owned(),
        };
        let json = serde_json::to_string(&actor).expect("serialise actor");
        // Internally tagged via `#[serde(tag = "kind")]`: `{"kind":"user","id":"u-1"}`.
        assert!(
            json.contains(r#""kind":"user""#),
            "wire form must include externally-tagged kind field; got: {json}"
        );
        assert!(
            json.contains(r#""id":"u-1""#),
            "wire form must include id field; got: {json}"
        );

        // Docker has no fields beyond the tag.
        let docker_json = serde_json::to_string(&ActorRef::Docker).expect("serialise docker actor");
        assert!(
            docker_json.contains(r#""kind":"docker""#),
            "docker variant must use snake_case tag; got: {docker_json}"
        );
    }
}
