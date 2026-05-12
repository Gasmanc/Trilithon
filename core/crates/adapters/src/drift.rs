//! `DriftDetector` — periodic scheduler that compares live Caddy state against
//! the latest desired-state snapshot (Slice 8.5).
//!
//! Runs as a long-lived tokio task: ticks once at startup, then every
//! `drift_check_interval_seconds`. Each tick fetches `GET /config/`, computes
//! the structural diff, and either records a drift event or returns silently.
//! A tick that overlaps an in-flight apply is skipped with a tracing event.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use ulid::Ulid;

use serde_json::Value;
use trilithon_core::audit::AuditEvent;
use trilithon_core::caddy::client::CaddyClient;
use trilithon_core::clock::Clock;
use trilithon_core::diff::{DriftEvent, diff_caddy_values, summarise_diff};
use trilithon_core::model::desired_state::DesiredState;
use trilithon_core::reconciler::CaddyJsonRenderer;
use trilithon_core::storage::Storage;
use trilithon_core::storage::types::{AuditOutcome, DriftEventRow, DriftResolution, DriftRowId};

use crate::audit_writer::{ActorRef, AuditAppend, AuditWriter};
use crate::tracing_correlation::with_correlation_span;

// ── Config ───────────────────────────────────────────────────────────────────

/// Configuration for the drift detection scheduler.
#[derive(Clone, Debug)]
pub struct DriftDetectorConfig {
    /// Interval between drift-detection ticks.
    pub interval: Duration,
    /// Caddy instance identifier (Phase 5 will make this dynamic).
    pub instance_id: String,
}

impl Default for DriftDetectorConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(60),
            instance_id: "local".into(),
        }
    }
}

/// Validation error for [`DriftDetectorConfig`].
#[derive(Debug, thiserror::Error)]
#[error("drift detector interval must be between 10 and 3600 seconds, got {0}s")]
pub struct ConfigValidationError(u64);

impl DriftDetectorConfig {
    /// Validate configuration constraints.
    ///
    /// # Errors
    ///
    /// Returns an error if `interval` is outside the `[10, 3600]` second range.
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        let secs = self.interval.as_secs();
        if !(10..=3600).contains(&secs) {
            return Err(ConfigValidationError(secs));
        }
        Ok(())
    }
}

// ── Outcome / Error ──────────────────────────────────────────────────────────

/// Result of a single drift-detection tick.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TickOutcome {
    /// The running state matches the desired state.
    Clean,
    /// Drift was detected between desired and running state.
    Drifted {
        /// The drift event details.
        event: DriftEvent,
    },
    /// An apply operation is currently in flight; this tick was skipped.
    SkippedApplyInFlight,
}

/// Errors that can occur during a single drift-detection tick.
#[derive(Debug, thiserror::Error)]
pub enum TickError {
    /// The Caddy admin API could not be reached or returned an error.
    #[error("caddy fetch failed: {0}")]
    CaddyFetch(String),
    /// The storage layer returned an error.
    #[error("storage: {0}")]
    Storage(String),
    /// The diff engine failed.
    #[error("diff: {0}")]
    Diff(#[from] trilithon_core::diff::DiffError),
    /// Serialisation of state failed.
    #[error("serialisation: {0}")]
    Serialisation(String),
}

// ── DriftDetector ────────────────────────────────────────────────────────────

/// How the operator resolved a detected drift.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResolutionKind {
    /// Accept the live state as the new desired state.
    Adopt,
    /// Re-apply the desired state to Caddy.
    Reapply,
    /// Defer action — revisit later.
    Defer,
}

/// The drift-detection scheduler.
///
/// Shared via `Arc` — [`Self::run`] takes `Arc<Self>`.
pub struct DriftDetector {
    /// Scheduler configuration.
    pub config: DriftDetectorConfig,
    /// Caddy admin API client.
    pub client: Arc<dyn CaddyClient>,
    /// Renderer: converts [`DesiredState`] → Caddy JSON for live-config comparison.
    pub renderer: Arc<dyn CaddyJsonRenderer>,
    /// Persistent storage.
    pub storage: Arc<dyn Storage>,
    /// Audit log writer.
    pub audit: Arc<AuditWriter>,
    /// Clock for timestamps.
    pub clock: Arc<dyn Clock>,
    /// Mutex shared with the config-apply path; `try_lock` detects in-flight applies.
    pub apply_mutex: Arc<tokio::sync::Mutex<()>>,
    /// Deduplication cache: the `running_state_hash` from the last successfully
    /// recorded drift event. Resets on resolution or daemon restart recovery.
    pub last_running_hash: tokio::sync::Mutex<Option<String>>,
}

impl DriftDetector {
    /// Run the drift-detection loop until `shutdown` fires.
    ///
    /// Ticks once immediately at startup, then at [`DriftDetectorConfig::interval`].
    pub async fn run(self: Arc<Self>, mut shutdown: watch::Receiver<bool>) {
        // Guard against a shutdown that was already signaled before we started.
        if *shutdown.borrow() {
            tracing::info!("drift-detector.shutdown");
            return;
        }

        let mut interval = tokio::time::interval(self.config.interval);
        // First tick fires immediately.
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                biased;
                _ = shutdown.changed() => {
                    tracing::info!("drift-detector.shutdown");
                    return;
                }
                _ = interval.tick() => {}
            }

            // Check shutdown state (changed() only fires on *transitions*).
            if *shutdown.borrow() {
                return;
            }

            let correlation_id = Ulid::new();
            let outcome =
                with_correlation_span(correlation_id, "system", "drift-detector", self.tick_once())
                    .await;

            match outcome {
                Ok(TickOutcome::Clean) => {
                    tracing::debug!("drift.clean");
                }
                Ok(TickOutcome::Drifted { ref event }) => {
                    tracing::warn!(
                        correlation_id = %event.correlation_id,
                        "drift.detected"
                    );
                    if let Err(e) = self.record(event.clone()).await {
                        tracing::error!(error = %e, "drift.record-failed");
                    }
                }
                Ok(TickOutcome::SkippedApplyInFlight) => {
                    // Already logged inside tick_once.
                }
                Err(e) => {
                    tracing::error!(error = %e, "drift.tick-error");
                }
            }
        }
    }

    /// Execute a single detection tick.
    ///
    /// Returns [`TickOutcome`] on success. The caller (the run loop) is
    /// responsible for audit persistence (Slice 8.6).
    ///
    /// # Errors
    ///
    /// Returns [`TickError`] when the Caddy API, storage, or diff engine fails.
    pub async fn tick_once(&self) -> Result<TickOutcome, TickError> {
        // Constraint 2: bind the guard to a named variable that lives for the
        // full duration of tick_once.
        let Ok(_apply_guard) = self.apply_mutex.try_lock() else {
            tracing::info!(target: "drift.skipped", reason = "apply-in-flight");
            return Ok(TickOutcome::SkippedApplyInFlight);
        };

        // Fetch the running Caddy config (raw Caddy JSON).
        let running_config = self
            .client
            .get_running_config()
            .await
            .map_err(|e| TickError::CaddyFetch(e.to_string()))?;

        // Fetch latest desired-state snapshot.
        let snapshot = self
            .storage
            .latest_desired_state()
            .await
            .map_err(|e| TickError::Storage(e.to_string()))?;

        let Some(snapshot) = snapshot else {
            // Running before bootstrap — nothing to compare against.
            return Ok(TickOutcome::Clean);
        };

        // Parse the stored desired state.
        let desired: DesiredState = serde_json::from_str(&snapshot.desired_state_json)
            .map_err(|e| TickError::Serialisation(e.to_string()))?;

        // Render desired state to Caddy JSON so both sides share the same schema.
        // Comparing DesiredState (Trilithon schema) against raw Caddy JSON directly
        // would produce a false diff because the schemas are different.
        let rendered_desired = self
            .renderer
            .render(&desired)
            .map_err(|e| TickError::Serialisation(e.to_string()))?;

        // Compute structural diff between rendered desired and live running config.
        let diff = diff_caddy_values(&rendered_desired, &running_config.0);

        if diff.entries.is_empty() {
            return Ok(TickOutcome::Clean);
        }

        // Compute running-state hash (SHA-256 of canonical Caddy JSON).
        let running_canonical = serde_json::to_vec(&running_config.0)
            .map_err(|e| TickError::Serialisation(e.to_string()))?;
        let running_state_hash = {
            use sha2::{Digest, Sha256};
            let hash = Sha256::digest(&running_canonical);
            format!("{hash:x}")
        };

        // Build diff summary.
        let diff_summary = summarise_diff(&diff);

        // Route diff JSON through the audit redactor so secrets in Caddy config
        // values (TLS keys, API tokens) are masked before persisting to drift_events.
        let diff_value =
            serde_json::to_value(&diff).map_err(|e| TickError::Serialisation(e.to_string()))?;
        let (redacted_diff_json, redaction_sites_count) = self
            .audit
            .redact_diff(&diff_value)
            .map_err(|e| TickError::Serialisation(e.to_string()))?;

        let event = DriftEvent {
            before_snapshot_id: snapshot.snapshot_id,
            running_state_hash,
            diff_summary,
            detected_at: self.clock.now_unix_ms() / 1_000,
            correlation_id: Ulid::new(),
            redacted_diff_json,
            redaction_sites: redaction_sites_count,
        };

        Ok(TickOutcome::Drifted { event })
    }

    /// Record a drift event, deduplicating against the previous tick's hash.
    ///
    /// Only updates the deduplication hash after both the audit row and the
    /// typed drift row have been successfully written.
    ///
    /// # Errors
    ///
    /// Returns [`TickError`] if either the audit write or the storage write fails.
    // zd:F010 expires:2026-11-01 reason: audit_log.record() uses BEGIN IMMEDIATE internally
    // for hash-chain integrity; nesting it inside a second transaction would deadlock on
    // SQLite. The two writes are therefore not atomic: a crash between them leaves an orphan
    // audit row with no matching drift_events row. Accepted trade-off — the dedup guard
    // (last_running_hash) prevents double-writes across restarts, and the audit chain's
    // own hash verification can detect the gap. Proper fix requires splitting record_audit_event
    // into a "prepare" + "commit" API so both rows can share one connection.
    #[allow(clippy::significant_drop_tightening)]
    // The guard is intentionally held across both writes — constraint 5 requires
    // the hash update only after both writes succeed.
    pub async fn record(&self, event: DriftEvent) -> Result<(), TickError> {
        let mut guard = self.last_running_hash.lock().await;

        // Deduplicate: same running hash means same drift cycle — skip.
        if guard.as_deref() == Some(&event.running_state_hash) {
            return Ok(());
        }

        // Step 3: Write the audit row.
        let diff_value: Value = serde_json::from_str(&event.redacted_diff_json)
            .map_err(|e| TickError::Serialisation(e.to_string()))?;

        let notes = serde_json::to_string(&event.diff_summary)
            .map_err(|e| TickError::Serialisation(e.to_string()))?;

        let append = AuditAppend {
            correlation_id: event.correlation_id,
            actor: ActorRef::System {
                component: "drift-detector".to_owned(),
            },
            event: AuditEvent::DriftDetected,
            target_kind: None,
            target_id: None,
            snapshot_id: Some(event.before_snapshot_id.clone()),
            diff: Some(diff_value),
            outcome: AuditOutcome::Ok,
            error_kind: None,
            notes: Some(notes),
        };

        self.audit
            .record(append)
            .await
            .map_err(|e| TickError::Storage(e.to_string()))?;

        // Step 4: Persist to the typed drift table.
        let drift_row = DriftEventRow {
            id: DriftRowId(Ulid::new().to_string()),
            correlation_id: event.correlation_id.to_string(),
            detected_at: event.detected_at,
            snapshot_id: event.before_snapshot_id,
            diff_json: event.redacted_diff_json,
            running_state_hash: event.running_state_hash.clone(),
            resolution: None,
            resolved_at: None,
        };

        self.storage
            .record_drift_event(drift_row)
            .await
            .map_err(|e| TickError::Storage(e.to_string()))?;

        // Step 5: Only update hash after both writes succeed.
        *guard = Some(event.running_state_hash);

        Ok(())
    }

    /// Mark a drift event as resolved, resetting the deduplication hash so that
    /// subsequent ticks re-evaluate.
    ///
    /// # Errors
    ///
    /// Returns [`TickError`] if the audit write or storage update fails.
    pub async fn mark_resolved(
        &self,
        correlation_id: Ulid,
        resolution: ResolutionKind,
    ) -> Result<(), TickError> {
        let storage_resolution = match resolution {
            ResolutionKind::Adopt => DriftResolution::Accepted,
            ResolutionKind::Reapply => DriftResolution::Reapplied,
            ResolutionKind::Defer => DriftResolution::Deferred,
        };

        let now_ms = self.clock.now_unix_ms();
        let now_secs = now_ms / 1_000;

        // Write the audit row.
        let mut notes_map = serde_json::Map::new();
        notes_map.insert(
            "resolution".to_owned(),
            serde_json::to_value(resolution)
                .map_err(|e| TickError::Serialisation(e.to_string()))?,
        );
        let notes = serde_json::Value::Object(notes_map).to_string();
        let append = AuditAppend {
            correlation_id,
            actor: ActorRef::System {
                component: "drift-detector".to_owned(),
            },
            event: AuditEvent::DriftResolved,
            target_kind: None,
            target_id: None,
            snapshot_id: None,
            diff: None,
            outcome: AuditOutcome::Ok,
            error_kind: None,
            notes: Some(notes),
        };

        self.audit
            .record(append)
            .await
            .map_err(|e| TickError::Storage(e.to_string()))?;

        // Update the drift table.
        self.storage
            .resolve_drift_event(&correlation_id.to_string(), storage_resolution, now_secs)
            .await
            .map_err(|e| TickError::Storage(e.to_string()))?;

        // Reset deduplication hash so the next tick re-evaluates.
        *self.last_running_hash.lock().await = None;

        Ok(())
    }

    /// Initialise the deduplication hash from storage on startup.
    ///
    /// Constraint 6: prevents duplicate detection rows across daemon restarts.
    ///
    /// # Errors
    ///
    /// Returns [`TickError`] if the storage query fails.
    pub async fn init_from_storage(&self) -> Result<(), TickError> {
        let existing = self
            .storage
            .latest_unresolved_drift_event()
            .await
            .map_err(|e| TickError::Storage(e.to_string()))?;

        if let Some(row) = existing {
            *self.last_running_hash.lock().await = Some(row.running_state_hash);
        }

        Ok(())
    }
}
