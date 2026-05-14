## Slice 9.1 — Axum HTTP Server Scaffold
**Status:** complete
**Date:** 2026-05-14
**Commit:** cd2e29e

### Implementation Summary
Stood up an axum-based HTTP server in the adapters crate. Defined the
`HttpServer` trait and `HttpServerError` enum in `core/crates/core/src/http.rs`.
Implemented `AxumServer` with `AppState`, `AxumServerConfig`, health handler,
and OpenAPI placeholder in `core/crates/adapters/src/http_axum.rs`. Registered
`GET /api/v1/health` (200 when ready, 503 while starting) and
`GET /api/v1/openapi.json` (static placeholder). Enforced H1 security
mitigation: remote binding rejected unless `allow_remote_binding = true`,
with a `tracing::warn!` emitted when the flag is set.

Wired the server into `run_with_shutdown` via `bind_and_spawn_http` helper in
`crates/cli/src/run.rs`.

### Simplify Findings
Items fixed inline during implementation:

- `json!` macro uses `unwrap` internally — replaced with manual
  `serde_json::Map` construction to satisfy `clippy::disallowed_methods`
- `.map(|ip| ip.is_loopback()).unwrap_or(false)` replaced with
  `.is_ok_and(|ip| ip.is_loopback())` (clippy::map_unwrap_or)
- `AxumServer::new` made `const` (clippy::missing_const_for_fn)
- `run_with_shutdown` exceeded 100-line limit after HTTP wiring — extracted
  `bind_and_spawn_http` helper (clippy::too_many_lines)
- `use trilithon_core::http::HttpServer as _;` moved to module level
  (clippy::items_after_statements)
- Doc comments: added backticks to `OpenAPI`, `BindFailed` references
  (clippy::doc_markdown)
- `tracing::warn!` custom target field removed — caused tracing-test env
  filter to miss the event
- tracing-test feature `no-env-filter` enabled so events from
  `trilithon_adapters` are captured in integration tests (default filter
  only captures events from the test crate itself)
- `raw_logs_contain` helper added to bypass scope-filtering in
  tracing-test — necessary because `span.enter()` guards do not
  persist across `.await` suspension points in async tests
- MutexGuard lifetime narrowed in `raw_logs_contain` to satisfy
  `clippy::significant_drop_tightening`

### Items Left Unfixed
None — all findings were fixed inline.

## Slice 9.4
**Status:** complete
**Date:** 2026-05-14
**Commit:** 1100cb5
**Summary:** Implemented `bootstrap_if_empty` in `auth/bootstrap.rs`. Generates a 24-char Crockford base32 password from 18 random bytes, writes `bootstrap-credentials.txt` with mode 0600, emits `auth.bootstrap-credentials-created` audit row with no password in diff. Four integration tests (file mode, must_change_pw, password not in logs, skips when users exist) all pass. The `run_with_shutdown` function was refactored via `setup_caddy` helper to stay under the 100-line clippy limit.

### Simplify Findings
- Extracted `setup_caddy` helper from `run_with_shutdown` to reduce function length from ~150 to ~90 lines (required by `clippy::too_many_lines`).
- Changed `encode_password` to build a `String` directly using `char::from(u8)` instead of collecting bytes and calling `.expect()` (required by `clippy::expect_used`).

### Items Fixed Inline
- `clippy::expect_used` on `String::from_utf8(out).expect(...)` in `encode_password` — replaced with `char::from` push approach.
- Unused import `std::fmt::Write as _` inside `on_event` in `bootstrap_password_not_in_logs.rs` test.
- `clippy::too_many_lines` on `run_with_shutdown` (101 lines) — extracted `setup_caddy` helper.
- `pub use sqlx::sqlite::SqlitePool` added to `sqlite_storage.rs` so `cli` can reference the pool type without a direct `sqlx` dependency.

### Items Left Unfixed
None.

## Slice 9.5
**Status:** complete
**Date:** 2026-05-14
**Commit:** c2ba104
**Summary:** Implemented `POST /api/v1/auth/login`, `/logout`, and `/change-password` in `core/crates/adapters/src/http_axum/auth_routes.rs`. Login enforces rate limiting, verifies Argon2id password hash, creates session cookie, emits `auth.login-succeeded` or `auth.login-failed` audit rows, and returns 409 with `must-change-password` code when the flag is set. Logout revokes the session and clears the cookie. Change-password verifies the old hash, validates minimum length and no-reuse, updates the hash, clears `must_change_pw`, and revokes all other sessions. A stub `AuthenticatedSession` extractor (via `X-Session-Id`/`X-User-Id` headers) is provided for slice 9.6 to replace. All 6 acceptance tests pass.

### Simplify Findings
No over-abstraction or dead code found. The implementation is tightly scoped with no unnecessary layers.

### Items Fixed Inline
- `rate_limit.rs` threshold bug: `record_failure` set `next_allowed_at_unix` only when `failure_count > 5` (after the 6th failure), meaning `check()` was called before the block was active and the 6th attempt passed through as 401 instead of 429. Fixed to `>= 5` (after the 5th failure) so the 6th attempt is correctly rejected with 429 and a `Retry-After` header.

### Items Left Unfixed
None.

## Slice 9.6
**Status:** complete
**Date:** 2026-05-14
**Commits:** d03de12 (main impl), 70792e6 (manifest update)

### Summary
Implemented Tower auth middleware (`auth_layer`) that resolves authentication from either a session cookie or an `Authorization: Bearer` header. Path classification (Public / MustChangePassword / Protected) enforces that unauthenticated protected requests return 401 with `{"code":"unauthenticated"}` and sessions with `must_change_pw=true` return 403 with `{"code":"must-change-password"}` on non-change-password routes. Added `tokens` table migration, `AuthContext` enum, `AuthenticatedSession` extractor, and replaced the prior stub extractor in `auth_routes.rs`. Updated all 9.5 tests to use real cookie auth.

### Simplify Findings
- Replaced redundant `match` blocks with `let...else` patterns as required by clippy.
- Added `Unauthenticated` and `Forbidden` variants to `ApiError` to distinguish middleware-level vs handler-level rejections.

### Items Fixed Inline
- Spec said 403 for `must_change_pw` blocked routes; initial implementation used 409 (`Conflict`). Corrected to add `ApiError::Forbidden` returning 403.
- Clippy `use-self` lint in `FromRequestParts`: replaced `.get::<AuthenticatedSession>()` with `.get::<Self>()`.
- Multiple `or_fun_call`, `manual_let_else`, and `doc_markdown` lints fixed per clippy guidance.

### Items Left Unfixed
None.
