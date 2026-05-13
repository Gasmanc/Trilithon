# Phase 09 — HTTP API surface (read + mutate) — Implementation Slices

> Phase reference: [../phases/phase-09-http-api.md](../phases/phase-09-http-api.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference [../phases/phase-09-http-api.md](../phases/phase-09-http-api.md).
- Architecture §6.2 (`users`), §6.3 (`sessions`), §6.4 (`tokens`), §6.6 (audit kinds for `auth.*`, `mutation.*`, `secrets.revealed`), §6.7 (`mutations`), §6.13 (capability probe results), §7.1 (mutation lifecycle), §8.1 (Caddy admin contract), §11 (security posture), §12.1 (tracing events `http.request.received`, `http.request.completed`).
- Trait signatures: `core::http::HttpServer`, `core::storage::Storage`, `core::reconciler::Applier`, `core::diff::DiffEngine`.
- ADRs: ADR-0009 (audit log), ADR-0011 (loopback by default), ADR-0012 (optimistic concurrency).

## Slice plan summary

| # | Title | Primary files | Effort (h) | Depends on |
|---|-------|---------------|-----------:|-----------|
| 9.1 | `axum` server scaffold, loopback bind, `/api/v1/health` (with `apply_in_flight`), OpenAPI | `core/crates/adapters/src/http_axum/mod.rs`, `core/crates/adapters/src/http_axum/health.rs` | 6 | Phase 8 |
| 9.2 | Argon2id password hashing and `users` adapter | `core/crates/adapters/src/auth/passwords.rs`, `core/crates/adapters/src/auth/users.rs` | 5 | 9.1 |
| 9.3 | Sessions table writer, cookie codec, login rate limiter | `core/crates/adapters/src/auth/sessions.rs`, `core/crates/adapters/src/auth/rate_limit.rs` | 6 | 9.2 |
| 9.4 | Bootstrap-account flow with `bootstrap-credentials.txt` mode 0600 | `core/crates/adapters/src/auth/bootstrap.rs` | 6 | 9.2 |
| 9.5 | Auth endpoints: login, logout, change-password | `core/crates/adapters/src/http_axum/auth_routes.rs` | 6 | 9.3, 9.4 |
| 9.6 | Authentication middleware (sessions and tokens) | `core/crates/adapters/src/http_axum/auth_middleware.rs` | 5 | 9.3 |
| 9.7 | `POST /api/v1/mutations` with `expected_version` envelope | `core/crates/adapters/src/http_axum/mutations.rs` | 7 | 9.6, Phase 7 |
| 9.8 | Snapshot read endpoints (list, get, diff) and `GET /api/v1/routes` | `core/crates/adapters/src/http_axum/snapshots.rs`, `core/crates/adapters/src/http_axum/routes.rs` | 6 | 9.6, Phase 5, Phase 8 |
| 9.9 | `GET /api/v1/audit` with paginated filters | `core/crates/adapters/src/http_axum/audit_routes.rs` | 4 | 9.6, Phase 6 |
| 9.10 | Drift endpoints: current, adopt, reapply, defer | `core/crates/adapters/src/http_axum/drift_routes.rs` | 5 | 9.6, Phase 8 |
| 9.11 | `GET /api/v1/capabilities` plus error envelope and OpenAPI publication | `core/crates/adapters/src/http_axum/capabilities.rs`, `core/crates/adapters/src/http_axum/openapi.rs` | 4 | 9.7, 9.8, 9.9, 9.10 |

---

## Slice 9.1 [cross-cutting] — `axum` server scaffold, loopback bind, `/api/v1/health`, OpenAPI surface

### Goal

Stand up the `axum`-based HTTP server in `adapters`, binding `127.0.0.1:<port>` by default. Register `GET /api/v1/health` returning 200 once the daemon reaches the ready state. Register `GET /api/v1/openapi.json` returning a static placeholder document; slice 9.11 fills it in. The phase reference's H1 mitigation lives here: remote binding requires `network.allow_remote_binding = true` and emits a stark warning at startup.

### Entry conditions

- Phase 8 done (mutation, snapshot, audit, drift in place).
- `core::http::HttpServer` trait exists per `trait-signatures.md` §10.

### Files to create or modify

- `core/crates/adapters/src/http_axum/mod.rs` — server type and `HttpServer` impl.
- `core/crates/adapters/src/http_axum/health.rs` — health handler.
- `core/crates/adapters/src/http_axum/openapi.rs` — placeholder.
- `core/crates/adapters/Cargo.toml` — add `axum`, `tower`, `tower-http`, `utoipa`, `utoipa-axum`.
- `core/crates/cli/src/main.rs` — instantiate and run.

### Signatures and shapes

```rust
use std::net::SocketAddr;
use std::sync::Arc;
use async_trait::async_trait;
use trilithon_core::http::{HttpServer, ServerConfig, HttpServerError, ShutdownSignal};

#[derive(Clone, Debug)]
pub struct AxumServerConfig {
    pub bind_host:               String,        // default "127.0.0.1"
    pub bind_port:               u16,           // default 7878
    pub allow_remote_binding:    bool,          // default false
    pub session_cookie_name:     String,        // default "trilithon_session"
    pub session_ttl_seconds:     u64,           // default 12 * 3600
}

pub struct AxumServer { /* router, listener, deps */ }

#[async_trait]
impl HttpServer for AxumServer {
    async fn bind(&mut self, config: &ServerConfig) -> Result<SocketAddr, HttpServerError>;
    async fn run(self, shutdown: ShutdownSignal) -> Result<(), HttpServerError>;
    async fn shutdown(&self) -> Result<(), HttpServerError>;
}

pub fn router() -> axum::Router<AppState>;

#[derive(Clone)]
pub struct AppState {
    pub storage:   Arc<dyn trilithon_core::storage::Storage>,
    pub applier:   Arc<dyn trilithon_core::reconciler::Applier>,
    pub diff:      Arc<dyn trilithon_core::diff::DiffEngine>,
    pub audit:     Arc<crate::audit_writer::AuditWriter>,
    pub clock:     Arc<dyn trilithon_core::clock::Clock>,
    pub rng:       Arc<dyn crate::rng::RandomBytes>,
    pub capabilities: Arc<crate::capability_cache::CapabilityCache>,
    pub drift:     Arc<crate::drift::DriftDetector>,
}
```

```http
GET /api/v1/health HTTP/1.1
→ 200 OK
{
  "status": "ready",
  "trilithon_version": "<semver>",
  "ready_since_unix_ms": <i64>,
  "apply_in_flight": false
}
```

The `apply_in_flight` field is a boolean published by the applier (Phase 7). The HTTP layer reads it from a shared `Arc<AtomicBool>` exposed on `AppState` (`pub apply_in_flight: Arc<AtomicBool>`); the applier sets it `true` for the duration of any in-flight `Applier::apply` call and `false` otherwise. The dashboard (Phase 11) consumes this field to render the "applying" banner.

### Algorithm

1. `bind` resolves `bind_host`. If the host is non-loopback (`127.0.0.0/8` or `::1/128` are loopback) and `allow_remote_binding == false`, return `HttpServerError::BindFailed { detail: "remote binding requires network.allow_remote_binding = true" }`.
2. If `allow_remote_binding == true`, emit `tracing::warn!(target = "http.bind.remote", "binding to non-loopback interface; authentication is required for every endpoint")`.
3. Bind the TCP listener; record the bound `SocketAddr`. Emit `tracing::info!(target = "daemon.started", bind = %addr)`.
4. `run` consumes `self`, attaches the shutdown future via `axum::serve(...).with_graceful_shutdown(...)`, and returns when the future resolves.
5. `GET /api/v1/health` returns 200 with the JSON body once the daemon's ready watch channel is `true`. Before ready, returns `503 Service Unavailable` with `{ "status": "starting" }`.
6. `GET /api/v1/openapi.json` returns a placeholder document with `info.title = "Trilithon Daemon API"`, `openapi: "3.1.0"`, no paths. Slice 9.11 replaces with the generated document.

### Tests

- `core/crates/adapters/tests/http_health_returns_200.rs` — start server; `GET /api/v1/health`; assert 200 within 5 seconds.
- `core/crates/adapters/tests/http_loopback_default.rs` — `AxumServerConfig::default().bind_host == "127.0.0.1"`.
- `core/crates/adapters/tests/http_remote_bind_rejected_without_flag.rs` — set `bind_host = "0.0.0.0"`, `allow_remote_binding = false`; assert `bind` returns `BindFailed`.
- `core/crates/adapters/tests/http_remote_bind_warns_with_flag.rs` — set `allow_remote_binding = true`; assert the warn-level event was emitted (capture via `tracing-test`).
- `core/crates/adapters/tests/http_openapi_placeholder.rs` — `GET /api/v1/openapi.json` returns a parseable OpenAPI 3.1 document.

### Acceptance command

`cargo test -p trilithon-adapters http_health_ http_loopback_ http_remote_bind_ http_openapi_`

### Exit conditions

- `GET /api/v1/health` MUST return 200 within 5 seconds of `trilithon run`.
- The default bind MUST be loopback.
- Remote binding MUST require `allow_remote_binding = true` and MUST emit the warning.
- `cargo build -p trilithon-adapters` succeeds.

### Audit kinds emitted

None directly.

### Tracing events emitted

`daemon.started`, `http.request.received`, `http.request.completed` (architecture §12.1).

### Cross-references

- ADR-0011.
- PRD T1.13.
- Hazard H1.
- trait-signatures.md §10 `HttpServer`.

---

## Slice 9.2 [standard] — Argon2id password hashing and `users` adapter

### Goal

Hash passwords with Argon2id at the RFC 9106 first-recommendation parameters (`m_cost = 19456 KiB, t_cost = 2, p_cost = 1`) and persist users in the `users` table. Provide a typed `UserStore` over `Storage` that exposes `find_by_username`, `verify_password`, `create_user`, `update_password`, and `set_must_change_pw`.

### Entry conditions

- Slice 9.1 done.
- The `users` table from architecture §6.2 exists (Phase 2 migration).

### Files to create or modify

- `core/crates/adapters/src/auth/mod.rs` — module root.
- `core/crates/adapters/src/auth/passwords.rs` — Argon2 wrapper.
- `core/crates/adapters/src/auth/users.rs` — `UserStore`.
- `core/crates/adapters/Cargo.toml` — add `argon2`, `password-hash`.

### Signatures and shapes

```rust
use argon2::{Argon2, Algorithm, Version, Params};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

pub const ARGON2_M_COST_KIB: u32 = 19456;
pub const ARGON2_T_COST: u32 = 2;
pub const ARGON2_P_COST: u32 = 1;

pub fn argon2id() -> Argon2<'static>;

pub fn hash_password(plaintext: &str, salt: &SaltString) -> Result<String, PasswordError>;
pub fn verify_password(plaintext: &str, encoded_hash: &str) -> Result<bool, PasswordError>;

#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    #[error("argon2 failure: {0}")]
    Argon2(String),
    #[error("hash decoding failure: {0}")]
    Decode(String),
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub id:             String,    // ULID
    pub username:       String,
    pub role:           UserRole,
    pub created_at:     i64,
    pub must_change_pw: bool,
    pub disabled_at:    Option<i64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserRole { Owner, Operator, Reader }

#[async_trait::async_trait]
pub trait UserStore: Send + Sync + 'static {
    async fn find_by_username(&self, username: &str) -> Result<Option<(User, String /* hash */)>, UserStoreError>;
    async fn create_user(&self, username: &str, password: &str, role: UserRole) -> Result<User, UserStoreError>;
    async fn update_password(&self, user_id: &str, new_password: &str) -> Result<(), UserStoreError>;
    async fn set_must_change_pw(&self, user_id: &str, value: bool) -> Result<(), UserStoreError>;
}
```

### Algorithm

1. `argon2id()` constructs `Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::new(ARGON2_M_COST_KIB, ARGON2_T_COST, ARGON2_P_COST, None)?)`.
2. `hash_password` calls `PasswordHasher::hash_password` with a fresh salt.
3. `verify_password` parses the encoded hash and calls `PasswordVerifier::verify_password`.
4. `UserStore::create_user` generates a fresh `Ulid`, hashes the password, and inserts into `users`.
5. `update_password` re-hashes and writes the new encoded string.

### Tests

- `core/crates/adapters/tests/argon2_parameters.rs` — assert the params via reflection through the encoded hash string (it embeds them).
- `core/crates/adapters/tests/argon2_distinct_passwords_distinct_hashes.rs` — property test: 100 random passwords; 100 distinct hashes.
- `core/crates/adapters/tests/users_create_and_verify.rs` — create then verify; assert success and 0.5–2-second hash time on a CI runner (sanity).
- `core/crates/adapters/tests/users_wrong_password_fails.rs` — verify with wrong password returns `Ok(false)`.

### Acceptance command

`cargo test -p trilithon-adapters argon2_ users_create_ users_wrong_`

### Exit conditions

- The Argon2 parameters MUST be `m=19456, t=2, p=1`.
- Hashes are stored only in `users.password_hash`.
- Distinct passwords MUST produce distinct hashes.

### Audit kinds emitted

None directly. Slice 9.5 emits `auth.login-succeeded` / `auth.login-failed` from the login handler.

### Tracing events emitted

None new.

### Cross-references

- PRD T1.14.
- Architecture §6.2.

---

## Slice 9.3 [standard] — Sessions table writer, cookie codec, login rate limiter

### Goal

Persist sessions in the `sessions` table; encode the session id in an HTTP cookie (`HttpOnly`, `Secure` when binding non-loopback or behind TLS, `SameSite=Strict`). Provide an in-memory rate limiter keyed by source address that tolerates at most five failures per minute and applies exponential backoff to a 60-second ceiling.

### Entry conditions

- Slice 9.2 done.
- The `sessions` table from architecture §6.3 exists.

### Files to create or modify

- `core/crates/adapters/src/auth/sessions.rs` — `SessionStore`, cookie codec.
- `core/crates/adapters/src/auth/rate_limit.rs` — in-memory rate limiter.

### Signatures and shapes

```rust
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub id:           String,    // 256-bit opaque, base64url
    pub user_id:      String,
    pub created_at:   i64,
    pub last_seen_at: i64,
    pub expires_at:   i64,
    pub revoked_at:   Option<i64>,
}

#[async_trait::async_trait]
pub trait SessionStore: Send + Sync + 'static {
    async fn create(&self, user_id: &str, ttl_seconds: u64, ua: Option<String>, ip: Option<String>) -> Result<Session, SessionError>;
    async fn touch(&self, session_id: &str) -> Result<Option<Session>, SessionError>;
    async fn revoke(&self, session_id: &str) -> Result<(), SessionError>;
    async fn revoke_all_for_user(&self, user_id: &str) -> Result<u32, SessionError>;
}

pub fn build_cookie(name: &str, session_id: &str, ttl_seconds: u64, secure: bool) -> http::HeaderValue;
pub fn parse_cookie(headers: &http::HeaderMap, name: &str) -> Option<String>;

pub struct LoginRateLimiter { /* DashMap<IpAddr, BucketState> */ }

impl LoginRateLimiter {
    pub fn new() -> Self;
    /// Returns Ok(()) if the request is admitted; otherwise the retry-after.
    pub fn check(&self, addr: std::net::IpAddr, now_unix: i64) -> Result<(), RateLimited>;
    pub fn record_failure(&self, addr: std::net::IpAddr, now_unix: i64);
    pub fn record_success(&self, addr: std::net::IpAddr);
}

#[derive(Clone, Debug)]
pub struct RateLimited { pub retry_after_seconds: u32 }
```

### Algorithm

1. `SessionStore::create`: generate 32 random bytes via `RandomBytes`, base64url-encode as `session.id`, insert row, return `Session`. Set `expires_at = now + ttl_seconds`.
2. `touch`: update `last_seen_at`. Return `None` if `revoked_at IS NOT NULL` or `expires_at < now`.
3. `revoke`: set `revoked_at = now`.
4. `build_cookie`: `format!("{name}={session_id}; Path=/; HttpOnly; SameSite=Strict; Max-Age={ttl_seconds}{secure_suffix}")` where `secure_suffix` is `"; Secure"` when `secure` is true.
5. Rate limiter: per-address bucket holding `failure_count` and `next_allowed_at_unix`. On failure, `failure_count += 1`. If `failure_count > 5`, set `next_allowed_at_unix = now + min(2.pow(failure_count - 5), 60)`. `check` rejects if `now < next_allowed_at_unix`. On success, reset `failure_count = 0`.

### Tests

- `core/crates/adapters/tests/sessions_round_trip.rs` — create, touch, revoke; assert lifecycle states.
- `core/crates/adapters/tests/sessions_expiry_honoured.rs` — create with `ttl=1`, sleep 2; assert `touch` returns `None`.
- `core/crates/adapters/tests/cookie_codec_round_trip.rs` — `parse_cookie(build_cookie(...))` returns the session id.
- `core/crates/adapters/tests/rate_limit_admits_first_five_failures.rs` — five failures admitted, sixth rejected.
- `core/crates/adapters/tests/rate_limit_backoff_caps_at_60s.rs` — drive the bucket past `2.pow(11)`; assert the retry-after caps at 60.

### Acceptance command

`cargo test -p trilithon-adapters sessions_ cookie_codec_ rate_limit_`

### Exit conditions

- Sessions MUST be revocable (admin operation supported via `revoke_all_for_user`).
- Rate limiter MUST tolerate at most five failures per source per minute; backoff MUST cap at 60 seconds.

### Audit kinds emitted

None directly.

### Tracing events emitted

None new.

### Cross-references

- PRD T1.14.
- Architecture §6.3, §11.

---

## Slice 9.4 [cross-cutting] — Bootstrap-account flow with `bootstrap-credentials.txt` mode 0600

### Goal

On first startup with empty `users`, generate a random 24-character password, write it to `<data_dir>/bootstrap-credentials.txt` with mode `0600`, and log a single line directing the user to the file. The credentials MUST NOT appear in process arguments, environment variables, or any other log line, satisfying hazard H13.

### Entry conditions

- Slice 9.2 done.

### Files to create or modify

- `core/crates/adapters/src/auth/bootstrap.rs` — bootstrap logic.
- `core/crates/cli/src/main.rs` — invoke at startup.

### Signatures and shapes

```rust
pub struct BootstrapOutcome {
    pub user:                User,
    pub credentials_path:    std::path::PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("user store: {0}")]
    UserStore(#[from] crate::auth::users::UserStoreError),
    #[error("data directory not writable: {path}")]
    DataDirNotWritable { path: std::path::PathBuf },
}

pub async fn bootstrap_if_empty(
    user_store: &dyn crate::auth::users::UserStore,
    rng:        &dyn crate::rng::RandomBytes,
    data_dir:   &std::path::Path,
    audit:      &crate::audit_writer::AuditWriter,
) -> Result<Option<BootstrapOutcome>, BootstrapError>;
```

### Algorithm

1. `bootstrap_if_empty` opens a transaction; selects `COUNT(*) FROM users`. If `> 0`, return `Ok(None)`.
2. Generate 18 random bytes; base32-encode without padding to a 24-character ASCII password using the alphabet `ABCDEFGHJKLMNPQRSTUVWXYZ23456789` (Crockford-without-confusables).
3. Generate a username `admin`. If a `disabled_at IS NOT NULL` row already holds the name, suffix with the next free integer.
4. Create the user with `role = Owner`, `must_change_pw = true`.
5. Construct `let path = data_dir.join("bootstrap-credentials.txt");`. Open with `OpenOptions::new().create_new(true).write(true).mode(0o600)`. Write `format!("username: {}\npassword: {}\n", user.username, password)`. The file is truncated to user-readable only on Unix; on Windows the equivalent ACL restriction applies via `cacls` or the `windows` crate's security descriptor calls.
6. Emit exactly one log line at INFO: `"bootstrap account created. credentials written to {path}; you will be required to change the password on first login."`. The plaintext password MUST NOT appear in this line nor in environment variables, process arguments, or any other tracing event.
7. Write one `auth.bootstrap-credentials-rotated`-equivalent audit row? — the §6.6 vocabulary lists `auth.bootstrap-credentials-rotated` for rotation of the pair, not the initial creation. This slice writes one `auth.login-succeeded` row at first login (slice 9.5), not a creation row. The bootstrap MUST be observable through the file mode test alone for V1.
8. Return `Ok(Some(BootstrapOutcome { user, credentials_path: path }))`.

### Tests

- `core/crates/adapters/tests/bootstrap_creates_file_mode_0600.rs` — fresh data dir; run bootstrap; assert `metadata.permissions().mode() & 0o777 == 0o600` (Unix only, gated on `cfg(unix)`).
- `core/crates/adapters/tests/bootstrap_skips_when_users_exist.rs` — pre-populate one user; run bootstrap; assert `Ok(None)`.
- `core/crates/adapters/tests/bootstrap_password_not_in_logs.rs` — capture all tracing output during bootstrap; assert the password does not appear.
- `core/crates/adapters/tests/bootstrap_password_not_in_env.rs` — capture `std::env::vars()` after bootstrap; assert the password does not appear in any value.
- `core/crates/adapters/tests/bootstrap_must_change_pw_set.rs` — assert `users.must_change_pw = 1`.

### Acceptance command

`cargo test -p trilithon-adapters bootstrap_`

### Exit conditions

- The credentials file MUST exist with mode 0600 on Unix.
- The plaintext password MUST NOT appear in any log line, environment variable, or process argument.
- `users.must_change_pw` MUST be set to true for the bootstrap user.

### Audit kinds emitted

`auth.bootstrap-credentials-created` — written once on first run when the bootstrap account row is inserted and the credentials file is written. The audit row's `actor.kind = system`, `actor.id = bootstrap`, and the redacted_diff_json carries `{ "username": "<placeholder>", "credentials_path": "/var/lib/trilithon/bootstrap.json" }` (no password). `auth.login-succeeded` fires later, when the bootstrap user logs in (slice 9.5).

### Tracing events emitted

`daemon.started` extension carrying `bootstrap = true` when this is the first run.

### Cross-references

- PRD T1.14.
- Hazard H13.
- Architecture §6.2, §11.

---

## Slice 9.5 [standard] — Auth endpoints: login, logout, change-password

### Goal

Implement `POST /api/v1/auth/login`, `POST /api/v1/auth/logout`, and `POST /api/v1/auth/change-password`. Login emits `auth.login-succeeded` or `auth.login-failed` audit rows. Login with `must_change_pw = true` returns a step-up response (`409 Conflict` with `code: "must-change-password"`) and the only callable endpoint thereafter (with the partial session) is change-password.

### Entry conditions

- Slices 9.2, 9.3, 9.4 done.

### Files to create or modify

- `core/crates/adapters/src/http_axum/auth_routes.rs` — three handlers.
- `core/crates/adapters/src/http_axum/mod.rs` — wire routes.

### Signatures and shapes

```rust
#[derive(serde::Deserialize)]
pub struct LoginRequest { pub username: String, pub password: String }

#[derive(serde::Serialize)]
pub struct LoginResponse {
    pub user_id:        String,
    pub role:           crate::auth::users::UserRole,
    pub must_change_pw: bool,
    pub config_version: i64,
}

#[derive(serde::Deserialize)]
pub struct ChangePasswordRequest { pub old_password: String, pub new_password: String }

pub async fn login(
    State(state): State<AppState>,
    addr:         axum::extract::ConnectInfo<std::net::SocketAddr>,
    Json(req):    Json<LoginRequest>,
) -> Result<(http::HeaderMap, Json<LoginResponse>), ApiError>;

pub async fn logout(
    State(state): State<AppState>,
    session:      AuthenticatedSession,    // extractor from slice 9.6
) -> Result<http::StatusCode, ApiError>;

pub async fn change_password(
    State(state): State<AppState>,
    session:      AuthenticatedSession,
    Json(req):    Json<ChangePasswordRequest>,
) -> Result<http::StatusCode, ApiError>;
```

### Algorithm

1. `login`:
   1. `state.rate_limiter.check(addr.ip(), now)?`. On rejection, return `429` with `Retry-After`.
   2. Look up the user. If absent or `disabled_at.is_some()`, record failure on the rate limiter, write `auth.login-failed`, return `401`.
   3. `verify_password(&req.password, &hash)`. On false, record failure, write `auth.login-failed`, return `401`.
   4. On success, `state.rate_limiter.record_success(addr.ip())`. Create a session via `SessionStore::create`. Build the cookie. Write `auth.login-succeeded`.
   5. If `user.must_change_pw`, return `409 Conflict` with body `{ code: "must-change-password", session_id: <cookie> }` so the client can step up.
   6. Otherwise, return `200` with the cookie and `LoginResponse`.
2. `logout`:
   1. Revoke the session via `SessionStore::revoke`.
   2. Emit a `Set-Cookie` clearing the cookie.
   3. Write `auth.logout`. Return `204`.
3. `change_password`:
   1. Verify `req.old_password` against the current hash. On miss, return `401`.
   2. Validate new password against a minimum length of 12 characters and reject if equal to the old password.
   3. `update_password`; clear `must_change_pw`. Revoke all other sessions for the user via `revoke_all_for_user`. Return `204`.
   4. Write `auth.session-revoked` for each revoked session.

### Tests

- `core/crates/adapters/tests/auth_login_happy.rs` — create a user; login returns 200 and a session cookie.
- `core/crates/adapters/tests/auth_login_wrong_password_401.rs` — assert 401 and `auth.login-failed` audit row.
- `core/crates/adapters/tests/auth_login_bootstrap_step_up.rs` — bootstrap user logs in; assert 409 with `code: "must-change-password"`.
- `core/crates/adapters/tests/auth_login_rate_limited.rs` — six failures; sixth is 429 with `Retry-After`.
- `core/crates/adapters/tests/auth_logout_revokes_session.rs` — after logout, the cookie is invalid for any subsequent request.
- `core/crates/adapters/tests/auth_change_password_clears_flag.rs` — after change-password, `must_change_pw = false` and other sessions are revoked.

### Acceptance command

`cargo test -p trilithon-adapters auth_login_ auth_logout_ auth_change_password_`

### Exit conditions

- Bootstrap login MUST step up to change-password before any other endpoint becomes reachable.
- Login MUST emit exactly one `auth.login-succeeded` or `auth.login-failed` audit row per attempt.

### Audit kinds emitted

`auth.login-succeeded`, `auth.login-failed`, `auth.logout`, `auth.session-revoked` (architecture §6.6).

### Tracing events emitted

`http.request.received`, `http.request.completed`.

### Cross-references

- PRD T1.14.
- Hazard H13.
- Architecture §6.6, §11.

---

## Slice 9.6 [cross-cutting] — Authentication middleware (sessions and tokens)

### Goal

Tower middleware that resolves a request's authentication context from either a session cookie or an `Authorization: Bearer <token>` header. Unauthenticated requests to mutation endpoints return 401. The middleware exposes an `AuthenticatedSession` axum extractor for handler use.

### Entry conditions

- Slices 9.3, 9.5 done.

### Files to create or modify

- `core/crates/adapters/src/http_axum/auth_middleware.rs` — middleware and extractor.

### Signatures and shapes

```rust
#[derive(Clone, Debug)]
pub enum AuthContext {
    Session  { user_id: String, role: crate::auth::users::UserRole, session_id: String, must_change_pw: bool },
    Token    { token_id: String, permissions: serde_json::Value, rate_limit_qps: u32 },
}

#[derive(Clone, Debug)]
pub struct AuthenticatedSession(pub AuthContext);

pub async fn auth_layer(
    state:   AppState,
    request: axum::extract::Request,
    next:    axum::middleware::Next,
) -> Result<axum::response::Response, ApiError>;

#[async_trait::async_trait]
impl<S> axum::extract::FromRequestParts<S> for AuthenticatedSession
where S: Send + Sync,
{ /* extract from request extensions populated by auth_layer */ }
```

A path is classified as `Public` (whitelist: `/api/v1/health`, `/api/v1/openapi.json`, `/api/v1/auth/login`), `MustChangePassword` (`/api/v1/auth/change-password` is reachable while `must_change_pw = true`), or `Protected` (everything else). Unauthenticated `Protected` requests return 401.

### Algorithm

1. The middleware classifies `request.uri().path()` against the whitelist.
2. If `Public`, pass through.
3. Read the session cookie via `parse_cookie`. If present, `SessionStore::touch`. On `Some(session)`, look up the user, attach `AuthContext::Session` to request extensions.
4. Otherwise read `Authorization: Bearer <token>`. Hash the token via Argon2id and compare against `tokens.token_hash`. On match, attach `AuthContext::Token`.
5. If neither produces a context and the path is `Protected`, return 401 with `{ code: "unauthenticated" }`.
6. If `AuthContext::Session.must_change_pw` and the path is not `MustChangePassword`, return 403 with `{ code: "must-change-password" }`.
7. Otherwise pass through.

### Tests

- `core/crates/adapters/tests/auth_middleware_unauthenticated_mutation_401.rs` — `POST /api/v1/mutations` without a cookie; assert 401.
- `core/crates/adapters/tests/auth_middleware_session_admits.rs` — with a valid session cookie; assert handler is reached.
- `core/crates/adapters/tests/auth_middleware_token_admits.rs` — with a valid bearer token; assert handler is reached.
- `core/crates/adapters/tests/auth_middleware_invalid_token_401.rs` — bogus bearer; assert 401.
- `core/crates/adapters/tests/auth_middleware_must_change_password_blocks.rs` — `must_change_pw = true`; `GET /api/v1/snapshots` returns 403.

### Acceptance command

`cargo test -p trilithon-adapters auth_middleware_`

### Exit conditions

- No mutation endpoint is reachable without an authenticated session or a valid tool-gateway token.
- A `must_change_pw` session MUST only be able to reach change-password.

### Audit kinds emitted

None directly.

### Tracing events emitted

`http.request.received`, `http.request.completed`.

### Cross-references

- PRD T1.14.
- Architecture §6.4 (`tokens`), §11.

---

## Slice 9.7 [standard] — `POST /api/v1/mutations` with `expected_version` envelope

### Goal

Accept any variant of the typed mutation set behind the envelope `{ "expected_version": <i64>, "body": { ... } }`. Authenticate, generate a correlation id, validate, enqueue, and apply via Phase 7's applier. Return the resulting snapshot id and `config_version` on success, `409 Conflict` on stale version, `422 Unprocessable Entity` on validation failure, `400 Bad Request` with audit kind `mutation.rejected.missing-expected-version` when the envelope omits `expected_version`.

### Entry conditions

- Slice 9.6 done.
- Phase 7 ships the `Applier`.

### Files to create or modify

- `core/crates/adapters/src/http_axum/mutations.rs` — handler.

### Signatures and shapes

```rust
#[derive(serde::Deserialize)]
pub struct MutationEnvelope {
    pub expected_version: Option<i64>,   // missing => 400 + audit row
    pub body:             serde_json::Value,
}

#[derive(serde::Serialize)]
pub struct MutationResponse {
    pub snapshot_id:    String,
    pub config_version: i64,
}

#[derive(serde::Serialize)]
pub struct MutationConflictBody {
    pub code:             &'static str,    // "conflict"
    pub current_version:  i64,
    pub expected_version: i64,
}

pub async fn post_mutation(
    State(state): State<AppState>,
    session:      AuthenticatedSession,
    Json(env):    Json<MutationEnvelope>,
) -> Result<axum::Json<MutationResponse>, ApiError>;
```

### Algorithm

1. Generate or read the correlation id (slice 6.7's middleware).
2. If `env.expected_version.is_none()`, write `mutation.rejected.missing-expected-version` audit row with the actor and correlation id. Return 400 with `{ code: "missing-expected-version" }`.
3. Deserialise `env.body` into `core::Mutation`. On parse failure, write `mutation.rejected` with `error_kind = "schema"`. Return 422.
4. Validate the mutation via `core::validate::validate_mutation(&mutation, &capability_set, &current_state)`. On `Err`, write `mutation.rejected` with `error_kind = "validation"`. Return 422.
5. Compute the next desired state. Construct the snapshot. Insert via `Storage::insert_snapshot`.
6. Call `applier.apply(&snapshot, env.expected_version.unwrap()).await`. Branch:
   - `Ok(Succeeded { snapshot_id, config_version, .. })` → write `mutation.applied` audit row (Phase 7 already wrote `config.applied`; the mutation-row write happens here at the HTTP layer to align with architecture §7.1 step 11). Return 200 with `MutationResponse`.
   - `Ok(Failed { kind, detail, .. })` → write `mutation.rejected` with `error_kind = format!("{:?}", kind)`. Return 502 with `{ code: "apply-failed", detail }`.
   - `Err(ApplyError::OptimisticConflict { observed_version, expected_version })` → return 409 with `MutationConflictBody`. The applier already wrote `mutation.conflicted`.
   - `Err(other)` → return 500 with the redacted detail.

### Tests

- `core/crates/adapters/tests/mutation_happy_path.rs` — submit `CreateRoute`; assert one new snapshot row, one new `config.applied` audit row, one `mutation.applied` audit row, 200 response with the new id and version.
- `core/crates/adapters/tests/mutation_missing_expected_version_400.rs` — envelope without `expected_version`; assert 400 and one `mutation.rejected.missing-expected-version` row.
- `core/crates/adapters/tests/mutation_stale_version_409.rs` — submit two simultaneous mutations; assert one 200 and one 409 with `MutationConflictBody`.
- `core/crates/adapters/tests/mutation_invalid_body_422.rs` — malformed `body`; assert 422.
- `core/crates/adapters/tests/mutation_unauthenticated_401.rs` — no cookie; assert 401.

### Acceptance command

`cargo test -p trilithon-adapters mutation_`

### Exit conditions

- Every Tier 1 mutation variant MUST be acceptable through this endpoint.
- A missing `expected_version` MUST return 400 and write `mutation.rejected.missing-expected-version`.
- A stale `expected_version` MUST return 409.

### Audit kinds emitted

`mutation.applied`, `mutation.rejected`, `mutation.rejected.missing-expected-version`, `mutation.conflicted` (the latter via the applier).

### Tracing events emitted

`http.request.received`, `http.request.completed`, `apply.started`, `apply.succeeded`, `apply.failed`.

### Cross-references

- PRD T1.6, T1.8.
- ADR-0012.
- Architecture §6.6, §7.1.

---

## Slice 9.8 [cross-cutting] — Snapshot and route read endpoints

### Goal

Implement four read endpoints: `GET /api/v1/snapshots`, `GET /api/v1/snapshots/{id}`, `GET /api/v1/snapshots/{id}/diff/{other_id}`, and `GET /api/v1/routes`. Pagination uses cursor-after-id; the diff endpoint returns the redacted diff (never plaintext secrets); the routes list returns a flattened summary view derived from the latest desired-state snapshot.

### Entry conditions

- Slice 9.6 done.
- Phase 5 (snapshots), Phase 8 (diff engine).

### Files to create or modify

- `core/crates/adapters/src/http_axum/snapshots.rs` — three handlers.
- `core/crates/core/src/storage.rs` — extend `Storage` with `list_snapshots(SnapshotSelector, limit) -> Result<Vec<Snapshot>, _>` if not present; flag the trait extension below.

### Signatures and shapes

```rust
#[derive(serde::Serialize)]
pub struct SnapshotSummary {
    pub id:             String,
    pub parent_id:      Option<String>,
    pub config_version: i64,
    pub created_at:     i64,
    pub actor_kind:     String,
    pub actor_id:       String,
    pub intent:         String,
}

#[derive(serde::Deserialize)]
pub struct SnapshotListQuery {
    pub limit:        Option<u32>,    // default 50, max 200
    pub cursor_after: Option<String>, // descending pagination cursor (snapshot id)
}

pub async fn list_snapshots(
    State(state): State<AppState>,
    _:            AuthenticatedSession,
    Query(q):     Query<SnapshotListQuery>,
) -> Result<Json<Vec<SnapshotSummary>>, ApiError>;

pub async fn get_snapshot(
    State(state): State<AppState>,
    _:            AuthenticatedSession,
    Path(id):     Path<String>,
) -> Result<Json<serde_json::Value>, ApiError>;

#[derive(serde::Serialize)]
pub struct SnapshotDiffResponse {
    pub redacted_diff_json: serde_json::Value,
    pub redaction_sites:    u32,
}

pub async fn diff_snapshots(
    State(state): State<AppState>,
    _:            AuthenticatedSession,
    Path((a, b)): Path<(String, String)>,
) -> Result<Json<SnapshotDiffResponse>, ApiError>;

#[derive(serde::Serialize)]
pub struct RouteSummary {
    pub id:                String,             // RouteId
    pub hostnames:         Vec<String>,        // wildcard form preserved (`*.example.com`)
    pub upstream_count:    u32,
    pub policy_attached:   Option<RoutePolicySummary>,
    pub enabled:           bool,
    pub updated_at:        i64,                // UTC unix seconds
}

#[derive(serde::Serialize)]
pub struct RoutePolicySummary {
    pub preset_id:      String,
    pub preset_version: u32,
}

#[derive(serde::Deserialize)]
pub struct RouteListQuery {
    pub limit:        Option<u32>,    // default 100, max 500
    pub cursor_after: Option<String>, // RouteId cursor (lexicographic)
    pub hostname_filter: Option<String>, // case-insensitive substring filter
}

pub async fn list_routes(
    State(state): State<AppState>,
    _:            AuthenticatedSession,
    Query(q):     Query<RouteListQuery>,
) -> Result<Json<Vec<RouteSummary>>, ApiError>;
```

### Algorithm

1. `list_snapshots`: clamp `limit` to `min(q.limit.unwrap_or(50), 200)`. Run `SELECT ... FROM snapshots ORDER BY config_version DESC LIMIT ? [WHERE id < ?]` using the cursor.
2. `get_snapshot`: `Storage::get_snapshot(&id)`. Returns the full row including `desired_state_json`. The body is the JSON document (no envelope).
3. `diff_snapshots`: load both snapshots; reject 404 if either is missing. Run `DiffEngine::structural_diff`. Run the redactor (Phase 6). Return the redacted diff and the site count.
4. `list_routes`: load `Storage::latest_desired_state()`; iterate over `desired_state.routes` (a `BTreeMap<RouteId, Route>`, naturally ordered). Apply the `cursor_after` filter (skip routes with `id <= cursor_after`). Apply the `hostname_filter` substring match (case-insensitive across the route's hostnames). Take `min(limit.unwrap_or(100), 500)` rows. For each, build `RouteSummary { upstream_count: route.upstreams.len() as u32, policy_attached: route.policy_attachment.as_ref().map(...) }`.

### Tests

- `core/crates/adapters/tests/snapshots_list_pagination.rs` — 250 snapshots; default limit returns 50; explicit `limit=200` returns 200; `limit=500` clamps to 200; cursor walks consistently.
- `core/crates/adapters/tests/snapshots_get_404_when_missing.rs` — unknown id returns 404.
- `core/crates/adapters/tests/snapshots_diff_redacts_secrets.rs` — diff between two states differing only in a basic-auth password; assert the response carries `***` and never the plaintext.
- `core/crates/adapters/tests/snapshots_unauthenticated_401.rs` — without a session, every endpoint returns 401.
- `core/crates/adapters/tests/routes_list_pagination.rs` — seed 1,200 routes via mutations; default returns 100; `limit=500` returns 500; `limit=900` clamps to 500; cursor walks the entire set without duplicates or skips.
- `core/crates/adapters/tests/routes_list_hostname_filter.rs` — seed routes with hostnames `api.example.com`, `admin.example.com`, `web.acme.io`; assert `hostname_filter=example` returns the first two; case-insensitive match.
- `core/crates/adapters/tests/routes_list_unauthenticated_401.rs` — without a session, returns 401.

### Acceptance command

`cargo test -p trilithon-adapters snapshots_ routes_list_`

### Exit conditions

- Every endpoint MUST require authentication.
- The diff endpoint MUST never return plaintext secret bytes.

### Audit kinds emitted

None.

### Tracing events emitted

`http.request.received`, `http.request.completed`.

### Cross-references

- PRD T1.2.
- Architecture §6.5, §6.6.

---

## Slice 9.9 [standard] — `GET /api/v1/audit` with paginated filters

### Goal

Expose the Phase 6 audit query API over HTTP. Filters: `since` and `until` (unix seconds), `actor_id`, `event` (a §6.6 wire kind), `correlation_id`. Pagination: `limit` default 100, max 1000; cursor is `cursor_before` (an audit row id).

### Entry conditions

- Slice 9.6 done.
- Phase 6 ships `Storage::tail_audit_log` (slice 6.6).

### Files to create or modify

- `core/crates/adapters/src/http_axum/audit_routes.rs` — handler.

### Signatures and shapes

```rust
#[derive(serde::Deserialize)]
pub struct AuditListQuery {
    pub since:           Option<i64>,
    pub until:           Option<i64>,
    pub actor_id:        Option<String>,
    pub event:           Option<String>,    // §6.6 wire kind
    pub correlation_id:  Option<String>,    // ULID
    pub limit:           Option<u32>,
    pub cursor_before:   Option<String>,    // audit row id (ULID)
}

#[derive(serde::Serialize)]
pub struct AuditRowResponse {
    pub id:              String,
    pub correlation_id:  String,
    pub occurred_at:     i64,
    pub occurred_at_ms:  i64,
    pub actor_kind:      String,
    pub actor_id:        String,
    pub event:           String,
    pub target_kind:     Option<String>,
    pub target_id:       Option<String>,
    pub snapshot_id:     Option<String>,
    pub redacted_diff_json: Option<serde_json::Value>,
    pub redaction_sites: u32,
    pub outcome:         String,
    pub error_kind:      Option<String>,
    pub notes:           Option<String>,
}

pub async fn list_audit(
    State(state): State<AppState>,
    _:            AuthenticatedSession,
    Query(q):     Query<AuditListQuery>,
) -> Result<Json<Vec<AuditRowResponse>>, ApiError>;
```

### Algorithm

1. Validate `q.event` against `AuditEvent::from_str`; on miss return 400.
2. Build `AuditSelector` from the query. Clamp `limit` per Phase 6's normalisation rule.
3. Call `Storage::tail_audit_log(selector, limit).await`.
4. Map each row to `AuditRowResponse`. Return as a JSON array.

### Tests

- `core/crates/adapters/tests/audit_list_default_limit.rs` — 250 rows; default returns 100.
- `core/crates/adapters/tests/audit_list_max_limit_clamped.rs` — `limit=5000` returns at most 1000.
- `core/crates/adapters/tests/audit_list_event_filter.rs` — `event=config.applied` returns only those rows.
- `core/crates/adapters/tests/audit_list_correlation_filter.rs` — assert the join on a single correlation id.
- `core/crates/adapters/tests/audit_list_unknown_event_400.rs` — `event=not.a.kind` returns 400.

### Acceptance command

`cargo test -p trilithon-adapters audit_list_`

### Exit conditions

- Default page size MUST be 100; max MUST be 1000.
- Unknown event filters MUST return 400.

### Audit kinds emitted

None.

### Tracing events emitted

`http.request.received`, `http.request.completed`.

### Cross-references

- PRD T1.7.
- Architecture §6.6.

---

## Slice 9.10 [standard] — Drift endpoints: current, adopt, reapply, defer

### Goal

Expose Phase 8's drift state over HTTP. `GET /api/v1/drift/current` returns the most recent unresolved drift event (or 204 if clean). `POST /api/v1/drift/{event_id}/adopt`, `/reapply`, `/defer` invoke the corresponding Phase 8 resolver and write `config.drift-resolved`.

### Entry conditions

- Slice 9.6 done.
- Phase 8 ships the resolvers and `DriftDetector::mark_resolved`.

### Files to create or modify

- `core/crates/adapters/src/http_axum/drift_routes.rs` — four handlers.

### Signatures and shapes

```rust
#[derive(serde::Serialize)]
pub struct DriftCurrentResponse {
    pub event_id:           String,        // audit row id of the most recent config.drift-detected
    pub correlation_id:     String,
    pub before_snapshot_id: String,
    pub running_state_hash: String,
    pub redacted_diff_json: serde_json::Value,
    pub redaction_sites:    u32,
    pub detected_at:        i64,
}

pub async fn current(
    State(state): State<AppState>,
    _:            AuthenticatedSession,
) -> Result<Result<Json<DriftCurrentResponse>, http::StatusCode /* 204 */>, ApiError>;

pub async fn adopt(
    State(state): State<AppState>,
    _:            AuthenticatedSession,
    Path(id):     Path<String>,
) -> Result<Json<MutationResponse>, ApiError>;

pub async fn reapply(/* same shape */) -> Result<Json<MutationResponse>, ApiError>;
pub async fn defer  (/* same shape */) -> Result<http::StatusCode /* 204 */, ApiError>;
```

### Algorithm

1. `current`:
   1. `Storage::latest_drift_event()`. If `None`, return 204.
   2. Otherwise, build the response from the row.
2. `adopt`/`reapply`/`defer`:
   1. Look up the drift event by `event_id`. If absent, 404.
   2. Construct the resolver mutation per Phase 8 slice 8.4.
   3. Submit through the same pipeline as `POST /api/v1/mutations` (slice 9.7) but with an `expected_version` derived from the latest committed snapshot (the resolver constructs the mutation in a way that targets the current pointer).
   4. On success, call `DriftDetector::mark_resolved(correlation_id, ResolutionKind::{Adopt|Reapply|Defer})`. The detector writes the `config.drift-resolved` audit row.
   5. `adopt` and `reapply` return the standard `MutationResponse`. `defer` returns 204.

### Tests

- `core/crates/adapters/tests/drift_current_204_when_clean.rs` — clean state; 204.
- `core/crates/adapters/tests/drift_current_returns_event.rs` — induce drift; assert the event is returned.
- `core/crates/adapters/tests/drift_adopt_writes_resolved_row.rs` — assert one `config.drift-resolved` row with `notes.resolution = "adopt"`.
- `core/crates/adapters/tests/drift_reapply_writes_resolved_row.rs` — same with `reapply`.
- `core/crates/adapters/tests/drift_defer_writes_resolved_row.rs` — same with `defer`; the response is 204.

### Acceptance command

`cargo test -p trilithon-adapters drift_current_ drift_adopt_ drift_reapply_ drift_defer_`

### Exit conditions

- Each resolution endpoint MUST produce exactly one `config.drift-resolved` audit row.
- `current` MUST return 204 when no drift is unresolved.

### Audit kinds emitted

`config.drift-resolved`, plus the standard mutation kinds when adopt or reapply produces an apply.

### Tracing events emitted

`drift.resolved` (Phase 8).

### Cross-references

- PRD T1.4.
- Architecture §6.6, §7.2.

---

## Slice 9.11 [cross-cutting] — `GET /api/v1/capabilities` plus error envelope and OpenAPI publication

### Goal

Return the cached capability probe payload at `GET /api/v1/capabilities`. Standardise the API error envelope. Generate the full OpenAPI 3.1 document via `utoipa` and serve it at `/api/v1/openapi.json`. The OpenAPI document is the source of truth for the Phase 11 typed client.

### Entry conditions

- Slices 9.7, 9.8, 9.9, 9.10 done.

### Files to create or modify

- `core/crates/adapters/src/http_axum/capabilities.rs` — handler.
- `core/crates/adapters/src/http_axum/openapi.rs` — `utoipa::OpenApi` derive root.
- `core/crates/adapters/src/http_axum/error.rs` — `ApiError` and the error envelope.

### Signatures and shapes

```rust
#[derive(Debug, thiserror::Error, serde::Serialize)]
#[serde(tag = "code", content = "detail")]
pub enum ApiError {
    #[error("unauthenticated")]
    Unauthenticated,
    #[error("forbidden: {reason}")]
    Forbidden { reason: String },
    #[error("not-found")]
    NotFound,
    #[error("conflict: stale expected_version")]
    Conflict { current_version: i64, expected_version: i64 },
    #[error("unprocessable: {detail}")]
    Unprocessable { detail: String },
    #[error("rate-limited")]
    RateLimited { retry_after_seconds: u32 },
    #[error("internal: {detail}")]
    Internal { detail: String },
}

impl axum::response::IntoResponse for ApiError { /* maps to status + JSON body */ }

#[derive(serde::Serialize)]
pub struct CapabilitiesResponse {
    pub caddy_version:   String,
    pub probed_at:       i64,
    pub modules:         Vec<String>,
    pub has_rate_limit:  bool,
    pub has_waf:         bool,
}

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        crate::http_axum::health::health,
        crate::http_axum::auth_routes::login,
        crate::http_axum::auth_routes::logout,
        crate::http_axum::auth_routes::change_password,
        crate::http_axum::mutations::post_mutation,
        crate::http_axum::snapshots::list_snapshots,
        crate::http_axum::snapshots::get_snapshot,
        crate::http_axum::snapshots::diff_snapshots,
        crate::http_axum::audit_routes::list_audit,
        crate::http_axum::drift_routes::current,
        crate::http_axum::drift_routes::adopt,
        crate::http_axum::drift_routes::reapply,
        crate::http_axum::drift_routes::defer,
        crate::http_axum::capabilities::get_capabilities,
    ),
    components(schemas(/* every request and response type */))
)]
pub struct ApiDoc;
```

### Algorithm

1. The `ApiError::IntoResponse` impl maps:
   - `Unauthenticated` → 401
   - `Forbidden` → 403
   - `NotFound` → 404
   - `Conflict` → 409
   - `Unprocessable` → 422
   - `RateLimited` → 429 with `Retry-After`
   - `Internal` → 500
   The body is `{ code: "...", detail: "..." }` for every variant.
2. `GET /api/v1/capabilities`: reads the latest row from `capability_probe_results` where `is_current = 1`; serialises as `CapabilitiesResponse`. If absent, returns 503 with `{ code: "capability-probe-pending" }`.
3. `GET /api/v1/openapi.json`: returns `ApiDoc::openapi().to_pretty_json()` with `Content-Type: application/json`.

### Tests

- `core/crates/adapters/tests/capabilities_returns_cached.rs` — seed a row; assert the response payload.
- `core/crates/adapters/tests/capabilities_503_when_unprobed.rs` — empty table; 503.
- `core/crates/adapters/tests/openapi_validates_against_3_1_schema.rs` — fetch the document, parse with `oas3` or equivalent; assert no validation errors.
- `core/crates/adapters/tests/openapi_documents_every_handler.rs` — assert the document's `paths` set matches the static list of routes registered in `router()`.
- `core/crates/adapters/tests/error_envelope_round_trip.rs` — every `ApiError` variant produces the documented status and body.

### Acceptance command

`cargo test -p trilithon-adapters capabilities_ openapi_ error_envelope_`

### Exit conditions

- Every handler in slices 9.5–9.10 MUST appear in the OpenAPI document.
- The error envelope MUST be uniform across all endpoints.
- `GET /api/v1/openapi.json` MUST validate against the OpenAPI 3.1 schema.

### Audit kinds emitted

None directly.

### Tracing events emitted

`http.request.received`, `http.request.completed`.

### Cross-references

- PRD T1.11, T1.13.
- Architecture §6.13, §11.

---

## Phase exit checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] No mutation endpoint is reachable without an authenticated session or a valid tool-gateway token (slices 9.6, 9.7).
- [ ] Sessions are stored server-side and revocable via `POST /auth/logout` and via `revoke_all_for_user` (slices 9.3, 9.5).
- [ ] The bootstrap account flow satisfies every clause of H13 (slice 9.4).
- [ ] Loopback-only binding is the default; remote binding requires an explicit flag and logs a warning (slice 9.1).
- [ ] `GET /api/v1/health` returns 200 within 5 seconds of `trilithon run` (slice 9.1).
- [ ] The OpenAPI document is published, validates, and documents every Tier 1 endpoint (slice 9.11).
- [ ] `docs/api/README.md` links the OpenAPI document and describes authentication, loopback default, and the bootstrap flow.

## Open questions

- The §6.6 vocabulary lists `mutation.applied`, `mutation.submitted`, and `mutation.conflicted`, but not a dedicated `mutation.created`. Slice 9.7 writes `mutation.applied` on success; whether a separate `mutation.submitted` row should fire at enqueue time is consistent with architecture §7.1 step 2 but doubles the audit-row volume per request. The breakdown defers the call to the project owner; a single `mutation.applied` plus the applier's `config.applied` is the conservative interpretation.
- Bearer-token rate limiting (`tokens.rate_limit_qps`) is out of scope for Phase 9; it lands in Phase 19/20 with the tool gateway.
- Remote binding's authentication-required posture is enforced by the middleware in slice 9.6; a future slice may add mTLS for the remote case. For V1, password authentication suffices per PRD T1.14.
