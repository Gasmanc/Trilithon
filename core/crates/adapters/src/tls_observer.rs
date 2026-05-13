//! [`TlsIssuanceObserver`] — background observer that polls Caddy for TLS
//! certificate issuance and emits follow-up audit rows (Slice 7.8).
//!
//! After a successful `config.applied` audit row is written, the applier
//! optionally spawns an observer task for any newly-introduced managed
//! hostnames.  The observer polls `GET /pki` (via [`CaddyClient::get_certificates`])
//! every five seconds for up to `self.timeout`.
//!
//! - When every requested hostname appears in the cert list: emit one
//!   `config.applied` row with `applied_state = "tls-issuing"`.
//! - On timeout: emit one `config.apply-failed` row with
//!   `error_kind = "TlsIssuanceTimeout"`.
//!
//! The observer never propagates errors to its caller.
//!
//! # Cross-references
//! ADR-0013, PRD H17 — §7.8.

use std::sync::Arc;
use std::time::Duration;

use ulid::Ulid;

use trilithon_core::{
    audit::AuditEvent,
    caddy::CaddyClient,
    reconciler::{AppliedStateTag, ApplyAuditNotes, ReloadKind},
    storage::types::{AuditOutcome, SnapshotId},
};

use crate::audit_notes::notes_to_string;
use crate::audit_writer::{ActorRef, AuditAppend, AuditWriter};

// ---------------------------------------------------------------------------
// Polling interval
// ---------------------------------------------------------------------------

/// How often the observer checks for certificate issuance.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// TlsIssuanceObserver
// ---------------------------------------------------------------------------

/// Background observer that polls Caddy for TLS certificate issuance status.
///
/// Spawn via `tokio::spawn(observer.observe(..))`.  The future never
/// returns an error to its caller; failures are recorded as audit rows.
pub struct TlsIssuanceObserver {
    /// HTTP client for the Caddy admin API.
    pub client: Arc<dyn CaddyClient>,
    /// Single entry point for writing to `audit_log`.
    pub audit: Arc<AuditWriter>,
    /// Maximum wall-clock time to wait for issuance before giving up.
    ///
    /// Default: 120 seconds.
    pub timeout: Duration,
}

impl TlsIssuanceObserver {
    /// Poll for TLS certificate issuance and emit a follow-up audit row.
    ///
    /// Spawning this via `tokio::spawn` is the intended usage; see module docs.
    /// The method never returns an error — all outcomes are written as audit rows.
    pub async fn observe(
        &self,
        correlation_id: Ulid,
        hostnames: Vec<String>,
        snapshot_id: Option<SnapshotId>,
    ) {
        // Nothing to observe if no hostnames were provided.
        if hostnames.is_empty() {
            return;
        }

        let deadline = tokio::time::Instant::now() + self.timeout;

        loop {
            // Check remaining time before sleeping.
            if tokio::time::Instant::now() >= deadline {
                self.emit_timeout(correlation_id, snapshot_id.as_ref())
                    .await;
                return;
            }

            // Poll for current certificates.
            match self.client.get_certificates().await {
                Ok(certs) => {
                    // Collect all names from all certificates.
                    let issued_names: std::collections::HashSet<String> =
                        certs.into_iter().flat_map(|c| c.names).collect();

                    // Check if every requested hostname is covered.
                    if hostnames.iter().all(|h| issued_names.contains(h)) {
                        self.emit_issued(correlation_id, snapshot_id.as_ref()).await;
                        return;
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        event = "tls_observer.poll_error",
                        correlation_id = %correlation_id,
                        error = %e,
                        "TlsIssuanceObserver: get_certificates failed; will retry"
                    );
                }
            }

            // Sleep until next poll or deadline, whichever is sooner.
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let sleep_for = POLL_INTERVAL.min(remaining);
            if sleep_for.is_zero() {
                self.emit_timeout(correlation_id, snapshot_id.as_ref())
                    .await;
                return;
            }
            tokio::time::sleep(sleep_for).await;
        }
    }

    /// Emit a `config.applied` row indicating TLS issuance has completed.
    async fn emit_issued(&self, correlation_id: Ulid, snapshot_id: Option<&SnapshotId>) {
        let notes = ApplyAuditNotes {
            reload_kind: ReloadKind::Graceful {
                drain_window_ms: None,
            },
            applied_state: AppliedStateTag::TlsIssuing,
            drain_window_ms: None,
            error_kind: None,
            error_detail: None,
            caddy_status: None,
            stale_version: None,
            current_version: None,
        };
        let _ = self
            .audit
            .record(AuditAppend {
                correlation_id,
                actor: ActorRef::System {
                    component: "tls-observer".to_owned(),
                },
                event: AuditEvent::ApplySucceeded,
                target_kind: None,
                target_id: None,
                snapshot_id: snapshot_id.cloned(),
                diff: None,
                outcome: AuditOutcome::Ok,
                error_kind: None,
                notes: Some(notes_to_string(&notes)),
            })
            .await;
        tracing::info!(
            event = "tls_observer.issued",
            correlation_id = %correlation_id,
            "TLS certificate issuance confirmed"
        );
    }

    /// Emit a `config.apply-failed` row when issuance did not complete within
    /// the timeout window.
    async fn emit_timeout(&self, correlation_id: Ulid, snapshot_id: Option<&SnapshotId>) {
        let notes = ApplyAuditNotes {
            reload_kind: ReloadKind::Graceful {
                drain_window_ms: None,
            },
            applied_state: AppliedStateTag::TlsIssuing,
            drain_window_ms: None,
            error_kind: Some("TlsIssuanceTimeout".to_owned()),
            error_detail: Some(
                "TLS certificate issuance did not complete within timeout".to_owned(),
            ),
            caddy_status: None,
            stale_version: None,
            current_version: None,
        };
        let _ = self
            .audit
            .record(AuditAppend {
                correlation_id,
                actor: ActorRef::System {
                    component: "tls-observer".to_owned(),
                },
                event: AuditEvent::ApplyFailed,
                target_kind: None,
                target_id: None,
                snapshot_id: snapshot_id.cloned(),
                diff: None,
                outcome: AuditOutcome::Error,
                error_kind: Some("TlsIssuanceTimeout".to_owned()),
                notes: Some(notes_to_string(&notes)),
            })
            .await;
        tracing::warn!(
            event = "tls_observer.timeout",
            correlation_id = %correlation_id,
            "TLS certificate issuance timed out"
        );
    }
}
