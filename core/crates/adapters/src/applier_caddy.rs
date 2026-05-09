//! [`CaddyApplier`] — the adapter that drives Caddy from desired state to live
//! state (Slices 7.4 + 7.5).
//!
//! # Apply algorithm (V1 — Slice 7.5)
//!
//! 0. CAS version check (Slice 7.5): open a `BEGIN IMMEDIATE` transaction,
//!    read the current `config_version` for this instance.
//!    - If `observed != expected_version`: write a `mutation.conflicted` audit
//!      row and return `ApplyOutcome::Conflicted { stale_version, current_version }`.
//!    - If versions match: proceed.
//! 1. Open an `apply.started` tracing span.
//! 2. Render the snapshot's desired state to a Caddy JSON document.
//! 3. Capability re-check: every module required by the desired state must be
//!    present in the live [`CapabilityCache`].
//! 4. Issue `POST /load` via [`CaddyClient::load_config`].
//!    - `CaddyError::Unreachable` → write `caddy.unreachable` audit row,
//!      return `Err(ApplyError::Unreachable { .. })`.
//!    - `CaddyError::BadStatus { status: 4xx }` → write `config.apply-failed`
//!      audit row, return `Ok(ApplyOutcome::Failed { .. })`.
//! 5. Fetch `GET /config/` and run a structural diff (§7.2 ignore list).
//!    A non-empty diff is a protocol violation; treat as
//!    `ApplyError::CaddyRejected`.
//! 6. Write a `config.applied` audit row.
//! 7. Emit `apply.succeeded` tracing event.
//! 8. Return `ApplyOutcome::Succeeded { .. }`.
//!
//! Advisory locks and TLS-state separation land in subsequent slices (7.6, 7.8).
//!
//! # Cross-references
//!
//! ADR-0002, ADR-0009, ADR-0012, ADR-0013 — PRD T1.1, T1.6, T1.7 — §7.1, §8.1.

use std::sync::Arc;

use async_trait::async_trait;
use ulid::Ulid;

use trilithon_core::caddy::{CaddyConfig, CaddyError};
use trilithon_core::storage::types::{AuditOutcome, Snapshot};
use trilithon_core::{
    audit::AuditEvent,
    caddy::CaddyClient,
    clock::Clock,
    diff::DiffEngine,
    model::desired_state::DesiredState,
    reconciler::{
        AppliedState, Applier, ApplyError, ApplyFailureKind, ApplyOutcome, CapabilityCheckError,
        ReloadKind, ValidationReport, capability_check::check_against_capability_set,
        render::CaddyJsonRenderer,
    },
    storage::{Storage, error::StorageError, types::SnapshotId},
};

use crate::{
    audit_writer::{ActorRef, AuditAppend, AuditWriter},
    caddy::cache::CapabilityCache,
};

// ---------------------------------------------------------------------------
// Maximum body excerpt kept in audit notes on 4xx failure
// ---------------------------------------------------------------------------

/// Maximum number of bytes from the Caddy response body kept in the
/// `config.apply-failed` audit note.  Bounded to prevent the audit log from
/// bloating on large error responses.
const EXCERPT_MAX_BYTES: usize = 512;

/// Truncate `s` to at most `EXCERPT_MAX_BYTES` bytes, appending `"…"` if it
/// was truncated.  The cut point is on a UTF-8 character boundary.
fn bounded_excerpt(s: &str) -> String {
    if s.len() <= EXCERPT_MAX_BYTES {
        s.to_owned()
    } else {
        // Find the last valid UTF-8 boundary at or before EXCERPT_MAX_BYTES.
        let mut end = EXCERPT_MAX_BYTES;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

// ---------------------------------------------------------------------------
// CaddyApplier
// ---------------------------------------------------------------------------

/// The production [`Applier`] implementation.
///
/// Holds shared references to the Caddy client, renderer, diff engine,
/// capability cache, audit writer, storage, and clock.  All fields are
/// clonable via `Arc` so the struct can be cheaply shared across tasks.
pub struct CaddyApplier {
    /// HTTP client for the Caddy admin API.
    pub client: Arc<dyn CaddyClient>,
    /// Renders a [`DesiredState`] to Caddy 2.x JSON.
    pub renderer: Arc<dyn CaddyJsonRenderer>,
    /// Compares desired and observed configs after a successful load.
    pub diff_engine: Arc<dyn DiffEngine>,
    /// Caches the most-recently probed capability set.
    pub capabilities: Arc<CapabilityCache>,
    /// Single entry point for writing to `audit_log`.
    pub audit: Arc<AuditWriter>,
    /// Persistent store for snapshot retrieval (used by `rollback`).
    pub storage: Arc<dyn Storage>,
    /// Identifies the Caddy instance; `"local"` in V1.
    pub instance_id: String,
    /// Wall-clock source; swap for a deterministic double in tests.
    pub clock: Arc<dyn Clock>,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

impl CaddyApplier {
    /// Deserialise `desired_state_json` from a snapshot, propagating parse
    /// errors as `ApplyError::Storage`.
    fn parse_desired_state(snapshot: &Snapshot) -> Result<DesiredState, ApplyError> {
        serde_json::from_str::<DesiredState>(&snapshot.desired_state_json)
            .map_err(|e| ApplyError::Storage(format!("deserialise desired_state_json: {e}")))
    }
}

// ---------------------------------------------------------------------------
// Internal apply helpers
// ---------------------------------------------------------------------------

impl CaddyApplier {
    /// Issue `POST /load` and handle transport / 4xx errors.
    ///
    /// Returns:
    /// - `Ok(None)` — load succeeded; caller continues with equivalence check.
    /// - `Ok(Some(outcome))` — 4xx rejection; caller should return this
    ///   `ApplyOutcome::Failed` directly.
    /// - `Err(e)` — transport or 5xx error; caller propagates.
    async fn load_or_fail(
        &self,
        caddy_config: CaddyConfig,
        correlation_id: Ulid,
        snapshot_id: &SnapshotId,
    ) -> Result<Option<ApplyOutcome>, ApplyError> {
        match self.client.load_config(caddy_config).await {
            Ok(()) => Ok(None),
            Err(CaddyError::Unreachable { detail }) => {
                let _ = self
                    .audit
                    .record(AuditAppend {
                        correlation_id,
                        actor: ActorRef::System {
                            component: "caddy-applier".to_owned(),
                        },
                        event: AuditEvent::CaddyUnreachable,
                        target_kind: None,
                        target_id: None,
                        snapshot_id: Some(snapshot_id.clone()),
                        diff: None,
                        outcome: AuditOutcome::Error,
                        error_kind: Some("CaddyUnreachable".to_owned()),
                        notes: Some(detail.clone()),
                    })
                    .await;
                Err(ApplyError::Unreachable { detail })
            }
            Err(CaddyError::Timeout { seconds }) => {
                let detail = format!("operation timed out after {seconds}s");
                let _ = self
                    .audit
                    .record(AuditAppend {
                        correlation_id,
                        actor: ActorRef::System {
                            component: "caddy-applier".to_owned(),
                        },
                        event: AuditEvent::CaddyUnreachable,
                        target_kind: None,
                        target_id: None,
                        snapshot_id: Some(snapshot_id.clone()),
                        diff: None,
                        outcome: AuditOutcome::Error,
                        error_kind: Some("CaddyUnreachable".to_owned()),
                        notes: Some(detail.clone()),
                    })
                    .await;
                Err(ApplyError::Unreachable { detail })
            }
            Err(CaddyError::BadStatus { status, body }) if status / 100 == 4 => {
                let excerpt = bounded_excerpt(&body);
                let _ = self
                    .audit
                    .record(AuditAppend {
                        correlation_id,
                        actor: ActorRef::System {
                            component: "caddy-applier".to_owned(),
                        },
                        event: AuditEvent::ApplyFailed,
                        target_kind: None,
                        target_id: None,
                        snapshot_id: Some(snapshot_id.clone()),
                        diff: None,
                        outcome: AuditOutcome::Error,
                        error_kind: Some("CaddyValidation".to_owned()),
                        notes: Some(excerpt.clone()),
                    })
                    .await;
                tracing::warn!(
                    event = "apply.failed",
                    correlation_id = %correlation_id,
                    snapshot.id = %snapshot_id.0,
                    status = status,
                    "caddy rejected load with 4xx"
                );
                Ok(Some(ApplyOutcome::Failed {
                    snapshot_id: snapshot_id.clone(),
                    kind: ApplyFailureKind::CaddyValidation,
                    detail: excerpt,
                }))
            }
            Err(other_err) => Err(ApplyError::Storage(other_err.to_string())),
        }
    }

    /// Fetch the running config from Caddy and assert structural equivalence.
    async fn verify_equivalence(&self, desired_state: &DesiredState) -> Result<(), ApplyError> {
        let observed =
            self.client
                .get_running_config()
                .await
                .map_err(|e| ApplyError::Unreachable {
                    detail: e.to_string(),
                })?;

        let diffs = self
            .diff_engine
            .structural_diff(desired_state, &observed)
            .map_err(|e| ApplyError::Storage(e.to_string()))?;

        if diffs.is_empty() {
            Ok(())
        } else {
            Err(ApplyError::CaddyRejected {
                detail: format!(
                    "post-load equivalence failed: {} path(s) differ: {}",
                    diffs.len(),
                    diffs.join(", ")
                ),
            })
        }
    }

    /// Write a `mutation.conflicted` audit row and return the typed outcome.
    ///
    /// Called when the CAS version check fails.  Returns `Ok(Conflicted { .. })`
    /// so the caller can propagate it directly from `apply`.
    async fn handle_conflict(
        &self,
        correlation_id: Ulid,
        snapshot_id: &SnapshotId,
        stale_version: i64,
        current_version: i64,
    ) -> Result<ApplyOutcome, ApplyError> {
        let _ = self
            .audit
            .record(AuditAppend {
                correlation_id,
                actor: ActorRef::System {
                    component: "caddy-applier".to_owned(),
                },
                event: AuditEvent::MutationConflicted,
                target_kind: None,
                target_id: None,
                snapshot_id: Some(snapshot_id.clone()),
                diff: None,
                outcome: AuditOutcome::Error,
                error_kind: Some("OptimisticConflict".to_owned()),
                notes: Some(format!(
                    "{{\"stale_version\":{stale_version},\"current_version\":{current_version}}}"
                )),
            })
            .await;
        tracing::warn!(
            event = "apply.conflicted",
            correlation_id = %correlation_id,
            snapshot.id = %snapshot_id.0,
            stale_version = stale_version,
            current_version = current_version,
            "optimistic conflict: stale expected_version"
        );
        Ok(ApplyOutcome::Conflicted {
            stale_version,
            current_version,
        })
    }

    /// Write a `config.applied` audit row after a successful load + equivalence
    /// check.
    async fn write_apply_succeeded_audit(&self, correlation_id: Ulid, snapshot_id: &SnapshotId) {
        let _ = self
            .audit
            .record(AuditAppend {
                correlation_id,
                actor: ActorRef::System {
                    component: "caddy-applier".to_owned(),
                },
                event: AuditEvent::ApplySucceeded,
                target_kind: None,
                target_id: None,
                snapshot_id: Some(snapshot_id.clone()),
                diff: None,
                outcome: AuditOutcome::Ok,
                error_kind: None,
                notes: Some(r#"{"reload_kind":"graceful","applied_state":"applied"}"#.to_owned()),
            })
            .await;
    }
}

// ---------------------------------------------------------------------------
// Applier impl
// ---------------------------------------------------------------------------

#[async_trait]
impl Applier for CaddyApplier {
    async fn apply(
        &self,
        snapshot: &Snapshot,
        expected_version: i64,
    ) -> Result<ApplyOutcome, ApplyError> {
        let snapshot_id = snapshot.snapshot_id.clone();
        let config_version = snapshot.config_version;

        let correlation_id: Ulid = snapshot
            .correlation_id
            .parse()
            .unwrap_or_else(|_| Ulid::new());

        // Step 0 (Slice 7.5): CAS version check — BEGIN IMMEDIATE in storage.
        // Prevents TOCTOU races between two concurrent apply() calls.
        match self
            .storage
            .cas_advance_config_version(&self.instance_id, expected_version, &snapshot_id)
            .await
        {
            Ok(_new_version) => {
                // CAS succeeded; proceed with apply.
            }
            Err(StorageError::OptimisticConflict { observed, expected }) => {
                return self
                    .handle_conflict(correlation_id, &snapshot_id, expected, observed)
                    .await;
            }
            Err(e) => {
                return Err(ApplyError::Storage(format!(
                    "cas_advance_config_version: {e}"
                )));
            }
        }

        // Step 1: emit apply.started tracing event (in_scope avoids !Send guard
        // crossing an await point).
        let start_ms = {
            let span = tracing::info_span!(
                "apply.started",
                correlation_id = %correlation_id,
                snapshot.id = %snapshot_id.0,
                snapshot.config_version = config_version,
            );
            span.in_scope(|| {
                tracing::info!(event = "apply.started", correlation_id = %correlation_id,
                    snapshot.id = %snapshot_id.0);
                self.clock.now_unix_ms()
            })
        };

        // Step 2: parse desired state.
        let desired_state = Self::parse_desired_state(snapshot)?;

        // Step 3: render to Caddy JSON.
        let caddy_config = CaddyConfig(self.renderer.render(&desired_state)?);

        // Step 4: capability re-check (skip if no cached snapshot yet).
        if let Some(caps) = self.capabilities.snapshot() {
            check_against_capability_set(&desired_state, &caps).map_err(|e| {
                let module = match e {
                    CapabilityCheckError::Missing { module, .. } => module,
                };
                ApplyError::CapabilityMismatch { module }
            })?;
        }

        // Step 5: POST /load — errors write audit rows and return early.
        if let Some(failed_outcome) = self
            .load_or_fail(caddy_config, correlation_id, &snapshot_id)
            .await?
        {
            return Ok(failed_outcome);
        }

        // Step 6: verify post-load structural equivalence.
        self.verify_equivalence(&desired_state).await?;

        // Step 7: write config.applied audit row.
        self.write_apply_succeeded_audit(correlation_id, &snapshot_id)
            .await;

        // Step 8: emit apply.succeeded tracing event.
        let latency_ms = {
            let elapsed = self.clock.now_unix_ms() - start_ms;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // reason: elapsed is always non-negative; 49-day overflow cannot occur
            {
                elapsed.unsigned_abs().min(u64::from(u32::MAX)) as u32
            }
        };
        tracing::info!(
            event = "apply.succeeded",
            correlation_id = %correlation_id,
            snapshot.id = %snapshot_id.0,
            latency_ms = latency_ms,
        );

        Ok(ApplyOutcome::Succeeded {
            snapshot_id,
            config_version,
            applied_state: AppliedState::Applied,
            reload_kind: ReloadKind::Graceful {
                drain_window_ms: None,
            },
            latency_ms,
        })
    }

    async fn validate(&self, _snapshot: &Snapshot) -> Result<ValidationReport, ApplyError> {
        // Phase 12 placeholder.
        Ok(ValidationReport::default())
    }

    async fn rollback(&self, target: &SnapshotId) -> Result<ApplyOutcome, ApplyError> {
        let snapshot = self
            .storage
            .get_snapshot(target)
            .await
            .map_err(|e| ApplyError::Storage(e.to_string()))?
            .ok_or_else(|| {
                ApplyError::Storage(format!("rollback target snapshot {target:?} not found"))
            })?;

        let expected = snapshot.config_version;
        self.apply(&snapshot, expected).await
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
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use super::bounded_excerpt;

    #[test]
    fn bounded_excerpt_short_passthrough() {
        let s = "hello";
        assert_eq!(bounded_excerpt(s), "hello");
    }

    #[test]
    fn bounded_excerpt_truncates_long_ascii() {
        let long = "x".repeat(600);
        let exc = bounded_excerpt(&long);
        assert!(exc.len() <= 520, "must be <= 512 chars + ellipsis overhead");
        assert!(exc.ends_with('…'), "must end with ellipsis marker");
    }

    #[test]
    fn bounded_excerpt_exact_boundary_passthrough() {
        let s = "a".repeat(512);
        assert_eq!(
            bounded_excerpt(&s),
            s,
            "exactly 512 bytes must pass through unchanged"
        );
    }
}
