# Phase 9 — Security Review Findings

**Reviewer:** security
**Date:** 2026-05-15
**Diff range:** fd5127f..HEAD
**Phase:** 9

---

[HIGH] INTERNAL ERROR DETAILS EXPOSED IN API RESPONSES
File: core/crates/adapters/src/http_axum/auth_routes.rs
Lines: 2131-2135
Description: ApiError::Internal(msg) renders the raw internal error string directly into the 500 response body. Internal error messages from sqlx, Argon2, and serde_json can contain schema hints, file paths, or query fragments.
Suggestion: Log the internal message at ERROR with a correlation id; return a generic {"code":"internal","ref":"<correlation-id>"} body. Fix the IntoResponse impl centrally.

[HIGH] SESSION COOKIE NEVER SETS Secure FLAG — HARDCODED FALSE
File: core/crates/adapters/src/http_axum/auth_routes.rs
Lines: 2157-2164
Description: `set_cookie_header` always calls `build_cookie(..., false)`. Any deployment that terminates TLS at a proxy and forwards plain HTTP to this daemon sets session cookies without `Secure`, making them transmissible over plain HTTP.
Suggestion: Add `secure_cookies: bool` to `AxumServerConfig` (default false for loopback, true when `allow_remote_binding = true`). Thread through AppState and pass to `build_cookie`.

[HIGH] RATE LIMITER BYPASSED BY PROXY — SINGLE SHARED BUCKET ON 127.0.0.1
File: core/crates/adapters/src/http_axum/auth_routes.rs
Lines: 2204, 2221, 2241, 2256
Description: Rate limiter keys on addr.ip() from ConnectInfo<SocketAddr>. Behind a reverse proxy all logins originate from 127.0.0.1 — one shared bucket for all clients. Five failures from any IP locks out all users.
Suggestion: Read X-Forwarded-For or X-Real-IP when a trusted_proxy config flag is set. Use the outermost client IP as the rate-limit key.

[WARNING] SHA-256 USED FOR BEARER TOKEN STORAGE — NO HMAC OR PEPPER
File: core/crates/adapters/src/http_axum/auth_middleware.rs
Lines: 1848-1850
Description: Bearer tokens stored as SHA256(raw_token). SHA-256 is fast and unkeyed — a stolen tokens table is preimage-searchable at GPU speed.
Suggestion: Use HMAC-SHA256 keyed by a server-side secret, or document that tokens must be ≥128 bits of entropy making brute-force infeasible.

[WARNING] MISSING DISABLED-ACCOUNT CHECK FOR BEARER TOKEN PATH
File: core/crates/adapters/src/http_axum/auth_middleware.rs
Lines: 1960-1986
Description: try_bearer_token selects tokens by token_hash and revoked_at IS NULL but does not check whether the associated user account is disabled. Bearer tokens issued to subsequently-disabled users remain valid.
Suggestion: Join tokens against users on user_id and reject tokens where users.disabled_at IS NOT NULL.

[WARNING] must_change_pw ENFORCEMENT ONLY APPLIES TO SESSION AUTH
File: core/crates/adapters/src/http_axum/auth_middleware.rs
Lines: 1922-1931
Description: The must_change_pw gate only fires for AuthContext::Session. Bearer token auth bypasses the forced-password-change wall.
Suggestion: Store must_change_pw in AuthContext::Token (via users join) and apply the same gate for token auth.

[WARNING] LOGIN EMITS DIFFERENT AUDIT ACTORS FOR MISSING vs WRONG-PASSWORD — USERNAME ENUMERATION
File: core/crates/adapters/src/http_axum/auth_routes.rs
Lines: 2224-2251
Description: Failed login with unknown username records ActorRef::System; failed login with valid username but wrong password records ActorRef::User. Audit log readers can enumerate valid usernames by observing the actor field.
Suggestion: Use uniform ActorRef::System { component: "auth" } for all failed login audit rows regardless of whether the username was found.

[SUGGESTION] BOOTSTRAP FILE NOT CREATED ATOMICALLY ON NON-UNIX
File: core/crates/adapters/src/auth/bootstrap.rs
Lines: 379-388
Description: On non-Unix platforms std::fs::File::create is used without create_new, so the file can be overwritten if it already exists. The Unix path uses create_new(true) correctly.
Suggestion: Use OpenOptions::new().create_new(true).write(true) on non-Unix platforms as well.

[SUGGESTION] TOKEN RATE LIMIT QPS READ FROM DB BUT NEVER ENFORCED
File: core/crates/adapters/src/http_axum/auth_middleware.rs
Lines: 1978-1984
Description: rate_limit_qps is fetched from tokens table and stored in AuthContext::Token but no middleware enforces it. Tokens configured with qps=1 can make unlimited requests.
Suggestion: Implement a per-token leaky bucket in AppState using DashMap<String, TokenBucket>, similar to LoginRateLimiter.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-16 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Internal error details exposed in API responses | 🔕 Superseded | — | — | — | F007: already fixed during Phase 9 — Internal logs server-side, returns generic body |
| 2 | Session cookie never sets Secure flag | ✅ Fixed | `2386697` | — | 2026-05-16 | F008 |
| 3 | Rate limiter bypassed by proxy | ✅ Fixed | `2386697` | — | 2026-05-16 | F009 + F010 |
| 4 | SHA-256 used for bearer token storage — no HMAC | ✅ Fixed | `2386697` | — | 2026-05-16 | F019: doc added explaining 256-bit entropy makes brute-force infeasible |
| 5 | Missing disabled-account check for bearer token | ⏭️ Deferred | — | — | — | F012 area; Phase 10+ |
| 6 | must_change_pw bypassed by token auth | ⏭️ Deferred | — | — | — | F012 area; Phase 10+ |
| 7 | Login audit actor reveals username enumeration | ✅ Fixed | `2386697` | — | 2026-05-16 | F018 |
| 8 | Bootstrap file not atomic on non-Unix | ✅ Fixed | `cf1359c` | — | 2026-05-16 | F040 |
| 9 | Token rate_limit_qps never enforced | ⏭️ Deferred | — | — | — | F039: significant new feature; Phase 10+ |
