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

use trilithon_core::caddy::client::CaddyClient;
use trilithon_core::diff::{DiffEngine, DriftEvent, summarise_diff};
use trilithon_core::model::desired_state::DesiredState;
use trilithon_core::storage::Storage;

use crate::audit_writer::AuditWriter;
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

/// The drift-detection scheduler.
///
/// Shared via `Arc` — [`Self::run`] takes `Arc<Self>`.
pub struct DriftDetector {
    /// Scheduler configuration.
    pub config: DriftDetectorConfig,
    /// Caddy admin API client.
    pub client: Arc<dyn CaddyClient>,
    /// Structural diff engine.
    pub diff_engine: Arc<dyn DiffEngine>,
    /// Persistent storage.
    pub storage: Arc<dyn Storage>,
    /// Audit log writer (Slice 8.6 will use this for drift audit rows).
    pub audit: Arc<AuditWriter>,
    /// Mutex shared with the config-apply path; `try_lock` detects in-flight applies.
    pub apply_mutex: Arc<tokio::sync::Mutex<()>>,
}

impl DriftDetector {
    /// Run the drift-detection loop until `shutdown` fires.
    ///
    /// Ticks once immediately at startup, then at [`DriftDetectorConfig::interval`].
    pub async fn run(self: Arc<Self>, mut shutdown: watch::Receiver<bool>) {
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

        // Fetch the running Caddy config.
        let running_config = self
            .client
            .get_running_config()
            .await
            .map_err(|e| TickError::CaddyFetch(e.to_string()))?;

        // Parse running config into DesiredState for comparison.
        let running: DesiredState = serde_json::from_value(running_config.0.clone())
            .map_err(|e| TickError::Serialisation(e.to_string()))?;

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

        // Compute structural diff.
        let diff = self.diff_engine.structural_diff(&desired, &running)?;

        if diff.entries.is_empty() {
            return Ok(TickOutcome::Clean);
        }

        // Compute running-state hash (SHA-256 of canonical JSON).
        let running_canonical = trilithon_core::canonical_json::to_canonical_bytes(&running)
            .map_err(|e| TickError::Serialisation(e.to_string()))?;
        let running_state_hash = {
            use sha2::{Digest, Sha256};
            let hash = Sha256::digest(&running_canonical);
            format!("{hash:x}")
        };

        // Build diff summary.
        let diff_summary = summarise_diff(&diff);

        // Redacted diff JSON — for now, store canonical serialisation of the diff.
        // Phase 6 redactor integration is deferred to Slice 8.6.
        let redacted_diff_json =
            serde_json::to_string(&diff).map_err(|e| TickError::Serialisation(e.to_string()))?;

        let event = DriftEvent {
            before_snapshot_id: snapshot.snapshot_id,
            running_state_hash,
            diff_summary,
            detected_at: time::OffsetDateTime::now_utc().unix_timestamp(),
            correlation_id: Ulid::new(),
            redacted_diff_json,
            redaction_sites: 0,
        };

        Ok(TickOutcome::Drifted { event })
    }
}
