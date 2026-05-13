//! Session persistence and cookie codec for the HTTP API.

use std::sync::Arc;

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use http::{HeaderMap, HeaderValue};
use sqlx::{Row as _, SqlitePool};
use thiserror::Error;

use crate::rng::RandomBytes;

// ---------------------------------------------------------------------------
// Session model
// ---------------------------------------------------------------------------

/// A persisted login session.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Session {
    /// 256-bit opaque token, base64url-encoded (no padding).
    pub id: String,
    /// The user this session belongs to.
    pub user_id: String,
    /// Unix timestamp (seconds) when the session was created.
    pub created_at: i64,
    /// Unix timestamp (seconds) of the most recent activity.
    pub last_seen_at: i64,
    /// Unix timestamp (seconds) after which the session is no longer valid.
    pub expires_at: i64,
    /// Unix timestamp (seconds) when the session was revoked, or `None` if active.
    pub revoked_at: Option<i64>,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur in session operations.
#[derive(Debug, Error)]
pub enum SessionError {
    /// A database operation failed.
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Persist and retrieve sessions.
#[async_trait]
pub trait SessionStore: Send + Sync + 'static {
    /// Create a new session for `user_id` that lives for `ttl_seconds`.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Db`] if the insert fails.
    async fn create(
        &self,
        user_id: &str,
        ttl_seconds: u64,
        ua: Option<String>,
        ip: Option<String>,
    ) -> Result<Session, SessionError>;

    /// Refresh `last_seen_at`. Returns `None` when the session is revoked or
    /// expired.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Db`] if the query fails.
    async fn touch(&self, session_id: &str) -> Result<Option<Session>, SessionError>;

    /// Mark a single session as revoked.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Db`] if the update fails.
    async fn revoke(&self, session_id: &str) -> Result<(), SessionError>;

    /// Revoke every active session for `user_id`. Returns the count revoked.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Db`] if the update fails.
    async fn revoke_all_for_user(&self, user_id: &str) -> Result<u32, SessionError>;
}

// ---------------------------------------------------------------------------
// SQLite implementation
// ---------------------------------------------------------------------------

/// A [`SessionStore`] backed by `SQLite`.
pub struct SqliteSessionStore {
    pool: SqlitePool,
    rng: Arc<dyn RandomBytes>,
}

impl SqliteSessionStore {
    /// Construct a new store.
    pub fn new(pool: SqlitePool, rng: Arc<dyn RandomBytes>) -> Self {
        Self { pool, rng }
    }

    fn now() -> i64 {
        // Unix timestamp in seconds fits in i64 for centuries; the cast is safe.
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        i64::try_from(secs).unwrap_or(i64::MAX)
    }

    fn generate_id(&self) -> String {
        let mut buf = [0u8; 32];
        self.rng.fill_bytes(&mut buf);
        URL_SAFE_NO_PAD.encode(buf)
    }
}

#[async_trait]
impl SessionStore for SqliteSessionStore {
    async fn create(
        &self,
        user_id: &str,
        ttl_seconds: u64,
        ua: Option<String>,
        ip: Option<String>,
    ) -> Result<Session, SessionError> {
        let id = self.generate_id();
        let now = Self::now();
        #[allow(clippy::cast_possible_wrap)]
        // zd:N/A reason: ttl_seconds will never approach i64::MAX
        let expires_at = now + ttl_seconds as i64;

        sqlx::query(
            "INSERT INTO sessions \
             (id, user_id, created_at, last_seen_at, expires_at, user_agent, ip_address) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(&id)
        .bind(user_id)
        .bind(now)
        .bind(now)
        .bind(expires_at)
        .bind(ua)
        .bind(ip)
        .execute(&self.pool)
        .await?;

        Ok(Session {
            id,
            user_id: user_id.to_owned(),
            created_at: now,
            last_seen_at: now,
            expires_at,
            revoked_at: None,
        })
    }

    async fn touch(&self, session_id: &str) -> Result<Option<Session>, SessionError> {
        let now = Self::now();

        let Some(row) = sqlx::query(
            "SELECT id, user_id, created_at, last_seen_at, expires_at, revoked_at \
             FROM sessions WHERE id = ?1",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        let revoked_at: Option<i64> = row.try_get("revoked_at")?;
        let expires_at: i64 = row.try_get("expires_at")?;

        if revoked_at.is_some() || expires_at < now {
            return Ok(None);
        }

        sqlx::query("UPDATE sessions SET last_seen_at = ?1 WHERE id = ?2")
            .bind(now)
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(Some(Session {
            id: row.try_get("id")?,
            user_id: row.try_get("user_id")?,
            created_at: row.try_get("created_at")?,
            last_seen_at: now,
            expires_at,
            revoked_at: None,
        }))
    }

    async fn revoke(&self, session_id: &str) -> Result<(), SessionError> {
        let now = Self::now();
        sqlx::query("UPDATE sessions SET revoked_at = ?1 WHERE id = ?2")
            .bind(now)
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn revoke_all_for_user(&self, user_id: &str) -> Result<u32, SessionError> {
        let now = Self::now();
        let result = sqlx::query(
            "UPDATE sessions SET revoked_at = ?1 WHERE user_id = ?2 AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(u32::try_from(result.rows_affected()).unwrap_or(u32::MAX))
    }
}

// ---------------------------------------------------------------------------
// Cookie codec
// ---------------------------------------------------------------------------

/// Build an HTTP `Set-Cookie` header value for a session cookie.
///
/// Attributes: `HttpOnly`, `SameSite=Strict`, `Path=/`, `Max-Age`.
/// The `Secure` attribute is added when `secure` is `true`.
pub fn build_cookie(name: &str, session_id: &str, ttl_seconds: u64, secure: bool) -> HeaderValue {
    let secure_suffix = if secure { "; Secure" } else { "" };
    let value = format!(
        "{name}={session_id}; Path=/; HttpOnly; SameSite=Strict; Max-Age={ttl_seconds}{secure_suffix}"
    );
    // SAFETY: the cookie string contains only printable ASCII characters that
    // are always valid HTTP header values.
    HeaderValue::from_str(&value).unwrap_or_else(|_| {
        // Fallback: return a safe empty-ish value rather than panic in
        // production. In practice this branch is unreachable.
        HeaderValue::from_static("__invalid__")
    })
}

/// Extract a cookie value by name from `Cookie:` request headers.
pub fn parse_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    for header in headers.get_all(http::header::COOKIE) {
        let s = header.to_str().ok()?;
        for pair in s.split(';') {
            let pair = pair.trim();
            if let Some(rest) = pair.strip_prefix(name) {
                if let Some(val) = rest.strip_prefix('=') {
                    return Some(val.to_owned());
                }
            }
        }
    }
    None
}
