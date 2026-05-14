//! `POST /api/v1/mutations` — apply a typed mutation with an `expected_version` envelope.
//!
//! # Algorithm
//!
//! 1. Generate a ULID correlation id.
//! 2. If `env.expected_version` is absent → write `mutation.rejected.missing-expected-version`
//!    audit row → return 400.
//! 3. Deserialise `env.body` into `Mutation`. On parse failure → write
//!    `mutation.rejected` audit row with `error_kind = "schema"` → return 422.
//! 4. Load the current desired state snapshot from storage.
//! 5. Apply the mutation (pure; no I/O).
//! 6. Build a [`Snapshot`] from the new desired state and insert it.
//! 7. Call `applier.apply(&snapshot, expected_version)`.
//! 8. Branch on outcome; write audit rows accordingly.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ulid::Ulid;

use trilithon_core::caddy::capabilities::CapabilitySet;
use trilithon_core::canonical_json::{
    CANONICAL_JSON_VERSION, content_address_bytes, to_canonical_bytes,
};
use trilithon_core::model::desired_state::DesiredState;
use trilithon_core::mutation::{Mutation, apply_mutation};
use trilithon_core::reconciler::ApplyOutcome;
use trilithon_core::storage::types::{Snapshot, SnapshotId};
use trilithon_core::{audit::AuditEvent, storage::types::AuditOutcome};

use crate::audit_writer::{ActorRef, AuditAppend};
use crate::http_axum::AppState;
use crate::http_axum::auth_middleware::AuthenticatedSession;

// ── Wire types ────────────────────────────────────────────────────────────────

/// Outer envelope accepted by `POST /api/v1/mutations`.
#[derive(Deserialize)]
pub struct MutationEnvelope {
    /// Optimistic-concurrency guard. Absence → 400.
    pub expected_version: Option<i64>,
    /// The typed mutation payload.
    pub body: Value,
}

/// Success response body.
#[derive(Serialize)]
pub struct MutationResponse {
    /// Content-addressed snapshot id of the snapshot that was applied.
    pub snapshot_id: String,
    /// Monotonically increasing config version of the applied snapshot.
    pub config_version: i64,
}

/// 409 Conflict response body.
#[derive(Serialize)]
pub struct MutationConflictBody {
    /// Always `"conflict"`.
    pub code: &'static str,
    /// The version currently stored.
    pub current_version: i64,
    /// The version the caller expected (stale).
    pub expected_version: i64,
}

// ── Error type ────────────────────────────────────────────────────────────────

enum MutationHandlerError {
    MissingExpectedVersion,
    SchemaError(serde_json::Error),
    Internal(String),
    Conflict {
        stale_version: i64,
        current_version: i64,
    },
    ApplyFailed(String),
    LockContested,
}

impl IntoResponse for MutationHandlerError {
    #[allow(clippy::disallowed_methods)]
    // reason: serde_json::json! macro uses unwrap internally; this is acceptable in HTTP response shaping
    fn into_response(self) -> Response {
        match self {
            Self::MissingExpectedVersion => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"code": "missing-expected-version"})),
            )
                .into_response(),
            Self::SchemaError(e) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({"code": "schema-error", "detail": e.to_string()})),
            )
                .into_response(),
            Self::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"code": "internal", "detail": msg})),
            )
                .into_response(),
            Self::Conflict {
                stale_version,
                current_version,
            } => (
                StatusCode::CONFLICT,
                Json(MutationConflictBody {
                    code: "conflict",
                    current_version,
                    expected_version: stale_version,
                }),
            )
                .into_response(),
            Self::ApplyFailed(detail) => (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"code": "apply-failed", "detail": detail})),
            )
                .into_response(),
            Self::LockContested => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"code": "lock-contested"})),
            )
                .into_response(),
        }
    }
}

// ── Handler ───────────────────────────────────────────────────────────────────

/// Handler for `POST /api/v1/mutations`.
///
/// Accepts any [`Mutation`] variant wrapped in a [`MutationEnvelope`] with an
/// `expected_version` field. Authenticates, deserialises, applies, and persists
/// the resulting snapshot.
///
/// # Errors
///
/// Returns an HTTP error response on:
/// - 400 — `expected_version` absent from the envelope.
/// - 401 — no valid auth context (enforced by middleware before this handler).
/// - 409 — optimistic-concurrency conflict.
/// - 422 — `body` cannot be deserialised as a valid [`Mutation`].
/// - 502 — the applier rejected the configuration.
/// - 503 — the apply lock is contested.
pub async fn post_mutation(
    State(state): State<Arc<AppState>>,
    session: AuthenticatedSession,
    Json(env): Json<MutationEnvelope>,
) -> Result<Json<MutationResponse>, Response> {
    let correlation_id = Ulid::new();
    let actor = actor_from_session(&session);

    let expected_version = require_expected_version(&state, correlation_id, actor.clone(), &env)
        .await
        .map_err(IntoResponse::into_response)?;

    let mutation = parse_mutation(&state, correlation_id, actor.clone(), env.body)
        .await
        .map_err(IntoResponse::into_response)?;

    let snapshot = build_snapshot(
        &state,
        &session,
        correlation_id,
        &mutation,
        expected_version,
    )
    .await?;

    state
        .storage
        .insert_snapshot(snapshot.clone())
        .await
        .map_err(|e| MutationHandlerError::Internal(e.to_string()).into_response())?;

    apply_and_respond(&state, correlation_id, actor, &snapshot, expected_version).await
}

// ── Sub-steps ─────────────────────────────────────────────────────────────────

/// Step 2 — validate that `expected_version` is present.
async fn require_expected_version(
    state: &AppState,
    correlation_id: Ulid,
    actor: ActorRef,
    env: &MutationEnvelope,
) -> Result<i64, MutationHandlerError> {
    if let Some(v) = env.expected_version {
        return Ok(v);
    }
    let _ = state
        .audit_writer
        .record(AuditAppend {
            correlation_id,
            actor,
            event: AuditEvent::MutationRejectedMissingExpectedVersion,
            target_kind: None,
            target_id: None,
            snapshot_id: None,
            diff: None,
            outcome: AuditOutcome::Error,
            error_kind: Some("missing-expected-version".to_owned()),
            notes: None,
        })
        .await;
    Err(MutationHandlerError::MissingExpectedVersion)
}

/// Step 3 — deserialise `body` into a typed [`Mutation`].
async fn parse_mutation(
    state: &AppState,
    correlation_id: Ulid,
    actor: ActorRef,
    body: Value,
) -> Result<Mutation, MutationHandlerError> {
    match serde_json::from_value(body) {
        Ok(m) => Ok(m),
        Err(e) => {
            let _ = state
                .audit_writer
                .record(AuditAppend {
                    correlation_id,
                    actor,
                    event: AuditEvent::MutationRejected,
                    target_kind: None,
                    target_id: None,
                    snapshot_id: None,
                    diff: None,
                    outcome: AuditOutcome::Error,
                    error_kind: Some("schema".to_owned()),
                    notes: Some(e.to_string()),
                })
                .await;
            Err(MutationHandlerError::SchemaError(e))
        }
    }
}

/// Steps 4 + 5 + 6 — load state, apply mutation, build and return [`Snapshot`].
async fn build_snapshot(
    state: &AppState,
    session: &AuthenticatedSession,
    correlation_id: Ulid,
    mutation: &Mutation,
    _expected_version: i64,
) -> Result<Snapshot, Response> {
    // Step 4: load current desired state.
    let current_snapshot = state
        .storage
        .latest_desired_state()
        .await
        .map_err(|e| MutationHandlerError::Internal(e.to_string()).into_response())?;

    let current_state = match current_snapshot {
        Some(ref snap) => serde_json::from_str::<DesiredState>(&snap.desired_state_json)
            .map_err(|e| MutationHandlerError::Internal(e.to_string()).into_response())?,
        None => DesiredState::default(),
    };
    let parent_id = current_snapshot.as_ref().map(|s| s.snapshot_id.clone());

    // Step 5: apply mutation (pure).
    let capabilities = CapabilitySet {
        loaded_modules: BTreeSet::new(),
        caddy_version: String::new(),
        probed_at: 0,
    };
    let outcome = apply_mutation(&current_state, mutation, &capabilities).map_err(|e| {
        use trilithon_core::mutation::MutationError as ME;
        match e {
            ME::Conflict {
                observed_version,
                expected_version: ev,
            } => MutationHandlerError::Conflict {
                stale_version: ev,
                current_version: observed_version,
            }
            .into_response(),
            other => MutationHandlerError::Internal(other.to_string()).into_response(),
        }
    })?;

    let new_state = outcome.new_state;

    // Step 6: build snapshot.
    let canonical_bytes = to_canonical_bytes(&new_state)
        .map_err(|e| MutationHandlerError::Internal(e.to_string()).into_response())?;
    let hash = content_address_bytes(&canonical_bytes);
    let snapshot_id = SnapshotId(hash);

    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    // reason: unix seconds fit in i64 for thousands of years; now_ms fits in u64 for thousands of years
    let (now_secs_i64, now_ms_u64) = (now_unix.as_secs() as i64, now_unix.as_millis() as u64);

    let desired_state_json = String::from_utf8(canonical_bytes)
        .map_err(|e| MutationHandlerError::Internal(e.to_string()).into_response())?;

    Ok(Snapshot {
        snapshot_id,
        parent_id,
        config_version: new_state.version,
        actor: actor_label(session),
        intent: format!("{:?}", mutation.kind()),
        correlation_id: correlation_id.to_string(),
        caddy_version: String::new(),
        trilithon_version: env!("CARGO_PKG_VERSION").to_owned(),
        created_at_unix_seconds: now_secs_i64,
        created_at_monotonic_nanos: now_ms_u64 * 1_000_000,
        canonical_json_version: CANONICAL_JSON_VERSION,
        desired_state_json,
    })
}

/// Step 7 + 8 — call the applier and map the outcome to a response.
async fn apply_and_respond(
    state: &AppState,
    correlation_id: Ulid,
    actor: ActorRef,
    snapshot: &Snapshot,
    expected_version: i64,
) -> Result<Json<MutationResponse>, Response> {
    let apply_result = state
        .applier
        .apply(snapshot, expected_version)
        .await
        .map_err(|e| {
            use trilithon_core::reconciler::ApplyError;
            match e {
                ApplyError::OptimisticConflict {
                    observed_version,
                    expected_version: ev,
                } => MutationHandlerError::Conflict {
                    stale_version: ev,
                    current_version: observed_version,
                }
                .into_response(),
                ApplyError::LockContested { .. } => {
                    MutationHandlerError::LockContested.into_response()
                }
                other => MutationHandlerError::Internal(other.to_string()).into_response(),
            }
        })?;

    match apply_result {
        ApplyOutcome::Succeeded {
            snapshot_id: applied_id,
            config_version,
            ..
        } => {
            let _ = state
                .audit_writer
                .record(AuditAppend {
                    correlation_id,
                    actor,
                    event: AuditEvent::MutationApplied,
                    target_kind: None,
                    target_id: None,
                    snapshot_id: Some(applied_id.clone()),
                    diff: None,
                    outcome: AuditOutcome::Ok,
                    error_kind: None,
                    notes: None,
                })
                .await;
            Ok(Json(MutationResponse {
                snapshot_id: applied_id.0,
                config_version,
            }))
        }
        ApplyOutcome::Failed { detail, .. } => {
            let _ = state
                .audit_writer
                .record(AuditAppend {
                    correlation_id,
                    actor,
                    event: AuditEvent::MutationRejected,
                    target_kind: None,
                    target_id: None,
                    snapshot_id: None,
                    diff: None,
                    outcome: AuditOutcome::Error,
                    error_kind: Some("apply-failed".to_owned()),
                    notes: Some(detail.clone()),
                })
                .await;
            Err(MutationHandlerError::ApplyFailed(detail).into_response())
        }
        ApplyOutcome::Conflicted {
            stale_version,
            current_version,
        } => Err(MutationHandlerError::Conflict {
            stale_version,
            current_version,
        }
        .into_response()),
    }
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

fn actor_label(session: &AuthenticatedSession) -> String {
    use crate::http_axum::auth_middleware::AuthContext;
    match &session.0 {
        AuthContext::Session { user_id, .. } => user_id.clone(),
        AuthContext::Token { token_id, .. } => token_id.clone(),
    }
}
