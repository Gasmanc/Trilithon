//! Row and value types for the `Storage` trait boundary.

use serde::{Deserialize, Serialize};

/// In-memory representation of a structured audit event.
///
/// Phase 6 will persist these via [`Storage::append_audit_event`].  Until
/// then, callers that generate events keep them in-memory so that unit tests
/// can assert on the constructed value without requiring a database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEvent {
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
pub type UnixSeconds = i64;

/// Content-addressed snapshot identifier — SHA-256 hex, 64 chars.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SnapshotId(pub String);

/// ULID-based identifier for an audit log row.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AuditRowId(pub String);

/// ULID-based identifier for a proposal row.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProposalId(pub String);

/// ULID-based identifier for a drift event row.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DriftRowId(pub String);

/// Immutable, content-addressed snapshot of desired state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Content-addressed identifier (SHA-256 of canonical JSON).
    pub id: SnapshotId,
    /// Parent snapshot in the chain; `None` for the genesis snapshot.
    pub parent_id: Option<SnapshotId>,
    /// Caddy instance this snapshot targets. V1: always `"local"`.
    pub caddy_instance_id: String,
    /// Kind of actor that produced this snapshot.
    pub actor_kind: ActorKind,
    /// Opaque actor identity string (username, token id, etc.).
    pub actor_id: String,
    /// Human-readable intent, length-bounded at 4 KiB.
    pub intent: String,
    /// ULID that ties this snapshot to an audit log entry.
    pub correlation_id: String,
    /// Caddy version string at the time of the snapshot.
    pub caddy_version: String,
    /// Trilithon version string at the time of the snapshot.
    pub trilithon_version: String,
    /// Creation time, whole seconds.
    pub created_at: UnixSeconds,
    /// Creation time, millisecond precision.
    pub created_at_ms: i64,
    /// Monotonically increasing config version.
    pub config_version: i64,
    /// Canonical JSON encoding of the desired state.
    pub desired_state_json: String,
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
    /// Upper bound on `AuditEventRow::occurred_at` (inclusive).
    pub until: Option<UnixSeconds>,
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
