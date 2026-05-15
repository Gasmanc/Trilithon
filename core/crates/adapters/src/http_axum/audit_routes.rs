//! Audit log endpoint (Slice 9.9).
//!
//! `GET /api/v1/audit` — paginated, filtered view of the Phase 6 audit log.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use trilithon_core::storage::audit_vocab::AUDIT_KINDS;
use trilithon_core::storage::types::{AuditRowId, AuditSelector, UnixSeconds};

use crate::http_axum::AppState;
use crate::http_axum::auth_middleware::AuthenticatedSession;
use crate::http_axum::auth_routes::ApiError;

// ── Wire types ────────────────────────────────────────────────────────────────

/// Query parameters for `GET /api/v1/audit`.
#[derive(Deserialize)]
pub struct AuditListQuery {
    /// Lower bound on `occurred_at` (inclusive), Unix seconds.
    pub since: Option<i64>,
    /// Upper bound on `occurred_at` (exclusive), Unix seconds.
    pub until: Option<i64>,
    /// Exact match on `actor_id`.
    pub actor_id: Option<String>,
    /// Exact match on `kind`; must be a known §6.6 vocabulary entry.
    pub event: Option<String>,
    /// Exact match on `correlation_id`.
    pub correlation_id: Option<String>,
    /// Maximum rows to return. Default 100; clamped to 1000.
    pub limit: Option<u32>,
    /// Cursor for descending pagination. Only rows with `id < cursor_before` are returned.
    pub cursor_before: Option<String>,
}

/// A single audit log row returned by `GET /api/v1/audit`.
#[derive(Serialize)]
pub struct AuditRowResponse {
    /// Row identifier (ULID).
    pub id: String,
    /// Correlation identifier.
    pub correlation_id: String,
    /// Event time, whole seconds.
    pub occurred_at: i64,
    /// Event time, millisecond precision.
    pub occurred_at_ms: i64,
    /// Kind of actor (`user`, `token`, `system`).
    pub actor_kind: String,
    /// Opaque actor identity string.
    pub actor_id: String,
    /// Event kind from the §6.6 vocabulary.
    pub event: String,
    /// Kind of entity that was the target, if applicable.
    pub target_kind: Option<String>,
    /// Identity of the target entity, if applicable.
    pub target_id: Option<String>,
    /// Associated snapshot id, if the event produced one.
    pub snapshot_id: Option<String>,
    /// Redacted diff payload, if the event involved a state change.
    pub redacted_diff_json: Option<serde_json::Value>,
    /// Number of secret fields redacted from the diff.
    pub redaction_sites: u32,
    /// Outcome of the operation (`ok`, `error`, `denied`).
    pub outcome: String,
    /// Machine-readable error kind, populated on error or denial.
    pub error_kind: Option<String>,
    /// Free-text notes for operator review.
    pub notes: Option<String>,
}

// ── Handler ───────────────────────────────────────────────────────────────────

/// `GET /api/v1/audit` — return audit rows in reverse chronological order.
///
/// Default page size is 100; maximum is 1000.  An unknown `event` filter
/// returns 400.
///
/// # Errors
///
/// Returns `ApiError::BadRequest` for an unknown event kind, or
/// `ApiError::Internal` on storage failure.
#[utoipa::path(
    get,
    path = "/api/v1/audit",
    responses(
        (status = 200, description = "Audit log entries"),
        (status = 400, description = "Unknown event kind"),
        (status = 401, description = "Unauthenticated"),
    )
)]
pub async fn list_audit(
    State(state): State<Arc<AppState>>,
    _session: AuthenticatedSession,
    Query(q): Query<AuditListQuery>,
) -> Result<Json<Vec<AuditRowResponse>>, ApiError> {
    // 1. Validate the event kind filter.
    if let Some(ref kind) = q.event {
        if !AUDIT_KINDS.contains(&kind.as_str()) {
            return Err(ApiError::BadRequest(format!(
                "unknown audit event kind: {kind}"
            )));
        }
    }

    // 2. Clamp limit.
    let limit = q.limit.unwrap_or(100).min(1000);

    // 3. Build selector.
    let selector = AuditSelector {
        kind_glob: q.event.clone(),
        actor_id: q.actor_id,
        correlation_id: q.correlation_id,
        since: q.since.map(|s| s as UnixSeconds),
        until: q.until.map(|u| u as UnixSeconds),
        cursor_before: q.cursor_before.map(AuditRowId),
    };

    // 4. Query storage.
    let rows = state
        .storage
        .tail_audit_log(selector, limit)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // 5. Map to response shape.
    let response: Vec<AuditRowResponse> = rows
        .into_iter()
        .map(|row| {
            let redacted_diff_json = row
                .redacted_diff_json
                .and_then(|s| serde_json::from_str(&s).ok());

            AuditRowResponse {
                id: row.id.0,
                correlation_id: row.correlation_id,
                occurred_at: row.occurred_at,
                occurred_at_ms: row.occurred_at_ms,
                actor_kind: row.actor_kind.as_audit_str().to_owned(),
                actor_id: row.actor_id,
                event: row.kind,
                target_kind: row.target_kind,
                target_id: row.target_id,
                snapshot_id: row.snapshot_id.map(|s| s.0),
                redacted_diff_json,
                redaction_sites: row.redaction_sites,
                outcome: row.outcome.as_audit_str().to_owned(),
                error_kind: row.error_kind,
                notes: row.notes,
            }
        })
        .collect();

    Ok(Json(response))
}
