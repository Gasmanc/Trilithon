//! `AuditWriter` — the single path into the `audit_log` table (Slice 6.5).
//!
//! Every adapter that needs to record an audit event calls
//! [`AuditWriter::record`].  The writer is the only place that reaches
//! [`Storage::record_audit_event`]; no other call site may invoke that method
//! directly (H10, ADR-0009).
//!
//! Internally the writer:
//! 1. Generates a fresh [`AuditRowId`].
//! 2. Reads the wall clock to obtain `occurred_at` (seconds) and
//!    `occurred_at_ms` (milliseconds).
//! 3. Runs the redactor on any `diff` payload before storing it.
//! 4. Constructs a [`storage::types::AuditEventRow`] and delegates to
//!    [`Storage::record_audit_event`].

use std::sync::Arc;

use serde_json::Value;
use ulid::Ulid;

use trilithon_core::{
    audit::redactor::{CiphertextHasher, RedactorError, SecretsRedactor},
    clock::Clock,
    schema::SchemaRegistry,
    storage::{
        Storage, StorageError,
        helpers::audit_prev_hash_seed,
        types::{ActorKind, AuditEventRow, AuditOutcome, AuditRowId, SnapshotId},
    },
};

/// Type alias for the boxed redactor closure stored inside [`AuditWriter`].
type RedactorFn = Arc<dyn Fn(&Value) -> Result<(Value, u32), AuditWriteError> + Send + Sync>;

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors that [`AuditWriter::record`] can return.
#[derive(Debug, thiserror::Error)]
pub enum AuditWriteError {
    /// The redactor rejected the diff payload.
    #[error("redaction failed: {0}")]
    Redaction(#[from] RedactorError),
    /// The underlying storage layer returned an error.
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    /// The redacted diff could not be serialized to JSON.
    ///
    /// Audit rows are immutable once written; a serialization failure is
    /// propagated as an error rather than silently stored as `"null"`.
    #[error("serialization failed: {0}")]
    Serialization(String),
    /// A length-bounded text field exceeded its cap.
    #[error("audit field {field} exceeded {limit}-byte cap (was {actual} bytes)")]
    FieldTooLong {
        /// The offending field name (`notes` or `target_id`).
        field: &'static str,
        /// The byte length of the supplied value.
        actual: usize,
        /// The configured cap.
        limit: usize,
    },
}

// ── ActorRef ──────────────────────────────────────────────────────────────────

/// Structured reference to the actor that triggered an audit event.
///
/// Mirrors `audit::row::ActorRef` but lives at the adapter boundary so that
/// callers outside `core` can construct it without a `core`-internal import.
#[derive(Clone, Debug)]
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

impl ActorRef {
    /// Decompose into the `(ActorKind, actor_id)` pair stored in the row.
    fn into_kind_and_id(self) -> (ActorKind, String) {
        match self {
            Self::User { id } => (ActorKind::User, id),
            Self::Token { id } => (ActorKind::Token, id),
            Self::System { component } => (ActorKind::System, component),
            Self::Docker => (ActorKind::System, "docker".to_owned()),
        }
    }
}

// ── AuditAppend ───────────────────────────────────────────────────────────────

/// Input to [`AuditWriter::record`].
///
/// The `diff` field carries un-redacted content; the writer applies the
/// redactor before the value reaches storage.
///
/// # Secret-handling contract for `notes` and `target_id`
///
/// `notes` and `target_id` are stored **verbatim** and are NOT passed through
/// the redactor.  Callers MUST NOT place secret material in these fields — no
/// bearer tokens, no PEM-encoded keys, no API keys, no Basic-auth strings.
/// Both fields are length-bounded by [`NOTES_MAX_LEN`] and [`TARGET_ID_MAX_LEN`]
/// to bound row size; values exceeding the cap are rejected with
/// [`AuditWriteError::FieldTooLong`].
#[derive(Clone, Debug)]
pub struct AuditAppend {
    /// Correlation identifier tying this event to a request or operation.
    pub correlation_id: Ulid,
    /// Actor that triggered the event.
    pub actor: ActorRef,
    /// Typed audit event from the closed §6.6 vocabulary.
    pub event: trilithon_core::audit::AuditEvent,
    /// Kind of entity that was the target, if applicable (e.g. `"route"`).
    pub target_kind: Option<String>,
    /// Identity of the target entity, if applicable.
    ///
    /// MUST NOT contain secret material (see struct-level contract).
    pub target_id: Option<String>,
    /// Associated snapshot, if the event produced one.
    pub snapshot_id: Option<SnapshotId>,
    /// Un-redacted diff payload. The writer redacts this before storage.
    /// Pass `None` when the event has no associated diff.
    pub diff: Option<Value>,
    /// Whether the operation succeeded, errored, or was denied.
    pub outcome: AuditOutcome,
    /// Machine-readable error kind, populated on error or denial.
    pub error_kind: Option<String>,
    /// Free-text notes for operator review.
    ///
    /// MUST NOT contain secret material (see struct-level contract).
    pub notes: Option<String>,
}

/// Maximum length of `AuditAppend::notes` in bytes (4 KiB).
pub const NOTES_MAX_LEN: usize = 4 * 1024;

/// Maximum length of `AuditAppend::target_id` in bytes (256 B).
pub const TARGET_ID_MAX_LEN: usize = 256;

impl AuditAppend {
    /// Construct an `AuditAppend` whose `correlation_id` is populated from the
    /// current tracing span via
    /// [`crate::tracing_correlation::current_correlation_id`].
    ///
    /// Use this helper from request handlers and background tasks to avoid
    /// the silent-divergence trap where a caller passes a `correlation_id`
    /// different from the one stamped on the active span.  Callers needing to
    /// override (e.g. resuming a saga with a stored id) construct `AuditAppend`
    /// directly.
    #[must_use]
    pub fn from_current_span(
        actor: ActorRef,
        event: trilithon_core::audit::AuditEvent,
        outcome: AuditOutcome,
    ) -> Self {
        Self {
            correlation_id: crate::tracing_correlation::current_correlation_id(),
            actor,
            event,
            target_kind: None,
            target_id: None,
            snapshot_id: None,
            diff: None,
            outcome,
            error_kind: None,
            notes: None,
        }
    }
}

// ── AuditWriter ───────────────────────────────────────────────────────────────

/// The single, crate-level entry point for writing to `audit_log`.
///
/// Construct via [`AuditWriter::new`] and inject as `Arc<AuditWriter>` (or
/// hold directly via `Clone`) into any adapter that needs to emit audit events.
pub struct AuditWriter {
    storage: Arc<dyn Storage>,
    redactor: RedactorFn,
    clock: Arc<dyn Clock>,
    caddy_instance_id: String,
}

/// Default `caddy_instance_id` when none is provided (single-instance deployment).
pub const DEFAULT_CADDY_INSTANCE_ID: &str = "local";

impl AuditWriter {
    /// Construct a new `AuditWriter`.
    ///
    /// # Parameters
    ///
    /// - `storage`: the persistent store; must outlive the writer.
    /// - `clock`: wall-clock source; use [`trilithon_core::clock::SystemClock`]
    ///   in production and a deterministic double in tests.
    /// - `redactor`: a [`SecretsRedactor`] with `'static` lifetime. The writer
    ///   captures it behind a type-erased closure so it can be cloned and shared
    ///   across threads without carrying a lifetime parameter.
    pub fn new(
        storage: Arc<dyn Storage>,
        clock: Arc<dyn Clock>,
        redactor: SecretsRedactor<'static>,
    ) -> Self {
        Self::new_with_instance(
            storage,
            clock,
            redactor,
            DEFAULT_CADDY_INSTANCE_ID.to_owned(),
        )
    }

    /// Construct an `AuditWriter` with an explicit `caddy_instance_id`.
    ///
    /// Use this in multi-instance deployments where each daemon must stamp its
    /// audit rows with a distinguishable identifier.  Single-instance
    /// deployments should use [`AuditWriter::new`], which defaults to
    /// [`DEFAULT_CADDY_INSTANCE_ID`].
    pub fn new_with_instance(
        storage: Arc<dyn Storage>,
        clock: Arc<dyn Clock>,
        redactor: SecretsRedactor<'static>,
        caddy_instance_id: String,
    ) -> Self {
        // Wrap the redactor in a closure so `AuditWriter` is `'static`.
        //
        // `redact_diff` is used instead of `redact` because diff payloads
        // follow the Phase 8 `{ added, removed, modified }` envelope shape:
        // secret paths are nested one level deeper, and `redact` would not
        // match them.  `redact_diff` descends into each top-level key before
        // running the path-matching walk.
        let redactor_fn = Arc::new(
            move |value: &Value| -> Result<(Value, u32), AuditWriteError> {
                let result = redactor
                    .redact_diff(value)
                    .map_err(AuditWriteError::Redaction)?;
                Ok((result.value, result.sites))
            },
        );
        Self {
            storage,
            redactor: redactor_fn,
            clock,
            caddy_instance_id,
        }
    }

    /// Construct an `AuditWriter` from `Arc`-owned registry and hasher.
    ///
    /// Prefer this over [`AuditWriter::new`] in daemon code — it avoids
    /// `Box::leak` by cloning the `Arc`s into the redactor closure.
    pub fn new_with_arcs(
        storage: Arc<dyn Storage>,
        clock: Arc<dyn Clock>,
        registry: Arc<SchemaRegistry>,
        hasher: Arc<dyn CiphertextHasher>,
    ) -> Self {
        Self::new_with_arcs_and_instance(
            storage,
            clock,
            registry,
            hasher,
            DEFAULT_CADDY_INSTANCE_ID.to_owned(),
        )
    }

    /// As [`AuditWriter::new_with_arcs`], but with an explicit `caddy_instance_id`.
    pub fn new_with_arcs_and_instance(
        storage: Arc<dyn Storage>,
        clock: Arc<dyn Clock>,
        registry: Arc<SchemaRegistry>,
        hasher: Arc<dyn CiphertextHasher>,
        caddy_instance_id: String,
    ) -> Self {
        let redactor_fn = Arc::new(
            move |value: &Value| -> Result<(Value, u32), AuditWriteError> {
                let redactor = SecretsRedactor::new(&registry, &*hasher);
                let result = redactor
                    .redact_diff(value)
                    .map_err(AuditWriteError::Redaction)?;
                Ok((result.value, result.sites))
            },
        );
        Self {
            storage,
            redactor: redactor_fn,
            clock,
            caddy_instance_id,
        }
    }

    /// Redact a diff value using the writer's configured redactor.
    ///
    /// Returns `(redacted_json_string, redaction_site_count)`. Use this when
    /// storing diff payloads outside the audit log (e.g., `drift_events`) to
    /// ensure secrets are masked consistently.
    ///
    /// # Errors
    ///
    /// Returns [`AuditWriteError::Redaction`] if the redactor rejects the payload,
    /// or [`AuditWriteError::Serialization`] if serialisation fails.
    pub fn redact_diff(&self, diff: &Value) -> Result<(String, u32), AuditWriteError> {
        let (redacted, sites) = (self.redactor)(diff)?;
        let json = serde_json::to_string(&redacted)
            .map_err(|e| AuditWriteError::Serialization(e.to_string()))?;
        Ok((json, sites))
    }

    /// The single, public path into `audit_log`.
    ///
    /// Generates a fresh row id, reads the clock, runs the redactor on any
    /// diff, constructs the row, and delegates to
    /// [`Storage::record_audit_event`].
    ///
    /// # Errors
    ///
    /// - [`AuditWriteError::Redaction`] if the redactor rejects the diff.
    /// - [`AuditWriteError::Storage`] if the storage layer returns an error.
    pub async fn record(&self, append: AuditAppend) -> Result<AuditRowId, AuditWriteError> {
        // Enforce length caps on free-text fields to bound row size.
        if let Some(ref notes) = append.notes {
            if notes.len() > NOTES_MAX_LEN {
                return Err(AuditWriteError::FieldTooLong {
                    field: "notes",
                    actual: notes.len(),
                    limit: NOTES_MAX_LEN,
                });
            }
        }
        if let Some(ref target_id) = append.target_id {
            if target_id.len() > TARGET_ID_MAX_LEN {
                return Err(AuditWriteError::FieldTooLong {
                    field: "target_id",
                    actual: target_id.len(),
                    limit: TARGET_ID_MAX_LEN,
                });
            }
        }

        let row_id = AuditRowId(Ulid::new().to_string());
        let now_ms = self.clock.now_unix_ms();

        let (redacted_diff_json, redaction_sites) = if let Some(ref diff) = append.diff {
            let (redacted, sites) = (self.redactor)(diff)?;
            let json = serde_json::to_string(&redacted)
                .map_err(|e| AuditWriteError::Serialization(e.to_string()))?;
            (Some(json), sites)
        } else {
            (None, 0u32)
        };

        let (actor_kind, actor_id) = append.actor.into_kind_and_id();
        let row = AuditEventRow {
            id: row_id,
            // prev_hash is overwritten by storage to maintain the ADR-0009 hash chain.
            prev_hash: audit_prev_hash_seed().to_owned(),
            caddy_instance_id: self.caddy_instance_id.clone(),
            correlation_id: append.correlation_id.to_string(),
            occurred_at: now_ms / 1_000,
            occurred_at_ms: now_ms,
            actor_kind,
            actor_id,
            kind: append.event.to_string(),
            target_kind: append.target_kind,
            target_id: append.target_id,
            snapshot_id: append.snapshot_id,
            redacted_diff_json,
            redaction_sites,
            outcome: append.outcome,
            error_kind: append.error_kind,
            notes: append.notes,
        };

        self.storage
            .record_audit_event(row)
            .await
            .map_err(AuditWriteError::Storage)
    }
}
