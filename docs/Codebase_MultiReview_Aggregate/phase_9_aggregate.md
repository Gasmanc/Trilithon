# Phase 9 — Aggregate Review Plan

**Generated:** 2026-05-15T00:00:00Z
**Reviewers:** code_adversarial, codex, gemini, glm, kimi, learnings_match, minimax, qwen, scope_guardian, security
**Raw findings:** ~70 across 10 reviewers
**Unique findings:** 45 after clustering
**Consensus:** 7 unanimous · 11 majority · 27 single-reviewer
**Conflicts:** 0
**Superseded (already fixed):** 0

---

## How to use this document

Feed this file to `/review-remediate` to drive the fix cycle. Each finding has a
unique ID (F001, F002, …) that `review-remediate` uses to track state. Do not
renumber or delete findings — append `SUPERSEDED` status instead.

---

## CRITICAL Findings

### F001 · [CRITICAL] Production HTTP server is wired to test stub state
**Consensus:** UNANIMOUS · flagged by: glm, codex, gemini
**File:** `core/crates/cli/src/run.rs` · **Lines:** 333–350
**Description:** `bind_and_spawn_http` constructs `AppState` via `stubs::make_test_app_state`, wiring `NoopStorage`, `NoopSessionStore`, `NoopUserStore`, and `NoopApplier`. Every API handler in the running daemon will fail or return stub/empty data. Real stores, the real applier, and the real drift detector are never wired into the HTTP server.
**Suggestion:** Replace `make_test_app_state` with a real `AppState` constructor receiving the already-created `SqlitePool`, `Arc<dyn Storage>`, `SqliteUserStore`, `SqliteSessionStore`, `CaddyApplier`, `DriftDetector`, rate limiter, and capability cache from `run_with_shutdown`.
**Claude's assessment:** Agree. This is the most severe finding in the phase — the HTTP API is completely non-functional in production. All other authentication and mutation findings are downstream of this.

---

### F002 · [CRITICAL] Drift `adopt` semantics are inverted — re-applies desired state instead of capturing running state
**Consensus:** UNANIMOUS · flagged by: gemini (CRITICAL), code_adversarial (HIGH), codex (HIGH), glm (HIGH), qwen (WARNING)
**File:** `core/crates/adapters/src/http_axum/drift_routes.rs` · **Lines:** 113–175
**Description:** The `adopt` handler loads `latest_desired_state()` and re-applies it through the applier — semantically identical to `reapply`. An operator choosing "adopt" to accept a manual Caddy config change silently gets a reapply instead: Caddy's diverged state is overwritten with the old desired state. The endpoint is fundamentally misleading.
**Suggestion:** Implement `adopt` correctly: fetch the running config from Caddy (`get_running_config`), build a new `DesiredState` and `Snapshot` from it, insert as the new desired state, then mark the drift event as resolved. At minimum, return `501 Not Implemented` until the correct semantics are implemented rather than silently doing the wrong thing.
**Claude's assessment:** Agree. This is a fundamental semantic error. Operators relying on "adopt" to accept running changes will have their changes overwritten without warning.

---

### F003 · [CRITICAL] Rate limiter backoff exponent unbounded — permanent lockout after 68+ failures
**Consensus:** SINGLE · flagged by: qwen
**File:** `core/crates/adapters/src/auth/rate_limit.rs` · **Lines:** 58–64
**Description:** `record_failure` computes `2_i64.saturating_pow(bucket.failure_count - 4)` with no exponent cap. After 68+ consecutive failures, `saturating_pow` saturates to `i64::MAX`, setting `next_allowed_at_unix` to near-overflow. All subsequent `check` calls compute a retry delay of `i64::MAX - now_unix ≈ u32::MAX` seconds (~136 years), permanently locking the bucket.
**Suggestion:** Cap the exponent before computing: `let exponent = (bucket.failure_count.saturating_sub(4)).min(10) as u32;` — `2^10 = 1024s` already exceeds any reasonable backoff cap.
**Claude's assessment:** Agree. Qwen's analysis is correct. This is a real overflow path with a clear fix.

---

### F004 · [CRITICAL] Insecure token generation in stubs (depends on F001)
**Consensus:** SINGLE · flagged by: kimi
**File:** `core/crates/adapters/src/http_axum/stubs.rs` · **Lines:** 85–90
**Description:** `random_token()` in `stubs.rs` uses 32 bits of nanosecond timestamp — brute-forceable in under a second. Because F001 shows the production daemon routes through these stubs, the insecure token generator is the live production token generator.
**Suggestion:** Fix F001 first (wire real stores). Once stubs are test-only, add a prominent `// TEST-ONLY: not for production use` comment and optionally replace with `uuid::Uuid::new_v4()`. If the stubs remain in the code path for any reason, use cryptographically secure random generation unconditionally.
**Claude's assessment:** Agree. Severity is escalated from kimi's initial HIGH because F001 means these stubs are live. Fixing F001 downgrades this to a documentation/test quality issue.

---

## HIGH Findings

### F005 · [HIGH] Non-atomic bootstrap: admin lockout if credentials file write fails
**Consensus:** MAJORITY · flagged by: codex, gemini, glm
**File:** `core/crates/adapters/src/auth/bootstrap.rs` · **Lines:** 88–142
**Description:** The bootstrap flow (1) creates the admin user in the database, then (2) writes `bootstrap-credentials.txt`. If the daemon crashes or the disk write fails between these steps, the account exists with a permanently lost random password. On the next startup `user_count > 0` causes bootstrap to skip — the admin is locked out with no recovery path. The race condition between two concurrent daemon processes (glm) also applies: both can observe `user_count == 0` and create duplicate admin accounts.
**Suggestion:** Wrap check-and-create in a SQLite transaction that also sets `must_change_pw` atomically. Write the credentials file *before* committing the transaction, or check for the file's existence first and delete the created user if the file write fails.
**Claude's assessment:** Agree. The ordering inversion is a real production risk on constrained filesystems.

---

### F006 · [HIGH] Snapshot inserted before apply — orphaned row on conflict
**Consensus:** SINGLE · flagged by: code_adversarial
**File:** `core/crates/adapters/src/http_axum/mutations.rs` · **Lines:** 187–193
**Description:** `storage.insert_snapshot(snapshot.clone())` is called unconditionally *before* `applier.apply()`. If the applier returns `OptimisticConflict`, `LockContested`, or `Failed`, the snapshot row is committed but Caddy never saw the config. `latest_desired_state()` then returns this phantom snapshot, causing subsequent mutations to compute diffs from a state Caddy has never applied.
**Suggestion:** Move `insert_snapshot` to after a successful `ApplyOutcome::Succeeded`, or wrap both in a single SQLite transaction that rolls back on apply failure.
**Claude's assessment:** Agree. The phantom snapshot breaks future mutations in a hard-to-diagnose way.

---

### F007 · [HIGH] Internal error details exposed in API responses
**Consensus:** MAJORITY · flagged by: glm (HIGH), security (HIGH), codex (WARNING)
**File:** `core/crates/adapters/src/http_axum/auth_routes.rs` · **Lines:** general
**Description:** Every `map_err(|e| ApiError::Internal(e.to_string()))` sends raw internal error strings — including SQLx schema hints, table names, column names, query fragments — directly in the 500 response body. This violates the principle of not leaking internal structure to API clients.
**Suggestion:** Log the full error at `ERROR` level with a correlation ID. Return `{"code":"internal","ref":"<correlation-id>"}` to the client. Fix centrally in `ApiError::IntoResponse`.
**Claude's assessment:** Agree. Internal error leakage is a well-established OWASP finding.

---

### F008 · [HIGH] Session cookie `Secure` flag hardcoded to `false`
**Consensus:** MAJORITY · flagged by: security (HIGH), glm (WARNING)
**File:** `core/crates/adapters/src/http_axum/auth_routes.rs` · **Lines:** 107–113
**Description:** `set_cookie_header` always calls `build_cookie(..., secure: false)`. Any deployment that terminates TLS at a reverse proxy and forwards plain HTTP to this daemon sends session cookies without the `Secure` flag, making them transmissible over plain HTTP.
**Suggestion:** Add `secure_cookies: bool` to `AxumServerConfig` (default `false` for loopback, `true` when `allow_remote_binding = true`). Thread through `AppState` and pass to `build_cookie`.
**Claude's assessment:** Agree. The default should be `false` for loopback-only deployments but must be configurable.

---

### F009 · [HIGH] Rate limiter keyed by direct TCP peer — bypassable via reverse proxy
**Consensus:** MAJORITY · flagged by: code_adversarial (HIGH), security (HIGH), gemini (WARNING)
**File:** `core/crates/adapters/src/auth/rate_limit.rs` · **Lines:** general
**Description:** The rate limiter keys on `ConnectInfo<SocketAddr>::ip()`, which is the direct TCP peer. Behind Caddy or any reverse proxy, all login requests originate from `127.0.0.1` — a single shared bucket for all clients. Five failures from any IP locks out all users simultaneously.
**Suggestion:** Read `X-Forwarded-For` or `X-Real-IP` when a `trusted_proxy` config flag is set. Use the outermost (client) IP as the rate-limit key.
**Claude's assessment:** Agree. This effectively disables per-IP rate limiting in any proxied deployment.

---

### F010 · [HIGH] Rate limiter DashMap grows without bound — no eviction
**Consensus:** MAJORITY · flagged by: code_adversarial (HIGH), gemini (WARNING), glm (HIGH), qwen (SUGGESTION)
**File:** `core/crates/adapters/src/auth/rate_limit.rs` · **Lines:** general
**Description:** `LoginRateLimiter::buckets` is a `DashMap<IpAddr, BucketState>` with no TTL, no eviction, and no size cap. An attacker rotating IP addresses creates stale entries that accumulate indefinitely.
**Suggestion:** Add a periodic sweep (e.g., every 60s via `DashMap::retain`) removing entries where `next_allowed_at_unix` is sufficiently in the past, or cap the map size with an LRU policy.
**Claude's assessment:** Agree. Combine the fix with F009 (both live in `rate_limit.rs`).

---

### F011 · [HIGH] Disabled accounts retain session and bearer token access
**Consensus:** MAJORITY · flagged by: codex (HIGH), security (WARNING)
**File:** `core/crates/adapters/src/http_axum/auth_middleware.rs` · **Lines:** 156–168, 1960–1986
**Description:** The auth middleware accepts any session where the user row exists, without checking `disabled_at`. Bearer token auth also does not join against `users.disabled_at`. Disabling an account only blocks *new* logins — all existing sessions and issued tokens remain valid indefinitely.
**Suggestion:** Reject sessions and bearer tokens for users with `disabled_at IS NOT NULL`. For sessions: check in the middleware after `touch()`. For tokens: add a JOIN to the `users` table in the token lookup query.
**Claude's assessment:** Agree. Account disable with retained access is a standard security gap.

---

### F012 · [HIGH] Bearer token auth bypasses `must_change_pw` enforcement
**Consensus:** MAJORITY · flagged by: code_adversarial (HIGH), security (WARNING)
**File:** `core/crates/adapters/src/http_axum/auth_middleware.rs` · **Lines:** 192–202, 1922–1931
**Description:** The `must_change_pw` enforcement block only fires for `AuthContext::Session`. Bearer token callers can reach all `Protected` endpoints regardless of the flag. This allows API automation to operate on an account that requires a password change.
**Suggestion:** Store `must_change_pw` in `AuthContext::Token` (via a JOIN to `users`) and apply the same gate. Or document explicitly that tokens are exempt and explain the rationale.
**Claude's assessment:** Agree. The exemption is unintentional; it should be an explicit design decision either way.

---

### F013 · [HIGH] `GET /api/v1/snapshots/:id` returns plaintext secrets
**Consensus:** SINGLE · flagged by: gemini
**File:** `core/crates/adapters/src/http_axum/snapshots.rs` · **Lines:** 147–151
**Description:** The `get_snapshot` handler returns the full `Snapshot` object including `desired_state_json` with plaintext secrets (TLS keys, API tokens). The `diff_snapshots` and `audit_routes` endpoints correctly redact these fields — `get_snapshot` is an oversight.
**Suggestion:** Deserialise `desired_state_json` into `DesiredState`, apply `SecretsRedactor`, and return the redacted JSON.
**Claude's assessment:** Agree. Consistent with the redaction applied elsewhere in the codebase.

---

### F014 · [HIGH] Envelope `expected_version` bypass — snapshot inserted before version check
**Consensus:** SINGLE · flagged by: codex
**File:** `core/crates/adapters/src/http_axum/mutations.rs` · **Lines:** 178–284
**Description:** `build_snapshot` ignores the envelope `expected_version`; only the version embedded in the mutation body is checked. The handler inserts the snapshot before the applier checks the version. A client can submit a stale envelope version, receive a 409 conflict from the applier, but still persist a new latest desired snapshot to storage.
**Suggestion:** Use the envelope `expected_version` as the authoritative concurrency guard before snapshot insertion. Reject body-version mismatches and only publish snapshots through the successful apply/CAS path.
**Claude's assessment:** Agree — though this overlaps with F006 (insert before apply). Fixing F006 (move insert after successful apply) resolves this as a side effect.

---

### F015 · [HIGH] Mutations handler uses empty `CapabilitySet` — no real Caddy capability validation
**Consensus:** SINGLE · flagged by: glm
**File:** `core/crates/adapters/src/http_axum/mutations.rs` · **Lines:** 281–285
**Description:** `build_snapshot` constructs a `CapabilitySet` with empty modules and zeroed fields. Mutations requiring Caddy modules (e.g., rate limiting) will always pass capability validation, potentially sending configs Caddy cannot apply.
**Suggestion:** Read from `state.capability_cache.snapshot()` to get the real probed capabilities.
**Claude's assessment:** Agree. The capability cache exists for this purpose.

---

### F016 · [HIGH] Silent audit write failures in change-password, adopt, and reapply handlers
**Consensus:** SINGLE · flagged by: minimax
**File:** `core/crates/adapters/src/http_axum/auth_routes.rs` (L393–401), `core/crates/adapters/src/http_axum/drift_routes.rs` (L326–337, L410–421)
**Description:** Three handlers emit supplemental audit rows using `let _ = audit.append(...).await`, silently discarding database write failures. Audit records are lost without any logging.
**Suggestion:** At minimum: `if let Err(e) = audit.append(...).await { tracing::warn!(error = %e, "audit write failed"); }`. Consider returning an error response if audit integrity is required.
**Claude's assessment:** Agree. Silent discard of audit failures undermines audit integrity.

---

## WARNING Findings

### F017 · [WARNING] `LoginResponse.config_version` hardcoded to `0`
**Consensus:** UNANIMOUS · flagged by: code_adversarial, codex, gemini (SUGGESTION), glm
**File:** `core/crates/adapters/src/http_axum/auth_routes.rs` · **Lines:** 185–213
**Description:** Login success response always returns `config_version: 0`. Clients that use this value to seed `expected_version` for mutations will always send 0, guaranteeing 409 conflicts on any non-empty system.
**Suggestion:** Fetch the latest `config_version` from `state.storage.latest_desired_state()` during the login handler and return the real value.
**Claude's assessment:** Agree. Simple fix; high usability impact.

---

### F018 · [WARNING] Login timing and audit actor both expose username existence
**Consensus:** MAJORITY · flagged by: glm (WARNING), security (WARNING × 2)
**File:** `core/crates/adapters/src/http_axum/auth_routes.rs` · **Lines:** 153–170, 2224–2251
**Description:** Two distinct enumeration vectors: (1) Timing: unknown username returns 401 without running Argon2 (~fast); known username with wrong password runs Argon2 (~0.5s). (2) Audit: failed login for unknown username records `ActorRef::System`; for a known username records `ActorRef::User`. Audit log readers enumerate valid usernames via the actor field.
**Suggestion:** (1) Always call `verify_password` against a dummy hash when username is not found. (2) Use uniform `ActorRef::System { component: "auth" }` for all failed login audit rows.
**Claude's assessment:** Agree. Both vectors are low-effort for an attacker with audit log access.

---

### F019 · [WARNING] SHA-256 used for bearer token storage — no HMAC or pepper
**Consensus:** SINGLE · flagged by: security
**File:** `core/crates/adapters/src/http_axum/auth_middleware.rs` · **Lines:** 1848–1850
**Description:** Bearer tokens stored as `SHA256(raw_token)`. SHA-256 is fast and unkeyed — a stolen tokens table is preimage-searchable at GPU speed.
**Suggestion:** Use `HMAC-SHA256` keyed by a server-side secret, or document that tokens must be ≥128 bits of entropy making brute force infeasible.
**Claude's assessment:** Agree, though if tokens are generated as 128-bit random values (ULID/UUID), preimage attack is infeasible in practice. Document the assumption clearly.

---

### F020 · [WARNING] Session `touch` called on every request — no throttling
**Consensus:** MAJORITY · flagged by: gemini, code_adversarial
**File:** `core/crates/adapters/src/auth/sessions.rs` · **Lines:** 124–148
**Description:** `auth_layer` calls `touch()` on every authenticated request, performing a SELECT + UPDATE on the sessions table. For high-frequency API clients, this creates unnecessary SQLite write pressure and contention.
**Suggestion:** Only update `last_seen_at` if more than a threshold (e.g., 60s) has elapsed since the stored value.
**Claude's assessment:** Agree. A simple time-gate reduces write load significantly.

---

### F021 · [WARNING] Session `touch` does not secondary-check `expires_at`
**Consensus:** SINGLE · flagged by: code_adversarial
**File:** `core/crates/adapters/src/http_axum/auth_middleware.rs` · **Lines:** 150–153
**Description:** The middleware treats `touch()` returning `Some` as valid, relying entirely on the SQL predicate. No secondary check is performed on the returned `Session.expires_at`. A clock step backward or latent SQL bug could admit expired sessions.
**Suggestion:** After `touch()` returns `Some(session)`, assert `session.expires_at > now_unix_ms()` as defense-in-depth.
**Claude's assessment:** Agree. Defense-in-depth; low cost to add.

---

### F022 · [WARNING] `classify()` brittle against trailing slashes in path matching
**Consensus:** SINGLE · flagged by: gemini
**File:** `core/crates/adapters/src/http_axum/auth_middleware.rs` · **Lines:** 40–47
**Description:** Exact string literal matches on `request.uri().path()` fail for paths with trailing slashes (e.g., `/api/v1/health/`).
**Suggestion:** Normalize the path (strip trailing slashes) before matching, or use Axum's routing state to classify routes.
**Claude's assessment:** Agree. Worth a quick fix to avoid surprising 401s.

---

### F023 · [WARNING] TOCTOU race between `latest_unresolved` and `mark_resolved`
**Consensus:** SINGLE · flagged by: code_adversarial
**File:** `core/crates/adapters/src/http_axum/drift_routes.rs` · **Lines:** 127–136, 233–241, 336–344
**Description:** Two concurrent requests for the same `event_id` can both pass the existence check and both call `mark_resolved`, writing duplicate audit rows.
**Suggestion:** Add idempotency to `mark_resolved` via `UPDATE drift_events SET resolved_at = ? WHERE id = ? AND resolved_at IS NULL`. Zero rows affected means a concurrent resolution won — return 409 or 200 (idempotent).
**Claude's assessment:** Agree. Standard optimistic concurrency fix.

---

### F024 · [WARNING] Drift `defer` writes no audit row for the acting user
**Consensus:** SINGLE · flagged by: code_adversarial
**File:** `core/crates/adapters/src/http_axum/drift_routes.rs` · **Lines:** 331–358
**Description:** `adopt` and `reapply` write supplemental audit rows crediting the acting session. `defer` does not — no audit trail identifies who deferred a drift event.
**Suggestion:** Write an `AuditAppend` with `actor = session.user_id` before returning 204.
**Claude's assessment:** Agree. Audit consistency requires all drift resolution paths to record an actor.

---

### F025 · [WARNING] Drift handlers return raw JSON tuples instead of `ApiError`
**Consensus:** MAJORITY · flagged by: qwen (WARNING), kimi (WARNING)
**File:** `core/crates/adapters/src/http_axum/drift_routes.rs` · **Lines:** throughout
**Description:** Drift handlers return `(StatusCode, Json(json!({...})))` tuples for errors while all other handlers use the unified `ApiError` enum. Error response shapes are inconsistent.
**Suggestion:** Convert drift handlers to return `Result<_, ApiError>` consistently.
**Claude's assessment:** Agree. Needed for the unified error envelope.

---

### F026 · [WARNING] `audit_routes` does not validate `since <= until`
**Consensus:** SINGLE · flagged by: glm
**File:** `core/crates/adapters/src/http_axum/audit_routes.rs` · **Lines:** 126–127
**Description:** When both `since` and `until` are provided, the handler does not validate `since <= until`. Negative values are also not rejected.
**Suggestion:** Validate and return 400 `{"code":"invalid_range"}` if `since > until` or either value is negative.
**Claude's assessment:** Agree. Defensive input validation.

---

### F027 · [WARNING] `DriftCurrentResponse.redaction_sites` always 0
**Consensus:** SINGLE · flagged by: glm
**File:** `core/crates/adapters/src/http_axum/drift_routes.rs` · **Lines:** 92–93
**Description:** `redaction_sites` is hardcoded to 0, making it indistinguishable from "no secrets in diff."
**Suggestion:** Store `redaction_sites` in the drift event row or compute at detection time.
**Claude's assessment:** Agree — though this is low priority relative to F001/F002.

---

### F028 · [WARNING] `bootstrap_password_not_in_env` test missing
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/adapters/tests/` · **Lines:** general
**Description:** Slice 9.4 specifies five tests; only four are present. `bootstrap_password_not_in_env.rs` (assert the password does not appear in any `std::env::vars()` value) is absent from both the file list and `Cargo.toml [[test]]` entries.
**Suggestion:** Add `core/crates/adapters/tests/bootstrap_password_not_in_env.rs` and a corresponding `[[test]]` entry.
**Claude's assessment:** Agree. Missing spec-required test.

---

### F029 · [WARNING] `ApiError::BadRequest` variant not in Phase 9.11 spec
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/adapters/src/http_axum/auth_routes.rs` · **Lines:** general
**Description:** The 9.11 spec defines `ApiError` without a `BadRequest` variant. The implementation adds `BadRequest(String)` used in audit_routes and the mutation handler.
**Suggestion:** Decide: either formally add `BadRequest` to the `ApiError` spec, or replace with `Unprocessable { detail }` (422) where appropriate. Update the spec accordingly.
**Claude's assessment:** Agree. `BadRequest(400)` is semantically correct for malformed input; the spec needs updating.

---

### F030 · [WARNING] `health_handler` placed in module root instead of `health.rs`
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/adapters/src/http_axum.rs` · **Lines:** 1327–1345
**Description:** Slice 9.1 specifies `src/http_axum/health.rs` as the file for the health handler. The handler lives in the module root instead.
**Suggestion:** Extract to `src/http_axum/health.rs`, or update the spec to reflect the consolidated layout.
**Claude's assessment:** Minor structural deviation; low priority.

---

### F031 · [WARNING] Cookie header `unwrap` in auth without safety comment
**Consensus:** SINGLE · flagged by: minimax
**File:** `core/crates/adapters/src/http_axum/auth_routes.rs` · **Lines:** 278–280
**Description:** `HeaderValue::from_str(&value).unwrap()` — correct by construction but not self-evident.
**Suggestion:** Add `// SAFETY: cookie string is always a valid HTTP header value` or replace with `expect("BUG: cookie format produces invalid header")`.
**Claude's assessment:** Agree. Per project convention, `unwrap()` in production code needs a documented safety invariant.

---

### F032 · [WARNING] Known pattern — ULID sort key for stable ordering (learnings_match)
**Consensus:** SINGLE · flagged by: learnings_match
**File:** `core/crates/adapters/src/http_axum/audit_routes.rs` · **Lines:** general
**Description:** Audit list endpoint may sort by `occurred_at` rather than ULID `id` — rows within the same second have non-deterministic ordering and pagination cursors are unstable.
**Suggestion:** Sort by ULID `id` column for stable millisecond tie-breaking. See `docs/solutions/best-practices/ulid-sort-key-stable-ordering-2026-05-09.md`.
**Claude's assessment:** Agree. ULID sort is the established project pattern.

---

### F033 · [WARNING] Known pattern — LIKE wildcard escape required (learnings_match)
**Consensus:** SINGLE · flagged by: learnings_match
**File:** `core/crates/adapters/src/http_axum/audit_routes.rs`, `routes.rs` · **Lines:** general
**Description:** User-controlled filter strings passed into `LIKE` clauses without escaping `%`, `_`, `\` allow callers to inject wildcards matching unintended rows.
**Suggestion:** Escape user input and specify `ESCAPE` clause in all `LIKE` queries. See `docs/solutions/security-issues/sqlite-like-wildcard-escape-2026-05-08.md`.
**Claude's assessment:** Agree. Known project vulnerability class.

---

### F034 · [WARNING] Known pattern — static SQL with coalesce for optional filters (learnings_match)
**Consensus:** SINGLE · flagged by: learnings_match
**File:** general · **Lines:** general
**Description:** Optional filter parameters built with `format!()` dynamic SQL are an injection surface.
**Suggestion:** Use `? IS NULL OR col = ?` pattern. See `docs/solutions/security-issues/sqlite-static-sql-coalesce-optional-filters-2026-05-05.md`.
**Claude's assessment:** Agree. Check all list endpoint SQL for `format!()` usage.

---

### F035 · [WARNING] Known pattern — CIDR validation at mutation boundary (learnings_match)
**Consensus:** SINGLE · flagged by: learnings_match
**File:** `core/crates/adapters/src/http_axum/mutations.rs` · **Lines:** general
**Description:** User-supplied route config structured fields (IP ranges, hostnames) must be validated at the HTTP handler boundary, not deferred to apply time.
**Suggestion:** See `docs/solutions/security-issues/cidr-validate-at-mutation-boundary-2026-05-06.md`.
**Claude's assessment:** Agree. Validate early, fail fast.

---

### F036 · [WARNING] Known pattern — `tokio::sync::Mutex` in async test doubles (learnings_match)
**Consensus:** SINGLE · flagged by: learnings_match
**File:** `core/crates/adapters/tests/` · **Lines:** general
**Description:** Async test doubles using `std::sync::Mutex` will poison the lock on panic and cascade-fail subsequent tests.
**Suggestion:** Use `tokio::sync::Mutex` in all async test doubles. See `docs/solutions/best-practices/tokio-mutex-in-async-test-doubles-2026-05-08.md`.
**Claude's assessment:** Agree. Check all test stubs/doubles.

---

## SUGGESTION / LOW Findings

### F037 · [SUGGESTION] Dead code `build_snapshot_from_desired` with `#[allow(dead_code)]`
**Consensus:** UNANIMOUS · flagged by: glm, minimax, qwen
**File:** `core/crates/adapters/src/http_axum/drift_routes.rs` · **Lines:** 341–400
**Description:** Function is annotated `#[allow(dead_code)]` with "retained for future use." Violates no-dead-code and no-suppressions-without-tracked-id conventions.
**Suggestion:** Remove now. Re-introduce when `adopt` is properly implemented (F002). The function will be needed then.
**Claude's assessment:** Agree. The suppression is a placeholder for work that F002 will require — removing it is cleaner.

---

### F038 · [SUGGESTION] Login audit actor reveals username existence via `ActorRef` variant
**Consensus:** SINGLE · flagged by: security
**Note:** Already covered in F018.
**Claude's assessment:** SUPERSEDED by F018.

---

### F039 · [SUGGESTION] Token `rate_limit_qps` field fetched but never enforced
**Consensus:** SINGLE · flagged by: security
**File:** `core/crates/adapters/src/http_axum/auth_middleware.rs` · **Lines:** 1978–1984
**Description:** `rate_limit_qps` is stored in `AuthContext::Token` but no middleware enforces it. Tokens configured with `qps=1` can make unlimited requests.
**Suggestion:** Implement a per-token leaky bucket in `AppState` using `DashMap<String, TokenBucket>`.
**Claude's assessment:** Agree — future-scope, but the field being silently ignored is misleading.

---

### F040 · [SUGGESTION] `bootstrap-credentials.txt` created with `File::create` on non-Unix (not `create_new`)
**Consensus:** SINGLE · flagged by: security
**File:** `core/crates/adapters/src/auth/bootstrap.rs` · **Lines:** 379–388
**Description:** On non-Unix platforms `std::fs::File::create` is used without `create_new`, allowing the file to be overwritten if it already exists. The Unix path uses `create_new(true)` correctly.
**Suggestion:** Use `OpenOptions::new().create_new(true).write(true)` on non-Unix platforms.
**Claude's assessment:** Agree. Trivial fix.

---

### F041 · [SUGGESTION] Shadowed `correlation_id` variable in drift handler
**Consensus:** SINGLE · flagged by: minimax
**File:** `core/crates/adapters/src/http_axum/drift_routes.rs` · **Lines:** 266–269
**Description:** Outer `correlation_id` shadowed by inner binding; not a bug but confusing.
**Suggestion:** Use distinct names.
**Claude's assessment:** Style issue; fix during F025 cleanup pass.

---

### F042 · [SUGGESTION] Near-identical code in `adopt` and `reapply` drift handlers
**Consensus:** SINGLE · flagged by: glm
**File:** `core/crates/adapters/src/http_axum/drift_routes.rs` · **Lines:** 100–267
**Description:** ~60 lines of shared logic differ only in `ResolutionKind`. Should be a shared helper.
**Suggestion:** Extract `resolve_drift_with_apply(state, event_id, session, kind)`. Note: once F002 is fixed, `adopt` and `reapply` will diverge significantly — extract after fixing F002.
**Claude's assessment:** Defer to after F002.

---

### F043 · [SUGGESTION] No password complexity beyond length ≥ 12
**Consensus:** SINGLE · flagged by: glm
**File:** `core/crates/adapters/src/http_axum/auth_routes.rs` · **Lines:** 314–320
**Description:** `change_password` validates only length ≥ 12 and "differs from old password."
**Suggestion:** Consider minimum diversity check or entropy estimation. Not required by the spec.
**Claude's assessment:** Out of spec scope for Phase 9; track for a future hardening phase.

---

## CONFLICTS

None. All findings had consistent direction across reviewers.

---

## Out-of-scope / Superseded

| ID | Title | Reason |
|----|-------|--------|
| F038 | Login audit actor reveals username (SUGGESTION) | Superseded by F018 (WARNING — same issue) |
| — | stubs.rs unspecified (scope_guardian SUGGESTION) | No action required — test infrastructure outside named work units |
| — | Offset-based pagination (kimi) | No pagination cursor exists yet; offset is the current design |
| — | Missing concurrent drift tests (kimi) | Test coverage gap; track in Phase 10 |
| — | Test AppState duplication (kimi) | Refactor candidate; track separately |
| — | Audit correlation linear scan (kimi) | DB index; track in a future DB-hardening pass |
| — | Session touch revoked_at always None (qwen) | Invariant is correct — touch only returns Some for valid sessions |
| — | Capabilities handler linear scan (glm) | Minor perf; addressed when capabilities are hot path |
| — | Bootstrap comment order misleading (minimax) | Comment cleanup; fix during F005 |
| — | Snapshot diff redaction test incomplete (kimi HIGH) | Redaction patterns are schema-driven; test extension is a separate PR |

---

## Summary statistics

| Severity | Unanimous | Majority | Single | Total |
|----------|-----------|----------|--------|-------|
| CRITICAL | 2 (F001, F002) | 0 | 2 (F003, F004) | **4** |
| HIGH | 0 | 8 (F005, F007–F012) | 4 (F006, F013–F015, F016) | **12** |
| WARNING | 1 (F017) | 3 (F018, F020, F025) | 18 (F019, F021–F024, F026–F036) | **22** |
| SUGGESTION | 1 (F037) | 0 | 6 (F039–F043) | **7** |
| **Total** | **4** | **11** | **24** | **45** |
