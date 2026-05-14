//! Auth endpoint handlers: login, logout, and change-password (Slice 9.5).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::audit_writer::{ActorRef, AuditAppend, AuditWriteError};
use crate::auth::users::UserRole;
use crate::auth::{build_cookie, verify_password};
use crate::http_axum::AppState;
use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use serde::{Deserialize, Serialize};
use trilithon_core::audit::AuditEvent;
use trilithon_core::storage::types::AuditOutcome;

// ── Request / Response types ──────────────────────────────────────────────────

/// Request body for `POST /api/v1/auth/login`.
#[derive(Deserialize)]
pub struct LoginRequest {
    /// The account username.
    pub username: String,
    /// The account password.
    pub password: String,
}

/// Successful-login response body.
#[derive(Serialize)]
pub struct LoginResponse {
    /// ULID of the authenticated user.
    pub user_id: String,
    /// Role assigned to the user.
    pub role: UserRole,
    /// When `true` the user must change their password before using other endpoints.
    pub must_change_pw: bool,
    /// Monotonically increasing config version at the time of login.
    pub config_version: i64,
}

/// Request body for `POST /api/v1/auth/change-password`.
#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    /// The current password to verify before accepting the change.
    pub old_password: String,
    /// The desired new password; must be at least 12 characters and differ from the old one.
    pub new_password: String,
}

// ── Stub session type (filled in by Slice 9.6) ────────────────────────────────

/// Placeholder for the authenticated-session extractor introduced in Slice 9.6.
///
/// Handlers that require an authenticated session accept this type.  Slice 9.6
/// replaces this with a real `FromRequestParts` impl that validates the session
/// cookie against the session store.
///
/// This stub reads the session from `X-Session-Id` + `X-User-Id` request
/// headers.  It exists only to make the handlers compile; the real
/// implementation will use cookie-based lookup.
#[derive(Clone, Debug)]
pub struct AuthenticatedSession {
    /// The session token extracted from the cookie.
    pub session_id: String,
    /// The authenticated user id.
    pub user_id: String,
}

impl<S> axum::extract::FromRequestParts<S> for AuthenticatedSession
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let session_id = parts
            .headers
            .get("x-session-id")
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned)
            .ok_or(ApiError::Unauthorized)?;

        let user_id = parts
            .headers
            .get("x-user-id")
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned)
            .ok_or(ApiError::Unauthorized)?;

        Ok(Self {
            session_id,
            user_id,
        })
    }
}

// ── ApiError ──────────────────────────────────────────────────────────────────

/// HTTP API error type — maps to a status code and a JSON body.
#[derive(Debug)]
pub enum ApiError {
    /// 401 Unauthorized.
    Unauthorized,
    /// 409 Conflict with a machine-readable code.
    Conflict {
        /// Machine-readable error code returned in the response body.
        code: &'static str,
    },
    /// 429 Too Many Requests with a `Retry-After` value.
    RateLimited {
        /// Number of seconds the client should wait before retrying.
        retry_after_seconds: u32,
    },
    /// 400 Bad Request with a message.
    BadRequest(String),
    /// 500 Internal Server Error.
    Internal(String),
}

fn error_body(key: &str, value: &str) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    m.insert(key.to_owned(), serde_json::Value::String(value.to_owned()));
    serde_json::Value::Object(m)
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                Json(error_body("error", "unauthorized")),
            )
                .into_response(),
            Self::Conflict { code } => {
                (StatusCode::CONFLICT, Json(error_body("code", code))).into_response()
            }
            Self::RateLimited {
                retry_after_seconds,
            } => {
                let mut headers = HeaderMap::new();
                if let Ok(v) = HeaderValue::from_str(&retry_after_seconds.to_string()) {
                    headers.insert("Retry-After", v);
                }
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    headers,
                    Json(error_body("error", "rate limited")),
                )
                    .into_response()
            }
            Self::BadRequest(msg) => {
                (StatusCode::BAD_REQUEST, Json(error_body("error", &msg))).into_response()
            }
            Self::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(error_body("error", &msg)),
            )
                .into_response(),
        }
    }
}

impl From<AuditWriteError> for ApiError {
    fn from(e: AuditWriteError) -> Self {
        Self::Internal(e.to_string())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}

fn set_cookie_header(state: &AppState, session_id: &str) -> HeaderValue {
    build_cookie(
        &state.session_cookie_name,
        session_id,
        state.session_ttl_seconds,
        false, // loopback-only; Secure is appropriate in production TLS terminator
    )
}

fn clear_cookie_header(state: &AppState) -> HeaderValue {
    // Max-Age=0 tells the browser to discard the cookie immediately.
    build_cookie(&state.session_cookie_name, "", 0, false)
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `POST /api/v1/auth/login`
///
/// Returns `200 OK` with a session cookie and [`LoginResponse`] on success.
/// Returns `409 Conflict` with `{"code":"must-change-password"}` when the
/// user's `must_change_pw` flag is set (session cookie is still issued so the
/// client can call change-password).
/// Returns `401` on wrong credentials or disabled account.
/// Returns `429` when the rate limiter rejects the address.
///
/// # Errors
///
/// Returns [`ApiError`] on rate-limit rejection, bad credentials, session
/// creation failure, or audit-write failure.
pub async fn login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<LoginRequest>,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>), ApiError> {
    let now = unix_now();

    // 1. Rate-limit check.
    if let Err(limited) = state.rate_limiter.check(addr.ip(), now) {
        return Err(ApiError::RateLimited {
            retry_after_seconds: limited.retry_after_seconds,
        });
    }

    // 2. Look up user.
    let lookup = state
        .user_store
        .find_by_username(&req.username)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let (user, hash) = match lookup {
        Some(row) if row.0.disabled_at.is_none() => row,
        _ => {
            // User absent or disabled.
            state.rate_limiter.record_failure(addr.ip(), now);
            state
                .audit_writer
                .record(AuditAppend::from_current_span(
                    ActorRef::System {
                        component: "auth".to_owned(),
                    },
                    AuditEvent::AuthLoginFailed,
                    AuditOutcome::Denied,
                ))
                .await?;
            return Err(ApiError::Unauthorized);
        }
    };

    // 3. Verify password.
    let ok =
        verify_password(&req.password, &hash).map_err(|e| ApiError::Internal(e.to_string()))?;

    if !ok {
        state.rate_limiter.record_failure(addr.ip(), now);
        state
            .audit_writer
            .record(AuditAppend::from_current_span(
                ActorRef::User {
                    id: user.id.clone(),
                },
                AuditEvent::AuthLoginFailed,
                AuditOutcome::Denied,
            ))
            .await?;
        return Err(ApiError::Unauthorized);
    }

    // 4. Success — clear rate-limit bucket and create session.
    state.rate_limiter.record_success(addr.ip());

    let session = state
        .session_store
        .create(
            &user.id,
            state.session_ttl_seconds,
            None,
            Some(addr.ip().to_string()),
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let cookie = set_cookie_header(&state, &session.id);
    let mut headers = HeaderMap::new();
    headers.insert("Set-Cookie", cookie);

    state
        .audit_writer
        .record(AuditAppend::from_current_span(
            ActorRef::User {
                id: user.id.clone(),
            },
            AuditEvent::AuthLoginSucceeded,
            AuditOutcome::Ok,
        ))
        .await?;

    // 5. Step-up when must_change_pw is set.
    if user.must_change_pw {
        return Ok((
            StatusCode::CONFLICT,
            headers,
            Json(error_body("code", "must-change-password")),
        ));
    }

    let body = Json(
        serde_json::to_value(LoginResponse {
            user_id: user.id,
            role: user.role,
            must_change_pw: user.must_change_pw,
            config_version: 0,
        })
        .map_err(|e| ApiError::Internal(e.to_string()))?,
    );

    Ok((StatusCode::OK, headers, body))
}

/// `POST /api/v1/auth/logout`
///
/// Revokes the current session and clears the cookie. Returns `204 No Content`.
///
/// # Errors
///
/// Returns [`ApiError`] if session revocation or the audit write fails.
pub async fn logout(
    State(state): State<Arc<AppState>>,
    session: AuthenticatedSession,
) -> Result<(StatusCode, HeaderMap), ApiError> {
    state
        .session_store
        .revoke(&session.session_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    state
        .audit_writer
        .record(AuditAppend::from_current_span(
            ActorRef::User {
                id: session.user_id,
            },
            AuditEvent::AuthLogout,
            AuditOutcome::Ok,
        ))
        .await?;

    let mut headers = HeaderMap::new();
    headers.insert("Set-Cookie", clear_cookie_header(&state));
    Ok((StatusCode::NO_CONTENT, headers))
}

/// `POST /api/v1/auth/change-password`
///
/// Verifies the old password, validates the new password, updates it, clears
/// the `must_change_pw` flag, and revokes all other sessions for the user.
/// Returns `204 No Content`.
///
/// # Errors
///
/// Returns [`ApiError`] on wrong old password, validation failure, storage
/// errors, or audit-write failure.
pub async fn change_password(
    State(state): State<Arc<AppState>>,
    session: AuthenticatedSession,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<StatusCode, ApiError> {
    // 1. Look up current hash.
    // We need the hash to verify the old password; we find by user_id indirectly
    // by searching all users — but UserStore only exposes find_by_username.
    // Use a helper that fetches by user_id via the session's user_id.
    let (user, current_hash) = state
        .user_store
        .find_user_by_id(&session.user_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::Unauthorized)?;

    // 2. Verify old password.
    let ok = verify_password(&req.old_password, &current_hash)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if !ok {
        return Err(ApiError::Unauthorized);
    }

    // 3. Validate new password.
    if req.new_password.len() < 12 {
        return Err(ApiError::BadRequest(
            "new password must be at least 12 characters".to_owned(),
        ));
    }
    if req.new_password == req.old_password {
        return Err(ApiError::BadRequest(
            "new password must differ from the current password".to_owned(),
        ));
    }

    // 4. Persist the new password and clear the flag.
    state
        .user_store
        .update_password(&user.id, &req.new_password)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    state
        .user_store
        .set_must_change_pw(&user.id, false)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // 5. Revoke all sessions for this user (including the current one).
    let revoked_count = state
        .session_store
        .revoke_all_for_user(&user.id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // 6. Emit one audit row per revoked session.
    for _ in 0..revoked_count {
        state
            .audit_writer
            .record(AuditAppend::from_current_span(
                ActorRef::User {
                    id: user.id.clone(),
                },
                AuditEvent::AuthSessionRevoked,
                AuditOutcome::Ok,
            ))
            .await?;
    }

    Ok(StatusCode::NO_CONTENT)
}
