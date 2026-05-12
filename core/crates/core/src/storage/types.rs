//! Row and value types for the `Storage` trait boundary.

use serde::{Deserialize, Serialize};

/// In-memory representation of an ownership-layer audit event.
///
/// Phase 6 will persist these via [`Storage::append_audit_event`].  Until
/// then, callers that generate events keep them in-memory so that unit tests
/// can assert on the constructed value without requiring a database.
///
/// This is distinct from [`crate::audit::AuditEvent`] which covers mutation
/// audit events; this type covers storage-layer lifecycle events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageAuditEvent {
    /// The ownership sentinel was overwritten because `--takeover` was
    /// passed.  Phase 6 will write this to the audit log.
    OwnershipSentinelTakeover {
        /// The previous owner's installation id that was replaced.
        previous_installation_id: String,
        /// Our own installation id that is now written.
        new_installation_id: String,
    },
}

/// Unix epoch seconds.
///
/// Re-exported from [`crate::model::primitive`] for convenience.
pub use crate::model::primitive::UnixSeconds;

/// Content-addressed snapshot identifier — SHA-256 hex, 64 chars.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SnapshotId(pub String);

/// ULID-based identifier for an audit log row.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AuditRowId(pub String);

/// ULID-based identifier for a proposal row.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProposalId(pub String);

/// ULID-based identifier for a drift event row.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DriftRowId(pub String);

/// Immutable, content-addressed snapshot of desired state.
///
/// Field names follow the T1.2 spec exactly.  The `snapshot_id` is the
/// SHA-256 hex digest of the canonical JSON (`desired_state_json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_field_names)]
// reason: T1.2 spec mandates the field be named `snapshot_id`; renaming would break API compat
pub struct Snapshot {
    /// Content-addressed identifier: SHA-256 hex of canonical JSON (64 chars).
    pub snapshot_id: SnapshotId,
    /// Parent snapshot in the chain; `None` for the genesis snapshot.
    pub parent_id: Option<SnapshotId>,
    /// Monotonically increasing config version.
    pub config_version: i64,
    /// Opaque actor identity string (username, token id, etc.).
    pub actor: String,
    /// Human-readable intent, length-bounded at 4 KiB (4 096 bytes).
    ///
    /// Callers that set this field directly MUST validate the value with
    /// [`Snapshot::validate_intent`] before persisting the snapshot.
    pub intent: String,
    /// ULID that ties this snapshot to an audit log entry.
    pub correlation_id: String,
    /// Caddy version string at the time of the snapshot.
    pub caddy_version: String,
    /// Trilithon version string at the time of the snapshot.
    pub trilithon_version: String,
    /// Creation time, whole seconds since the Unix epoch.
    pub created_at_unix_seconds: UnixSeconds,
    /// Creation time in nanoseconds, derived from the wall-clock millisecond
    /// timestamp stored in the legacy `created_at_ms` column (value × 1 000 000).
    /// Sub-millisecond digits are always zero; the field is not a true monotonic
    /// counter despite the name carried forward from the T1.2 spec.
    pub created_at_monotonic_nanos: u64,
    /// Version of the canonical JSON format used to produce `desired_state_json`.
    ///
    /// Always equals [`crate::canonical_json::CANONICAL_JSON_VERSION`] at
    /// the time the snapshot is created.
    pub canonical_json_version: u32,
    /// Canonical JSON encoding of the desired state.
    pub desired_state_json: String,
}

/// Maximum byte length permitted for [`Snapshot::intent`].
pub const INTENT_MAX_BYTES: usize = 4 * 1024;

impl Snapshot {
    /// Validate that `intent` is within the 4 KiB length bound.
    ///
    /// Returns `true` when the intent string is valid, `false` otherwise.
    #[must_use]
    pub const fn validate_intent(intent: &str) -> bool {
        intent.len() <= INTENT_MAX_BYTES
    }
}

impl SnapshotId {
    /// Construct a `SnapshotId` from a hex string, validating that it is
    /// exactly 64 lowercase hexadecimal characters (`[0-9a-f]{64}`).
    ///
    /// # Errors
    ///
    /// Returns the invalid string unchanged when validation fails.
    pub fn try_from_hex(s: String) -> Result<Self, String> {
        if s.len() == 64 && s.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
            Ok(Self(s))
        } else {
            Err(s)
        }
    }
}

/// Classification of the actor that caused a state change or audit event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActorKind {
    /// Human operator acting via the CLI or web UI.
    User,
    /// Automated token (LLM session, CI pipeline, etc.).
    Token,
    /// Internal daemon process.
    System,
}

/// A single audit log row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEventRow {
    /// Row identifier (ULID).
    pub id: AuditRowId,
    /// SHA-256 of previous row's canonical JSON (or all-zero for first row); ADR-0009.
    pub prev_hash: String,
    /// Caddy instance this event originated from. V1: always `"local"`.
    pub caddy_instance_id: String,
    /// Correlation identifier tying this event to a request.
    pub correlation_id: String,
    /// Event time, whole seconds.
    pub occurred_at: UnixSeconds,
    /// Event time, millisecond precision.
    pub occurred_at_ms: i64,
    /// Kind of actor that triggered the event.
    pub actor_kind: ActorKind,
    /// Opaque actor identity string.
    pub actor_id: String,
    /// Event kind string from the §6.6 vocabulary.
    pub kind: String,
    /// Kind of entity that was the target, if applicable.
    pub target_kind: Option<String>,
    /// Identity of the target entity, if applicable.
    pub target_id: Option<String>,
    /// Associated snapshot, if the event produced one.
    pub snapshot_id: Option<SnapshotId>,
    /// Redacted diff payload, if the event involved a state change.
    pub redacted_diff_json: Option<String>,
    /// Number of secret fields that were redacted from the diff.
    pub redaction_sites: u32,
    /// Whether the operation succeeded, errored, or was denied.
    pub outcome: AuditOutcome,
    /// Machine-readable error kind, populated on error or denial.
    pub error_kind: Option<String>,
    /// Free-text notes for operator review.
    pub notes: Option<String>,
}

/// Outcome of the operation that produced an audit event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditOutcome {
    /// The operation completed successfully.
    Ok,
    /// The operation failed with an internal error.
    Error,
    /// The operation was rejected by a policy check.
    Denied,
}

/// Filter predicate for `Storage::tail_audit_log`.
#[derive(Debug, Clone, Default)]
pub struct AuditSelector {
    /// Glob pattern matched against `AuditEventRow::kind`.
    pub kind_glob: Option<String>,
    /// Exact match on `AuditEventRow::actor_id`.
    pub actor_id: Option<String>,
    /// Exact match on `AuditEventRow::correlation_id`.
    pub correlation_id: Option<String>,
    /// Lower bound on `AuditEventRow::occurred_at` (inclusive).
    pub since: Option<UnixSeconds>,
    /// Upper bound on `AuditEventRow::occurred_at` (exclusive).
    pub until: Option<UnixSeconds>,
    /// Cursor for descending pagination; only rows with `id < cursor_before`
    /// are returned.  A `None` value starts from the most recent row.
    pub cursor_before: Option<AuditRowId>,
}

/// The result of walking the parent chain of a snapshot.
#[derive(Debug, Clone)]
pub struct ParentChain {
    /// Snapshots in the chain, oldest first.
    pub snapshots: Vec<Snapshot>,
    /// `true` when the walk stopped at `max_depth` before reaching genesis.
    pub truncated: bool,
}

/// A drift detection event row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftEventRow {
    /// Row identifier (ULID).
    pub id: DriftRowId,
    /// Correlation identifier for the detection run.
    pub correlation_id: String,
    /// Time the drift was detected, whole seconds.
    pub detected_at: UnixSeconds,
    /// Snapshot that was the desired state at detection time.
    pub snapshot_id: SnapshotId,
    /// JSON encoding of the structural diff between desired and live state.
    pub diff_json: String,
    /// SHA-256 hash of the running state at detection time (deduplication key).
    pub running_state_hash: String,
    /// How the drift was resolved, if it has been.
    pub resolution: Option<DriftResolution>,
    /// Time the drift was resolved, if it has been.
    pub resolved_at: Option<UnixSeconds>,
}

/// How a detected drift event was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftResolution {
    /// The desired state was re-applied to Caddy.
    Reapplied,
    /// The live state was accepted as the new desired state.
    Accepted,
    /// A previous snapshot was restored.
    RolledBack,
    /// Action was deferred for later manual reconciliation.
    Deferred,
}

/// A proposal row in the pending-proposals queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalRow {
    /// Row identifier (ULID).
    pub id: ProposalId,
    /// Correlation identifier for the originating request.
    pub correlation_id: String,
    /// Source system that submitted the proposal.
    pub source: ProposalSource,
    /// Source-specific reference (container id, LLM session id, etc.).
    pub source_ref: Option<String>,
    /// JSON encoding of the proposed mutation payload.
    pub payload_json: String,
    /// Human-readable rationale for the proposal.
    pub rationale: Option<String>,
    /// Time the proposal was submitted, whole seconds.
    pub submitted_at: UnixSeconds,
    /// Time after which the proposal expires, whole seconds.
    pub expires_at: UnixSeconds,
    /// Current lifecycle state of the proposal.
    pub state: ProposalState,
    /// `true` when the proposal uses a wildcard scope, requiring extra review.
    pub wildcard_callout: bool,
}

/// Source system that submitted a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProposalSource {
    /// A Docker label-driven auto-discovery event.
    Docker,
    /// A language-model tool-gateway invocation.
    Llm,
    /// A Caddyfile import operation.
    Import,
}

/// Lifecycle state of a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProposalState {
    /// Awaiting operator or automated approval.
    Pending,
    /// Approved and applied.
    Approved,
    /// Rejected by an operator or policy check.
    Rejected,
    /// Timed out before being acted upon.
    Expired,
    /// Superseded by a newer proposal with the same `(source, source_ref)`.
    Superseded,
}

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
    use crate::canonical_json::CANONICAL_JSON_VERSION;

    fn make_snapshot() -> Snapshot {
        Snapshot {
            snapshot_id: SnapshotId(
                "a".repeat(64), // placeholder hex
            ),
            parent_id: None,
            config_version: 1,
            actor: "test-actor".to_owned(),
            intent: "initial bootstrap".to_owned(),
            correlation_id: "01HCORRELATION0000000000AB".to_owned(),
            caddy_version: "2.8.0".to_owned(),
            trilithon_version: "0.1.0".to_owned(),
            created_at_unix_seconds: 1_700_000_000,
            created_at_monotonic_nanos: 1_000_000_000_u64,
            canonical_json_version: CANONICAL_JSON_VERSION,
            desired_state_json: "{}".to_owned(),
        }
    }

    #[test]
    fn snapshot_has_all_required_fields() {
        let snap = make_snapshot();

        // snapshot_id: SHA-256 hex (64 chars) identifier.
        assert_eq!(snap.snapshot_id.0.len(), 64);

        // parent_id: optional reference to parent snapshot.
        assert!(snap.parent_id.is_none());

        // config_version: monotonically increasing i64.
        assert_eq!(snap.config_version, 1_i64);

        // actor: opaque identity string.
        assert_eq!(snap.actor, "test-actor");

        // intent: human-readable, bounded at 4 KiB.
        assert!(Snapshot::validate_intent(&snap.intent));
        assert!(!snap.intent.is_empty());

        // correlation_id: ULID-format string.
        assert!(!snap.correlation_id.is_empty());

        // caddy_version: version string.
        assert!(!snap.caddy_version.is_empty());

        // trilithon_version: version string.
        assert!(!snap.trilithon_version.is_empty());

        // created_at_unix_seconds: whole Unix seconds.
        assert_eq!(snap.created_at_unix_seconds, 1_700_000_000_i64);

        // created_at_monotonic_nanos: nanosecond counter.
        assert_eq!(snap.created_at_monotonic_nanos, 1_000_000_000_u64);

        // canonical_json_version: must record the version constant.
        assert_eq!(snap.canonical_json_version, CANONICAL_JSON_VERSION);

        // desired_state_json: the canonical JSON payload.
        assert_eq!(snap.desired_state_json, "{}");
    }

    #[test]
    fn intent_4kib_boundary() {
        // Exactly 4096 bytes is valid.
        let at_limit = "x".repeat(INTENT_MAX_BYTES);
        assert!(Snapshot::validate_intent(&at_limit));

        // 4097 bytes is invalid.
        let over_limit = "x".repeat(INTENT_MAX_BYTES + 1);
        assert!(!Snapshot::validate_intent(&over_limit));
    }

    #[test]
    fn snapshot_canonical_json_version_matches_constant() {
        // The field on the snapshot row must equal the module-level constant.
        let snap = make_snapshot();
        assert_eq!(snap.canonical_json_version, CANONICAL_JSON_VERSION);
    }

    #[test]
    fn snapshot_round_trip_serde() {
        let snap = make_snapshot();
        let json = serde_json::to_string(&snap).expect("serialise");
        let restored: Snapshot = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(restored.snapshot_id.0, snap.snapshot_id.0);
        assert_eq!(restored.config_version, snap.config_version);
        assert_eq!(restored.canonical_json_version, snap.canonical_json_version);
        assert_eq!(
            restored.created_at_monotonic_nanos,
            snap.created_at_monotonic_nanos
        );
    }
}
