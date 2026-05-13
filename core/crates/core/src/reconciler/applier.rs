//! Apply-outcome types and the `Applier` trait consumed by the HTTP layer
//! (Phase 9) and produced by the applier (Phase 7+).
//!
//! Type-only definitions live here; the concrete implementation lives in
//! `crates/adapters/src/applier_caddy.rs`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::storage::types::{Snapshot, SnapshotId};

// ---------------------------------------------------------------------------
// Apply outcome
// ---------------------------------------------------------------------------

/// The outcome of a single apply attempt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ApplyOutcome {
    /// The configuration was successfully loaded by Caddy.
    Succeeded {
        /// Snapshot that was applied.
        snapshot_id: SnapshotId,
        /// Monotonically increasing config version of the applied snapshot.
        config_version: i64,
        /// Whether the config is fully applied or a TLS issuance is pending.
        applied_state: AppliedState,
        /// How Caddy was asked to reload the configuration.
        reload_kind: ReloadKind,
        /// Wall-clock time spent on the apply call, in milliseconds.
        latency_ms: u32,
    },
    /// The apply attempt failed.
    Failed {
        /// Snapshot that was being applied.
        snapshot_id: SnapshotId,
        /// Broad classification of the failure.
        kind: ApplyFailureKind,
        /// Human-readable detail string for logging and audit.
        detail: String,
    },
    /// An optimistic-concurrency conflict was detected; no write was made.
    Conflicted {
        /// The version the caller observed (stale).
        stale_version: i64,
        /// The version currently stored in the database.
        current_version: i64,
    },
}

// ---------------------------------------------------------------------------
// Applied state discriminator
// ---------------------------------------------------------------------------

/// Discriminates between a fully-applied configuration and one where TLS
/// certificate issuance is still in progress.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AppliedState {
    /// The configuration is loaded; certificates are not necessarily issued.
    Applied,
    /// Reserved: a follow-up observation MAY upgrade the audit metadata once
    /// certificates have been issued.
    TlsIssuing {
        /// Hostnames for which TLS issuance is pending.
        hostnames: Vec<String>,
    },
}

// ---------------------------------------------------------------------------
// Reload kind
// ---------------------------------------------------------------------------

/// How the Caddy process was asked to reload its configuration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReloadKind {
    /// A graceful reload; in-flight requests drain before the old worker exits.
    Graceful {
        /// Optional drain window in milliseconds.  `None` means the Caddy
        /// default is used.  Slice 7.7 populates this field.
        drain_window_ms: Option<u32>,
    },
    /// An abrupt reload; connections are closed immediately.
    Abrupt,
}

// ---------------------------------------------------------------------------
// Failure kind
// ---------------------------------------------------------------------------

/// Broad classification of an apply failure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApplyFailureKind {
    /// Caddy rejected the configuration with a 400 response.
    CaddyValidation,
    /// Caddy returned a 5xx server error.
    CaddyServerError,
    /// Caddy could not be reached.
    CaddyUnreachable,
    /// A required Caddy module is not loaded.
    CapabilityMismatch {
        /// The module name that was missing.
        missing_module: String,
    },
    /// The ownership sentinel stored in Caddy did not match what was expected.
    OwnershipSentinelConflict {
        /// The value observed in Caddy (`None` means the sentinel was absent).
        observed: Option<String>,
    },
    /// The renderer returned an error before any Caddy request was made.
    Renderer,
}

// ---------------------------------------------------------------------------
// Apply error
// ---------------------------------------------------------------------------

/// Error type returned by the applier.
///
/// Callers convert this into an [`ApplyOutcome`] at the apply-call boundary;
/// the error type is also used for `?`-propagation inside the applier.
#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    /// Caddy returned a 4xx rejection for the configuration document.
    #[error("caddy rejected the load: {detail}")]
    CaddyRejected {
        /// Human-readable rejection detail from the Caddy response body.
        detail: String,
    },
    /// An optimistic-concurrency conflict was detected.
    #[error("optimistic conflict: observed {observed_version}, expected {expected_version}")]
    OptimisticConflict {
        /// Version found in the database.
        observed_version: i64,
        /// Version the caller expected.
        expected_version: i64,
    },
    /// A required Caddy module was not loaded at apply time.
    #[error("capability mismatch: module {module} not loaded at apply time")]
    CapabilityMismatch {
        /// The missing module name.
        module: String,
    },
    /// Caddy could not be reached over the admin socket.
    #[error("caddy unreachable: {detail}")]
    Unreachable {
        /// Human-readable detail (e.g. socket path or OS error).
        detail: String,
    },
    /// The ownership sentinel stored in Caddy did not match the expected value.
    #[error("ownership sentinel conflict (expected {expected}, observed {observed:?})")]
    OwnershipSentinelConflict {
        /// The value we expected to find.
        expected: String,
        /// The value actually observed (`None` = sentinel absent).
        observed: Option<String>,
    },
    /// A renderer error was surfaced before any Caddy call was attempted.
    #[error("renderer: {0}")]
    Renderer(#[from] super::render::RenderError),
    /// A storage-layer error was encountered during the apply sequence.
    #[error("storage: {0}")]
    Storage(String),
    /// The advisory apply lock for this instance is already held by another
    /// process; the caller should retry later.
    #[error("apply lock contested: held by pid {holder_pid}")]
    LockContested {
        /// PID of the process currently holding the lock.
        holder_pid: i32,
    },
}

// ---------------------------------------------------------------------------
// Validation report
// ---------------------------------------------------------------------------

/// A single validation failure returned by [`Applier::validate`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ValidationFailure {
    /// JSON pointer to the failing element within the desired state.
    pub path: String,
    /// Human-readable description of the failure.
    pub message: String,
}

/// Summary of a pre-apply validation pass.
///
/// Phase 12 populates this type; in earlier phases `failures` is always empty.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ValidationReport {
    /// Validation failures found; empty means the configuration is valid.
    pub failures: Vec<ValidationFailure>,
}

// ---------------------------------------------------------------------------
// Applier trait
// ---------------------------------------------------------------------------

/// The single path that drives Caddy from desired state to live state.
///
/// Implementations live in `adapters`; this trait is defined here so that
/// `core` can reference it without a cross-layer dependency.
#[async_trait]
pub trait Applier: Send + Sync + 'static {
    /// Apply `snapshot` to the running Caddy instance.
    ///
    /// `expected_version` is the config version the caller observed; the
    /// applier uses it for optimistic-concurrency checking (Slice 7.5).
    async fn apply(
        &self,
        snapshot: &Snapshot,
        expected_version: i64,
    ) -> Result<ApplyOutcome, ApplyError>;

    /// Validate `snapshot` without applying it.
    ///
    /// Phase 12 preflight populates the report; earlier phases return an
    /// empty report.
    async fn validate(&self, snapshot: &Snapshot) -> Result<ValidationReport, ApplyError>;

    /// Force a rollback to the snapshot identified by `target`.
    ///
    /// Retrieves the target snapshot from storage, then applies it via
    /// [`Self::apply`].
    async fn rollback(&self, target: &SnapshotId) -> Result<ApplyOutcome, ApplyError>;
}

// ---------------------------------------------------------------------------
// Audit notes
// ---------------------------------------------------------------------------

/// Flat tag for the applied-state dimension recorded in audit notes.
///
/// Unlike [`AppliedState`], this type has no payload so it round-trips through
/// serde without the tagged-enum envelope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppliedStateTag {
    /// The configuration is fully loaded.
    Applied,
    /// A TLS certificate issuance is still in progress.
    TlsIssuing,
}

/// Structured JSON payload written to the `notes` column of every terminal
/// apply audit row (`config.applied`, `config.apply-failed`,
/// `mutation.conflicted`).
///
/// Serialised via `trilithon_adapters::audit_notes::notes_to_string`, which
/// produces a key-sorted JSON object. This is NOT `canonical_json::to_canonical_bytes`
/// (that function accepts `&DesiredState`, not `&ApplyAuditNotes`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApplyAuditNotes {
    /// How Caddy was asked to reload the configuration.
    pub reload_kind: ReloadKind,
    /// Whether the configuration was fully applied or TLS issuance is pending.
    pub applied_state: AppliedStateTag,
    /// Drain window in milliseconds; only populated when `reload_kind` is
    /// `Graceful` and a non-default drain window was configured.
    pub drain_window_ms: Option<u32>,
    /// Machine-readable error classification, populated on failure paths.
    pub error_kind: Option<String>,
    /// Human-readable error detail, populated on failure paths.
    pub error_detail: Option<String>,
    /// HTTP status returned by Caddy on 4xx rejection; `None` on success.
    pub caddy_status: Option<u16>,
    /// Applied version at conflict time; populated only on `mutation.conflicted` rows.
    pub stale_version: Option<i64>,
    /// Observed current version at conflict time; populated only on `mutation.conflicted` rows.
    pub current_version: Option<i64>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    /// Constructs one value of every `ApplyOutcome` variant and verifies that
    /// serialising then deserialising returns the original value.
    #[test]
    fn apply_outcome_serde_round_trip() {
        let cases: Vec<ApplyOutcome> = vec![
            ApplyOutcome::Succeeded {
                snapshot_id: SnapshotId("a".repeat(64)),
                config_version: 42,
                applied_state: AppliedState::Applied,
                reload_kind: ReloadKind::Graceful {
                    drain_window_ms: Some(500),
                },
                latency_ms: 123,
            },
            ApplyOutcome::Succeeded {
                snapshot_id: SnapshotId("b".repeat(64)),
                config_version: 43,
                applied_state: AppliedState::TlsIssuing {
                    hostnames: vec!["example.com".to_owned()],
                },
                reload_kind: ReloadKind::Abrupt,
                latency_ms: 50,
            },
            ApplyOutcome::Failed {
                snapshot_id: SnapshotId("c".repeat(64)),
                kind: ApplyFailureKind::CaddyValidation,
                detail: "bad config".to_owned(),
            },
            ApplyOutcome::Failed {
                snapshot_id: SnapshotId("d".repeat(64)),
                kind: ApplyFailureKind::CaddyServerError,
                detail: "internal error".to_owned(),
            },
            ApplyOutcome::Failed {
                snapshot_id: SnapshotId("e".repeat(64)),
                kind: ApplyFailureKind::CaddyUnreachable,
                detail: "socket not found".to_owned(),
            },
            ApplyOutcome::Failed {
                snapshot_id: SnapshotId("f".repeat(64)),
                kind: ApplyFailureKind::CapabilityMismatch {
                    missing_module: "http.handlers.reverse_proxy".to_owned(),
                },
                detail: "module not loaded".to_owned(),
            },
            ApplyOutcome::Failed {
                snapshot_id: SnapshotId("0".repeat(64)),
                kind: ApplyFailureKind::OwnershipSentinelConflict {
                    observed: Some("other-owner".to_owned()),
                },
                detail: "sentinel mismatch".to_owned(),
            },
            ApplyOutcome::Failed {
                snapshot_id: SnapshotId("1".repeat(64)),
                kind: ApplyFailureKind::OwnershipSentinelConflict { observed: None },
                detail: "sentinel absent".to_owned(),
            },
            ApplyOutcome::Failed {
                snapshot_id: SnapshotId("2".repeat(64)),
                kind: ApplyFailureKind::Renderer,
                detail: "render failed".to_owned(),
            },
            ApplyOutcome::Conflicted {
                stale_version: 5,
                current_version: 6,
            },
        ];

        for outcome in &cases {
            let json = serde_json::to_string(outcome).expect("serialise");
            let restored: ApplyOutcome = serde_json::from_str(&json).expect("deserialise");
            assert_eq!(outcome, &restored, "round-trip failed for: {json}");
        }
    }

    /// Exhaustive match arm coverage over all `ApplyFailureKind` variants.
    ///
    /// The const list acts as a compile-time checklist: adding a new variant
    /// without updating this list will cause a compile error in the match.
    #[test]
    fn apply_failure_kind_exhaustive() {
        // Build one instance of every variant.
        let all_variants: &[ApplyFailureKind] = &[
            ApplyFailureKind::CaddyValidation,
            ApplyFailureKind::CaddyServerError,
            ApplyFailureKind::CaddyUnreachable,
            ApplyFailureKind::CapabilityMismatch {
                missing_module: "mod".to_owned(),
            },
            ApplyFailureKind::OwnershipSentinelConflict {
                observed: Some("x".to_owned()),
            },
            ApplyFailureKind::Renderer,
        ];

        // Ensure every variant is covered by a match arm so that new variants
        // added to the enum without updating this test will cause a compile
        // error.
        for kind in all_variants {
            let _covered = match kind {
                ApplyFailureKind::CaddyValidation => "caddy_validation",
                ApplyFailureKind::CaddyServerError => "caddy_server_error",
                ApplyFailureKind::CaddyUnreachable => "caddy_unreachable",
                ApplyFailureKind::CapabilityMismatch { .. } => "capability_mismatch",
                ApplyFailureKind::OwnershipSentinelConflict { .. } => "ownership_sentinel_conflict",
                ApplyFailureKind::Renderer => "renderer",
            };
        }

        // Every variant must also survive a serde round-trip.
        for kind in all_variants {
            let json = serde_json::to_string(kind).expect("serialise");
            let restored: ApplyFailureKind = serde_json::from_str(&json).expect("deserialise");
            assert_eq!(kind, &restored);
        }
    }
}
