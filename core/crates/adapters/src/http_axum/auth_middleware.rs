//! Tower middleware that resolves a request's authentication context (Slice 9.6).
//!
//! Reads either:
//! * a session cookie (name from [`AppState::session_cookie_name`]), or
//! * an `Authorization: Bearer <token>` header.
//!
//! Attaches an [`AuthContext`] to request extensions. Handlers that require
//! authentication extract [`AuthenticatedSession`] from the extensions.

use std::sync::Arc;
use std::time::SystemTime;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;
use sha2::{Digest as _, Sha256};
use sqlx::Row as _;

use crate::auth::sessions::parse_cookie;
use crate::auth::users::UserRole;
use crate::http_axum::AppState;
use crate::http_axum::auth_routes::ApiError;

// ── Path classification ───────────────────────────────────────────────────────

/// Path access class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathClass {
    /// No authentication needed; always pass through.
    Public,
    /// Reachable while `must_change_pw = true`; also reachable normally.
    MustChangePassword,
    /// All other paths; require a valid auth context.
    Protected,
}

fn classify(path: &str) -> PathClass {
    // Normalise trailing slashes so that `/api/v1/health/` and `/api/v1/health`
    // both classify correctly (F022).
    let path = path.trim_end_matches('/');
    match path {
        "/api/v1/health" | "/api/v1/openapi.json" | "/api/v1/auth/login" => PathClass::Public,
        "/api/v1/auth/change-password" => PathClass::MustChangePassword,
        _ => PathClass::Protected,
    }
}

// ── Auth context ──────────────────────────────────────────────────────────────

/// The resolved authentication context attached to every authenticated request.
#[derive(Clone, Debug)]
pub enum AuthContext {
    /// Authenticated via a session cookie.
    Session {
        /// ULID of the authenticated user.
        user_id: String,
        /// Role assigned to the user.
        role: UserRole,
        /// The session token.
        session_id: String,
        /// When `true` the user must change their password before using other endpoints.
        must_change_pw: bool,
    },
    /// Authenticated via an `Authorization: Bearer` token.
    Token {
        /// Identifier of the token row.
        token_id: String,
        /// Permission blob stored in the tokens table.
        permissions: serde_json::Value,
        /// Per-token rate-limit quota.
        rate_limit_qps: u32,
        /// When `true` the owning user must change their password (F012).
        must_change_pw: bool,
    },
}

// ── Extractor ─────────────────────────────────────────────────────────────────

/// Extractor that provides the authenticated session context for handlers.
///
/// Populated by [`auth_layer`] which runs before any handler. If the
/// middleware is bypassed (public routes), this extractor will return 401.
#[derive(Clone, Debug)]
pub struct AuthenticatedSession(pub AuthContext);

/// Convenience accessors.
impl AuthenticatedSession {
    /// Return the `user_id` if this is a session context.
    pub fn user_id(&self) -> Option<&str> {
        match &self.0 {
            AuthContext::Session { user_id, .. } => Some(user_id.as_str()),
            AuthContext::Token { .. } => None,
        }
    }

    /// Return the `session_id` if this is a session context.
    pub fn session_id(&self) -> Option<&str> {
        match &self.0 {
            AuthContext::Session { session_id, .. } => Some(session_id.as_str()),
            AuthContext::Token { .. } => None,
        }
    }
}

impl<S> FromRequestParts<S> for AuthenticatedSession
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Self>()
            .cloned()
            .ok_or(ApiError::Unauthenticated)
    }
}

// ── Middleware ────────────────────────────────────────────────────────────────

/// SHA-256 hex digest of the raw bearer token string.
///
/// SHA-256 is used (not HMAC) because bearer tokens are generated as 256-bit
/// random values — preimage attack is computationally infeasible at that entropy
/// level (F019). If token entropy is ever reduced below 128 bits, switch to
/// HMAC-SHA256 keyed by a server secret.
fn sha256_hex(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    format!("{digest:x}")
}

/// Tower middleware function (for use with [`axum::middleware::from_fn_with_state`]).
///
/// # Errors
///
/// Returns [`ApiError::Unauthorized`] when a protected path cannot be
/// authenticated, or [`ApiError`] with `"must-change-password"` code when
/// the session has `must_change_pw = true` and the path is not the
/// change-password route.
pub async fn auth_layer(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    mut request: axum::extract::Request,
    next: Next,
) -> Result<Response, ApiError> {
    let path = request.uri().path().to_owned();
    let class = classify(&path);

    // Public routes bypass auth entirely.
    if class == PathClass::Public {
        return Ok(next.run(request).await);
    }

    let headers = request.headers().clone();

    // 1. Try session cookie.
    let auth_ctx = if let Some(session_id) = parse_cookie(&headers, &state.session_cookie_name) {
        let session = state
            .session_store
            .touch(&session_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;

        if let Some(session) = session {
            // Defense-in-depth: secondary expiry check at the middleware layer (F021).
            // touch() already enforces this via SQL, but guard against clock skew.
            let now_unix = i64::try_from(
                SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            )
            .unwrap_or(i64::MAX);
            if session.expires_at <= now_unix {
                return Err(ApiError::Unauthenticated);
            }

            // Look up the user to get role, must_change_pw, and disabled_at.
            let user_row = state
                .user_store
                .find_user_by_id(&session.user_id)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?;

            if let Some((user, _hash)) = user_row {
                // Reject sessions for disabled accounts (F011).
                if user.disabled_at.is_some() {
                    return Err(ApiError::Forbidden {
                        code: "account-disabled",
                    });
                }
                Some(AuthContext::Session {
                    user_id: user.id,
                    role: user.role,
                    session_id,
                    must_change_pw: user.must_change_pw,
                })
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // 2. Try Authorization: Bearer <token>.
    let auth_ctx = if auth_ctx.is_none() {
        try_bearer_token(&state, &headers).await?
    } else {
        auth_ctx
    };

    // 3. If no auth context resolved and path is protected → 401.
    let Some(auth_ctx) = auth_ctx else {
        return Err(ApiError::Unauthenticated);
    };

    // 4. must_change_pw enforcement — applies to both session and bearer token auth (F012).
    let ctx_must_change_pw = match &auth_ctx {
        AuthContext::Session { must_change_pw, .. } | AuthContext::Token { must_change_pw, .. } => {
            *must_change_pw
        }
    };
    if ctx_must_change_pw && class != PathClass::MustChangePassword {
        return Err(ApiError::Forbidden {
            code: "must-change-password",
        });
    }

    // 5. Attach context to request extensions and continue.
    request
        .extensions_mut()
        .insert(AuthenticatedSession(auth_ctx));
    Ok(next.run(request).await)
}

/// Attempt to authenticate via an `Authorization: Bearer <token>` header.
///
/// Returns `Ok(None)` when no bearer header is present or no matching row
/// is found. Returns `Err` only on storage failures.
async fn try_bearer_token(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Result<Option<AuthContext>, ApiError> {
    let Some(bearer) = extract_bearer(headers) else {
        return Ok(None);
    };

    let hash = sha256_hex(&bearer);

    // Look up the token by its hash.
    let Some(pool) = state.token_pool.as_ref() else {
        return Ok(None);
    };

    // LEFT JOIN to users to enforce disabled_at and must_change_pw for bearer
    // tokens (F011, F012).  user_id is nullable (pre-migration tokens have no
    // user association), so the join is outer — tokens without a user_id skip
    // the user-level checks and are treated as active/unrestricted.
    let row = sqlx::query(
        "SELECT t.token_id, t.permissions, t.rate_limit_qps, \
                u.must_change_pw, u.disabled_at \
         FROM tokens t \
         LEFT JOIN users u ON u.id = t.user_id \
         WHERE t.token_hash = ?1 AND t.revoked_at IS NULL",
    )
    .bind(&hash)
    .fetch_optional(pool)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    if let Some(row) = row {
        // Reject tokens whose owning user is disabled (F011).
        let disabled_at: Option<i64> = row
            .try_get("disabled_at")
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        if disabled_at.is_some() {
            return Err(ApiError::Forbidden {
                code: "account-disabled",
            });
        }

        let token_id: String = row
            .try_get("token_id")
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        let permissions_str: String = row
            .try_get("permissions")
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        let rate_limit_qps: i64 = row
            .try_get("rate_limit_qps")
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        // NULL when user_id is absent (legacy token); default to false.
        let must_change_pw: bool = row.try_get("must_change_pw").unwrap_or(false);
        let permissions: serde_json::Value = serde_json::from_str(&permissions_str)
            .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
        Ok(Some(AuthContext::Token {
            token_id,
            permissions,
            rate_limit_qps: u32::try_from(rate_limit_qps).unwrap_or(0),
            must_change_pw,
        }))
    } else {
        Ok(None)
    }
}

/// Extract the raw token string from `Authorization: Bearer <token>`.
fn extract_bearer(headers: &axum::http::HeaderMap) -> Option<String> {
    let value = headers.get(axum::http::header::AUTHORIZATION)?;
    let s = value.to_str().ok()?;
    s.strip_prefix("Bearer ").map(ToOwned::to_owned)
}
