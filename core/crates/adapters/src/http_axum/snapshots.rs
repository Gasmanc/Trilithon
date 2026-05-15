//! Snapshot read endpoints (Slice 9.8).
//!
//! - `GET /api/v1/snapshots` — paginated list
//! - `GET /api/v1/snapshots/{id}` — single snapshot
//! - `GET /api/v1/snapshots/{id}/diff/{other_id}` — redacted diff

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use trilithon_core::audit::redactor::SecretsRedactor;
use trilithon_core::model::desired_state::DesiredState;
use trilithon_core::storage::types::SnapshotId;

use crate::http_axum::AppState;
use crate::http_axum::auth_middleware::AuthenticatedSession;
use crate::http_axum::auth_routes::ApiError;

// ── Wire types ────────────────────────────────────────────────────────────────

/// Summary row returned by `GET /api/v1/snapshots`.
#[derive(Serialize)]
pub struct SnapshotSummary {
    /// Content-addressed snapshot identifier.
    pub id: String,
    /// Parent snapshot id, if any.
    pub parent_id: Option<String>,
    /// Monotonically increasing config version.
    pub config_version: i64,
    /// Creation time (Unix seconds).
    pub created_at: i64,
    /// Actor kind string (e.g. `"user"`, `"token"`).
    pub actor_kind: String,
    /// Actor identifier.
    pub actor_id: String,
    /// Human-readable mutation intent.
    pub intent: String,
}

/// Query parameters for `GET /api/v1/snapshots`.
#[derive(Deserialize)]
pub struct SnapshotListQuery {
    /// Maximum number of rows to return. Clamped to 200.
    pub limit: Option<u32>,
    /// Opaque pagination cursor — the `id` of the last seen snapshot.
    /// Only rows with `config_version` strictly less than the cursor
    /// snapshot's version are returned.
    pub cursor_after: Option<String>,
}

/// Response body for `GET /api/v1/snapshots/{id}/diff/{other_id}`.
#[derive(Serialize)]
pub struct SnapshotDiffResponse {
    /// The diff as a JSON value with secrets redacted.
    pub redacted_diff_json: Value,
    /// Number of secret fields that were redacted.
    pub redaction_sites: u32,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `GET /api/v1/snapshots` — paginated list of snapshots.
///
/// Returns snapshots in descending `config_version` order.  Limit is clamped
/// to 200; default is 50.  The cursor is the id of the last-seen snapshot;
/// only rows with a strictly lower `config_version` are returned.
///
/// # Errors
///
/// Returns `ApiError::Internal` on storage failure.
/// Returns `ApiError::NotFound` when the cursor snapshot id is unknown.
#[utoipa::path(
    get,
    path = "/api/v1/snapshots",
    responses(
        (status = 200, description = "Paginated snapshot list"),
        (status = 401, description = "Unauthenticated"),
    )
)]
pub async fn list_snapshots(
    State(state): State<Arc<AppState>>,
    _session: AuthenticatedSession,
    Query(q): Query<SnapshotListQuery>,
) -> Result<Json<Vec<SnapshotSummary>>, ApiError> {
    let limit = q.limit.unwrap_or(50).min(200);

    // Resolve cursor to a config_version bound.
    let cursor_before_version = if let Some(cursor_id) = q.cursor_after {
        let snap = state
            .storage
            .get_snapshot(&SnapshotId(cursor_id))
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .ok_or(ApiError::NotFound)?;
        Some(snap.config_version)
    } else {
        None
    };

    let snapshots = state
        .storage
        .list_snapshots(limit, cursor_before_version)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let summaries = snapshots
        .into_iter()
        .map(|s| SnapshotSummary {
            id: s.snapshot_id.0,
            parent_id: s.parent_id.map(|p| p.0),
            config_version: s.config_version,
            created_at: s.created_at_unix_seconds,
            // The storage layer stores actor_kind and actor_id merged into `actor`.
            // We use "user" as the kind and the full actor string as the id, matching
            // the write path in the mutations handler.
            actor_kind: "user".to_owned(),
            actor_id: s.actor,
            intent: s.intent,
        })
        .collect();

    Ok(Json(summaries))
}

/// `GET /api/v1/snapshots/{id}` — full snapshot JSON.
///
/// # Errors
///
/// Returns `ApiError::NotFound` when the id is unknown.
/// Returns `ApiError::Internal` on storage failure.
#[utoipa::path(
    get,
    path = "/api/v1/snapshots/{id}",
    params(("id" = String, Path, description = "Snapshot id")),
    responses(
        (status = 200, description = "Snapshot"),
        (status = 401, description = "Unauthenticated"),
        (status = 404, description = "Not found"),
    )
)]
pub async fn get_snapshot(
    State(state): State<Arc<AppState>>,
    _session: AuthenticatedSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let snap = state
        .storage
        .get_snapshot(&SnapshotId(id))
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::NotFound)?;

    let value = serde_json::to_value(&snap).map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(value))
}

/// `GET /api/v1/snapshots/{a}/diff/{b}` — redacted structural diff.
///
/// Loads both snapshots, deserialises their `desired_state_json`, computes a
/// structural diff, then redacts all secret fields.
///
/// # Errors
///
/// Returns `ApiError::NotFound` when either snapshot is unknown.
/// Returns `ApiError::Internal` on storage, diff, or redaction failure.
#[utoipa::path(
    get,
    path = "/api/v1/snapshots/{a}/diff/{b}",
    params(
        ("a" = String, Path, description = "Before snapshot id"),
        ("b" = String, Path, description = "After snapshot id"),
    ),
    responses(
        (status = 200, description = "Redacted diff"),
        (status = 401, description = "Unauthenticated"),
        (status = 404, description = "One or both snapshots not found"),
    )
)]
pub async fn diff_snapshots(
    State(state): State<Arc<AppState>>,
    _session: AuthenticatedSession,
    Path((a, b)): Path<(String, String)>,
) -> Result<Json<SnapshotDiffResponse>, ApiError> {
    let snap_a = state
        .storage
        .get_snapshot(&SnapshotId(a))
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::NotFound)?;

    let snap_b = state
        .storage
        .get_snapshot(&SnapshotId(b))
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::NotFound)?;

    let state_a: DesiredState = serde_json::from_str(&snap_a.desired_state_json)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let state_b: DesiredState = serde_json::from_str(&snap_b.desired_state_json)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let diff = state
        .diff_engine
        .structural_diff(&state_a, &state_b)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Serialise the diff to a Value so the redactor can walk it.
    let diff_value = serde_json::to_value(&diff).map_err(|e| ApiError::Internal(e.to_string()))?;

    // Build redactor using shared registry and hasher (borrowed for this call).
    let redactor = SecretsRedactor::new(&state.schema_registry, state.hasher.as_ref());
    let redaction_result = redactor
        .redact_diff(&diff_value)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(SnapshotDiffResponse {
        redacted_diff_json: redaction_result.value,
        redaction_sites: redaction_result.sites,
    }))
}
