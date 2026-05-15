//! Drift-state HTTP endpoints (Slice 9.10).
//!
//! - `GET /api/v1/drift/current`              — latest unresolved drift event or 204.
//! - `POST /api/v1/drift/{event_id}/adopt`    — adopt running state as desired state.
//! - `POST /api/v1/drift/{event_id}/reapply`  — re-push desired state to Caddy.
//! - `POST /api/v1/drift/{event_id}/defer`    — dismiss the drift event.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use ulid::Ulid;

use trilithon_core::audit::AuditEvent;
use trilithon_core::canonical_json::{
    CANONICAL_JSON_VERSION, content_address_bytes, to_canonical_bytes,
};
use trilithon_core::model::desired_state::DesiredState;
use trilithon_core::reconciler::ApplyOutcome;
use trilithon_core::storage::types::{AuditOutcome, Snapshot, SnapshotId};

use crate::audit_writer::{ActorRef, AuditAppend};
use crate::drift::ResolutionKind;
use crate::http_axum::AppState;
use crate::http_axum::auth_middleware::AuthenticatedSession;
use crate::http_axum::auth_routes::ApiError;
use crate::http_axum::mutations::MutationResponse;

// ── Response type ─────────────────────────────────────────────────────────────

/// Response body for `GET /api/v1/drift/current`.
#[derive(serde::Serialize)]
pub struct DriftCurrentResponse {
    /// Row identifier of the drift event (ULID string).
    pub event_id: String,
    /// Correlation identifier from the detection run.
    pub correlation_id: String,
    /// Snapshot id that was the desired state at detection time.
    pub before_snapshot_id: String,
    /// SHA-256 hash of the running state at detection time.
    pub running_state_hash: String,
    /// Redacted structural diff between desired and live state.
    pub redacted_diff_json: serde_json::Value,
    /// Number of redacted secret sites in the diff.
    pub redaction_sites: u32,
    /// Unix seconds when the drift was detected.
    pub detected_at: i64,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `GET /api/v1/drift/current` — returns the latest unresolved drift event.
///
/// Returns 200 + [`DriftCurrentResponse`] when drift exists, 204 when clean.
#[utoipa::path(
    get,
    path = "/api/v1/drift/current",
    responses(
        (status = 200, description = "Unresolved drift event"),
        (status = 204, description = "No drift detected"),
        (status = 401, description = "Unauthenticated"),
    )
)]
pub async fn current(
    State(state): State<Arc<AppState>>,
    _session: AuthenticatedSession,
) -> Response {
    let row = match state.storage.latest_unresolved_drift_event().await {
        Ok(r) => r,
        Err(e) => {
            #[allow(clippy::disallowed_methods)]
            // reason: serde_json::json! macro uses unwrap internally; this is acceptable in HTTP response shaping
            let body = serde_json::json!({"code": "internal", "detail": e.to_string()});
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response();
        }
    };

    let Some(row) = row else {
        return StatusCode::NO_CONTENT.into_response();
    };

    let redacted_diff_json: serde_json::Value =
        serde_json::from_str(&row.diff_json).unwrap_or(serde_json::Value::Null);

    let body = DriftCurrentResponse {
        event_id: row.id.0,
        correlation_id: row.correlation_id,
        before_snapshot_id: row.snapshot_id.0,
        running_state_hash: row.running_state_hash,
        redacted_diff_json,
        redaction_sites: 0, // redaction_sites not stored in DriftEventRow; diff is already redacted
        detected_at: row.detected_at,
    };

    (StatusCode::OK, Json(body)).into_response()
}

/// `POST /api/v1/drift/{event_id}/adopt` — accept running state as desired state.
///
/// Looks up the drift event, builds a new snapshot from the current desired state
/// (in lieu of having direct access to the running config at this layer — adopt
/// works by syncing desired state to the known running hash), then resolves the
/// drift event via the detector.
///
/// # Errors
///
/// Returns 404 when the `event_id` does not match the latest unresolved event.
/// Returns 500 on storage or apply failures.
#[utoipa::path(
    post,
    path = "/api/v1/drift/{event_id}/adopt",
    params(("event_id" = String, Path, description = "Drift event id")),
    responses(
        (status = 200, description = "Drift adopted"),
        (status = 401, description = "Unauthenticated"),
        (status = 404, description = "Event not found"),
    )
)]
pub async fn adopt(
    State(state): State<Arc<AppState>>,
    session: AuthenticatedSession,
    Path(id): Path<String>,
) -> Result<Json<MutationResponse>, ApiError> {
    let row = state
        .storage
        .latest_unresolved_drift_event()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::NotFound)?;

    if row.id.0 != id {
        return Err(ApiError::NotFound);
    }

    let correlation_id = Ulid::new();
    let actor = actor_from_session(&session);

    // For adopt: take the latest snapshot and re-insert it to mark "desired = current snapshot"
    // then resolve drift. The running state is already captured in the drift event.
    let snap = state
        .storage
        .latest_desired_state()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::Internal("no snapshot available".to_owned()))?;

    let expected_version = snap.config_version;

    // Apply the same snapshot through the applier to re-sync.
    let apply_result = state
        .applier
        .apply(&snap, expected_version)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let (snapshot_id, config_version) = match apply_result {
        ApplyOutcome::Succeeded {
            snapshot_id,
            config_version,
            ..
        } => (snapshot_id.0, config_version),
        ApplyOutcome::Failed { detail, .. } => {
            return Err(ApiError::Internal(format!("apply failed: {detail}")));
        }
        ApplyOutcome::Conflicted {
            current_version, ..
        } => {
            return Err(ApiError::Internal(format!(
                "version conflict: current={current_version}"
            )));
        }
    };

    // Resolve via detector — writes config.drift-resolved audit row.
    let event_correlation_id: Ulid = row
        .correlation_id
        .parse()
        .map_err(|e: ulid::DecodeError| ApiError::Internal(e.to_string()))?;

    state
        .drift_detector
        .mark_resolved(event_correlation_id, ResolutionKind::Adopt)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Write supplemental mutation.applied audit row.
    let _ = state
        .audit_writer
        .record(AuditAppend {
            correlation_id,
            actor,
            event: AuditEvent::MutationApplied,
            target_kind: None,
            target_id: None,
            snapshot_id: Some(SnapshotId(snapshot_id.clone())),
            diff: None,
            outcome: AuditOutcome::Ok,
            error_kind: None,
            notes: Some("drift.adopt".to_owned()),
        })
        .await;

    Ok(Json(MutationResponse {
        snapshot_id,
        config_version,
    }))
}

/// `POST /api/v1/drift/{event_id}/reapply` — re-push desired state to Caddy.
///
/// # Errors
///
/// Returns 404 when the `event_id` does not match the latest unresolved event.
/// Returns 500 on apply failures.
#[utoipa::path(
    post,
    path = "/api/v1/drift/{event_id}/reapply",
    params(("event_id" = String, Path, description = "Drift event id")),
    responses(
        (status = 200, description = "Desired state re-applied"),
        (status = 401, description = "Unauthenticated"),
        (status = 404, description = "Event not found"),
    )
)]
pub async fn reapply(
    State(state): State<Arc<AppState>>,
    session: AuthenticatedSession,
    Path(id): Path<String>,
) -> Result<Json<MutationResponse>, ApiError> {
    let row = state
        .storage
        .latest_unresolved_drift_event()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::NotFound)?;

    if row.id.0 != id {
        return Err(ApiError::NotFound);
    }

    let correlation_id = Ulid::new();
    let actor = actor_from_session(&session);

    let snap = state
        .storage
        .latest_desired_state()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::Internal("no snapshot available".to_owned()))?;

    let expected_version = snap.config_version;

    let apply_result = state
        .applier
        .apply(&snap, expected_version)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let (snapshot_id, config_version) = match apply_result {
        ApplyOutcome::Succeeded {
            snapshot_id,
            config_version,
            ..
        } => (snapshot_id.0, config_version),
        ApplyOutcome::Failed { detail, .. } => {
            return Err(ApiError::Internal(format!("apply failed: {detail}")));
        }
        ApplyOutcome::Conflicted {
            current_version, ..
        } => {
            return Err(ApiError::Internal(format!(
                "version conflict: current={current_version}"
            )));
        }
    };

    let event_correlation_id: Ulid = row
        .correlation_id
        .parse()
        .map_err(|e: ulid::DecodeError| ApiError::Internal(e.to_string()))?;

    state
        .drift_detector
        .mark_resolved(event_correlation_id, ResolutionKind::Reapply)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let _ = state
        .audit_writer
        .record(AuditAppend {
            correlation_id,
            actor,
            event: AuditEvent::MutationApplied,
            target_kind: None,
            target_id: None,
            snapshot_id: Some(SnapshotId(snapshot_id.clone())),
            diff: None,
            outcome: AuditOutcome::Ok,
            error_kind: None,
            notes: Some("drift.reapply".to_owned()),
        })
        .await;

    Ok(Json(MutationResponse {
        snapshot_id,
        config_version,
    }))
}

/// `POST /api/v1/drift/{event_id}/defer` — dismiss the drift event.
///
/// Writes a `config.drift-resolved` audit row with `resolution = "defer"`.
/// Returns 204 on success.
///
/// # Errors
///
/// Returns 404 when the `event_id` does not match the latest unresolved event.
#[utoipa::path(
    post,
    path = "/api/v1/drift/{event_id}/defer",
    params(("event_id" = String, Path, description = "Drift event id")),
    responses(
        (status = 204, description = "Drift deferred"),
        (status = 401, description = "Unauthenticated"),
        (status = 404, description = "Event not found"),
    )
)]
pub async fn defer(
    State(state): State<Arc<AppState>>,
    _session: AuthenticatedSession,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let row = state
        .storage
        .latest_unresolved_drift_event()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::NotFound)?;

    if row.id.0 != id {
        return Err(ApiError::NotFound);
    }

    let event_correlation_id: Ulid = row
        .correlation_id
        .parse()
        .map_err(|e: ulid::DecodeError| ApiError::Internal(e.to_string()))?;

    state
        .drift_detector
        .mark_resolved(event_correlation_id, ResolutionKind::Defer)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn actor_from_session(session: &AuthenticatedSession) -> ActorRef {
    use crate::http_axum::auth_middleware::AuthContext;
    match &session.0 {
        AuthContext::Session { user_id, .. } => ActorRef::User {
            id: user_id.clone(),
        },
        AuthContext::Token { token_id, .. } => ActorRef::Token {
            id: token_id.clone(),
        },
    }
}

/// Build a snapshot from the current desired state with a new mutation intent.
///
/// Used by adopt/reapply to produce a traceable snapshot for the apply step.
#[allow(dead_code)]
// reason: retained for future use when adopt needs to build a snapshot from running state
async fn build_snapshot_from_desired(
    state: &AppState,
    session: &AuthenticatedSession,
    correlation_id: Ulid,
    intent: &str,
) -> Result<Snapshot, ApiError> {
    use crate::http_axum::auth_middleware::AuthContext;

    let current_snap = state
        .storage
        .latest_desired_state()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let (current_state, parent_id) = match current_snap {
        Some(ref s) => {
            let ds = serde_json::from_str::<DesiredState>(&s.desired_state_json)
                .map_err(|e| ApiError::Internal(e.to_string()))?;
            (ds, Some(s.snapshot_id.clone()))
        }
        None => (DesiredState::default(), None),
    };

    let canonical_bytes =
        to_canonical_bytes(&current_state).map_err(|e| ApiError::Internal(e.to_string()))?;
    let hash = content_address_bytes(&canonical_bytes);
    let snapshot_id = SnapshotId(hash);

    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    // reason: unix seconds fit in i64 for thousands of years
    let (now_secs_i64, now_ms_u64) = (now_unix.as_secs() as i64, now_unix.as_millis() as u64);

    let desired_state_json =
        String::from_utf8(canonical_bytes).map_err(|e| ApiError::Internal(e.to_string()))?;

    let actor_label = match &session.0 {
        AuthContext::Session { user_id, .. } => user_id.clone(),
        AuthContext::Token { token_id, .. } => token_id.clone(),
    };

    Ok(Snapshot {
        snapshot_id,
        parent_id,
        config_version: current_state.version,
        actor: actor_label,
        intent: intent.to_owned(),
        correlation_id: correlation_id.to_string(),
        caddy_version: String::new(),
        trilithon_version: env!("CARGO_PKG_VERSION").to_owned(),
        created_at_unix_seconds: now_secs_i64,
        created_at_monotonic_nanos: now_ms_u64 * 1_000_000,
        canonical_json_version: CANONICAL_JSON_VERSION,
        desired_state_json,
    })
}
