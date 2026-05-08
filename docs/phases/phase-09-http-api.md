# Phase 9 — HTTP API surface (read + mutate)

Source of truth: [`../phases/phased-plan.md#phase-9--http-api-surface-read--mutate`](../phases/phased-plan.md#phase-9--http-api-surface-read--mutate).

## Pre-flight checklist

- [ ] Phase 8 complete (mutation, snapshot, audit, drift are all in place).

## Tasks

### Backend / adapters crate

- [ ] **Stand up the `axum` HTTP server.**
  - Acceptance: An `axum`-based HTTP server MUST live in `crates/adapters/` and MUST be the only HTTP surface; `core` MUST remain pure.
  - Done when: the server compiles, binds the configured port, and serves `/api/v1/health` with 200.
  - Feature: T1.13 (server side).

- [ ] **Loopback binding by default.**
  - Acceptance: The listener MUST bind `127.0.0.1:<port>` by default; binding `0.0.0.0` MUST require `network.allow_remote_binding = true` and MUST emit a stark startup warning, satisfying H1 and T1.13.
  - Done when: an integration test with the flag absent rejects external bind attempts; with the flag present the warning is logged.
  - Feature: T1.13 (mitigates H1).

- [ ] **Authentication middleware for sessions and tokens.**
  - Acceptance: Middleware MUST validate session cookies against `sessions` and tool-gateway tokens against `gateway_tokens`; both MUST reject with `401` on absence or invalidity. Token lookup MUST use a fast prefix index — no full-table Argon2id scan is permitted. Per-source rate limiting for bearer-token 401 responses MUST be fire-and-forget (async write, best-effort) via a **dedicated bounded mpsc channel with capacity 4096**; on a full channel the send MUST be dropped silently (best-effort) and a `tracing::warn!` event MUST be emitted and a counter incremented so that channel saturation is observable. This matches the login-attempts channel's drop-and-warn policy — the two channels MUST be symmetric in safety properties. Invalid-token floods MUST NOT contend with the SQLite WAL writer used by mutations and snapshots.
  - Done when: integration tests cover happy path, missing credential, and invalid credential for both schemes; a test confirms that a request with an invalid bearer token that matches no prefix completes in under 5 ms; a load test confirms that 1000 concurrent invalid bearer requests do not delay concurrent valid mutation requests beyond 100 ms additional latency; a test that fills the bearer-token 401 rate-limit channel confirms a warn event is emitted on drop.
  - Feature: T1.14 (mitigates F-R5-004).

- [ ] **Argon2id password hashing — single authoritative parameter set.**
  - Acceptance: Password hashing (user passwords only, not tokens) MUST use the `argon2` crate with `m_cost=19456 KiB`, `t_cost=2`, `p_cost=1` (RFC 9106 first-recommendation). These parameters MUST be defined in exactly one named constant (`ARGON2_PARAMS`) in `crates/adapters/src/auth/passwords.rs`. Hashes MUST live only in `users`. The architecture doc MUST be updated in the same commit to match. Token authentication uses a non-secret prefix lookup; tokens are NOT hashed with Argon2id per-request.
  - Done when: a unit test asserts the encoded PHC string contains the chosen parameters (`m=19456,t=2,p=1`); a property test asserts hash uniqueness across distinct passwords; the architecture doc and this file agree.
  - Note: resolves the F003 contradiction. RFC 9106 first-recommendation is the chosen winner.
  - Feature: T1.14.

- [ ] **Tool-gateway token storage and fast-lookup authentication.**
  - Acceptance: The `tokens` table MUST carry a `token_prefix TEXT NOT NULL` column containing the first 16 hex characters of the raw token (derived from the token's first 8 bytes). A unique index on `token_prefix` MUST exist. Token authentication MUST: (1) extract the prefix from the submitted bearer value, (2) query the `tokens` table by prefix (O(1) lookup), (3) run Argon2id only on the retrieved candidate row. A request bearing a bearer value whose prefix matches no row MUST return 401 without invoking any Argon2id hash. When a new token INSERT would violate the unique prefix constraint, the implementation MUST generate a new token and retry up to 5 times before returning an error; the error MUST be user-visible ("token creation failed due to prefix collision — please retry"). The `token_prefix` column MUST be treated as a sensitive value in database backups and exports because it is linkable to the raw bearer value.
  - **Token role semantics (F-R8-004):** The `tokens` table MUST carry a `role TEXT NOT NULL` column storing the issuing user's role value at the moment of token creation (snapshot semantics — role is frozen at issuance and does not change if the user's role is later modified). The middleware MUST load the `role` from the token row (not from the current user row) for all role-based access decisions. This ensures role-restricted endpoints (`GET /api/v1/audit` — Operator/Owner only; etc.) apply identically to `AuthContext::Session` and `AuthContext::Token`.
  - Done when: an integration test confirms that 100 sequential requests with random invalid bearer tokens complete in under 2 seconds total; a test confirms a valid token authenticates correctly; a test simulates a prefix collision and asserts the retry path produces a valid token; a test confirms a Reader-role token receives 403 on `GET /api/v1/audit`; a test confirms that promoting a user's role after token issuance does NOT upgrade the token's frozen role (snapshot semantics).
  - Feature: T1.14 (mitigates F002, F012, F-R2-002, F-R8-004).

- [ ] **Implement bootstrap-account flow — idempotent and atomically safe across restarts.**
  - Acceptance: On first startup with empty `users`, Trilithon MUST: (1) INSERT the bootstrap user row and write the `auth.bootstrap-credentials-created` audit row within a single committed SQLite transaction FIRST; (2) write `<data_dir>/bootstrap-credentials.txt` with mode `0600` SECOND, only after the transaction commits. The credentials MUST NOT appear in process arguments, environment variables, or any other log line (H13). **Interrupted-rotation recovery (F-R7-001, F-R8-001):** Before the idempotency check, `bootstrap_if_empty` MUST check whether `<data_dir>/bootstrap-credentials.tmp` exists. If it does, the recovery procedure MUST verify whether step (c) committed by querying whether an `auth.bootstrap-credentials-rotated` audit row exists with a timestamp more recent than `bootstrap-credentials.txt`'s last-modified time. If such a row exists, step (c) committed and the DB holds the new hash — proceed directly to step (d) (rename, or copy-then-delete if EXDEV) using the existing temp file without generating a new password. If no such recent audit row exists (or the most recent such row predates the credentials file), the crash occurred between step (b) and step (c): the DB still holds the old hash and the temp file MUST be deleted; the full rotation sequence (steps a–d) must then be re-run from the beginning. Renaming the temp file without this DB-state check would make the file show the new password while the DB verifies against the old hash, permanently locking out the bootstrap account. Only after the temp file situation is fully resolved proceed to the normal idempotency check. The flow MUST be idempotent: if the bootstrap user exists and the file exists → skip. If the bootstrap user exists but the file is missing → regenerate via the rotation path with this strict ordering: (a) generate new password and hash; (b) write password to `<data_dir>/bootstrap-credentials.tmp` with mode `0600`; (c) UPDATE `password_hash` AND write `auth.bootstrap-credentials-rotated` audit row in a single committed SQLite transaction; (d) `rename()` the temp file to the final path atomically. Failure handling: if step (c) fails → delete the temp file and return an error (no audit row written, DB unchanged); if step (d) fails → the DB is consistent (new hash committed, audit row written), and the rename MUST be retried up to 3 times with logarithmic backoff. If `rename()` returns `EXDEV` (cross-device — temp dir and data dir are on different filesystems), or if all 3 retries are exhausted, fall back to copy-then-delete: write the final path directly with `O_WRONLY|O_CREAT|O_TRUNC`, fsync, close, then delete the temp file. On total failure (copy also fails), do NOT delete the temp file — return an error that includes the temp file path so the operator can complete the rotation manually by copying it. Deleting the temp file when copy also fails would destroy the only copy of the new plaintext password while the DB already holds the new hash, permanently locking out the bootstrap account with no recovery path. "Roll back the DB update" is NOT a valid recovery — a committed transaction cannot be rolled back. `bootstrap_if_empty` MUST be called exactly once, from the synchronous startup sequence before the Tokio runtime spawns any concurrent tasks ("call once" invariant on the function signature).
  - Done when: an integration test asserts the file mode, the single log line, and credential absence in env/args/other logs; a crash-and-restart test asserts the rotation path produces a valid, usable credential; a test asserts `auth.bootstrap-credentials-rotated` appears in the audit log after a restart-triggered rotation; a test simulates a rename failure (via a read-only directory) and asserts the DB hash and temp file are consistent and the copy-then-delete fallback succeeds; a test confirms EXDEV triggers the copy-then-delete path without retrying `rename()`; a test simulates total failure (both rename and copy fail) and asserts the temp file is NOT deleted and the error message contains the temp file path; a test simulates a crash-after-commit-before-rename (temp file present, recent audit row exists) and asserts startup completes the rename without generating a new password; a test simulates a crash-after-(b)-before-(c) (temp file present, no recent audit row) and asserts startup deletes the temp file and re-runs the full rotation sequence.
  - Feature: T1.14 (mitigates F001, F006, F-R2-003, F-R3-003, F-R4-003, F-R6-001, F-R7-001 — resolves H13).

- [ ] **Force password change on bootstrap login — enforced for both session and token auth.**
  - Acceptance: Login with bootstrap credentials MUST require an immediate password change before any other endpoint becomes reachable. The `must_change_pw` check MUST apply to BOTH `AuthContext::Session` and `AuthContext::Token`: when resolving a bearer token, the middleware MUST load the owning user row and check `must_change_pw`; if true, all endpoints except `POST /auth/change-password` and `POST /auth/logout` MUST return 403. Logout MUST NOT be blocked — an operator must be able to terminate their session at any time regardless of must_change_pw status. Blocking logout under a compromised-credential scenario would prevent the operator from revoking the session.
  - Done when: integration tests assert (a) the step-up redirect for session auth; (b) a valid token whose owning user has `must_change_pw = true` receives 403 on a protected endpoint and succeeds on `change-password`; (c) `POST /auth/logout` returns 200 (not 403) when `must_change_pw = true`.
  - Feature: T1.14 (mitigates F004, F-R5-001).

- [ ] **Rate-limit `POST /auth/login` — dual-keyed (IP + username), SQLite-persisted, fire-and-forget writes, bounded channel with observability, length-bounded inputs.**
  - Acceptance: `POST /api/v1/auth/login` MUST reject any request where the username parameter exceeds 255 bytes with 400, without touching `login_attempts`. This bound MUST be enforced before any rate-limit write so that unbounded-length username strings cannot fill the SQLite database. Login MUST tolerate at most five failures per source address **within any 60-second sliding window** AND at most five failures per username within any 60-second sliding window; the stricter of the two limits applies. Exponential backoff to a 60-second ceiling on exhaustion. State MUST be persisted in `login_attempts` (`ip TEXT, username TEXT NOT NULL CHECK(length(username) <= 255), failure_count INTEGER, window_started_at INTEGER, next_allowed_at INTEGER, last_attempt_at INTEGER`, indexed on both `ip` and `username`). The `window_started_at` column records when the current 60-second window began; `failure_count` MUST be reset to 0 when `now - window_started_at >= 60`. This enforces true sliding-window semantics: an attacker who stays under 5 failures per window is bounded, not an attacker who merely stays under the lockout ceiling. Rate-limit state writes MUST be fire-and-forget via a dedicated bounded mpsc channel with capacity 4096; on a full channel the send is dropped (best-effort) and a `tracing::warn!` event MUST be emitted and a counter incremented (so channel saturation is observable). A successful login MUST synchronously DELETE both the IP row AND the username row for the authenticated user directly in the login handler — NOT via the fire-and-forget channel. Clearing stale rate-limit state on successful login is a correctness invariant, not a best-effort accounting write: a dropped `LoginClearBothKeys` would leave the authenticated user exposed to lockout from a single subsequent failed attempt within the 5-minute janitor window. The synchronous delete adds negligible latency on the non-hot success path and is not subject to channel saturation. A janitor task MUST prune rows where `last_attempt_at < now - 300`.
  - Done when: a test asserts the per-IP threshold; a test asserts the per-username threshold across 5 different source IPs; a test submits 4 failures, waits 61 seconds, then submits 4 more failures, and asserts the second batch also triggers backoff (sliding window reset); a test restarts the daemon mid-backoff and asserts the backoff is still enforced; a test confirms a successful login synchronously clears both the IP and the username rows (asserting both rows are gone immediately after the login response, not after a channel drain); a test simulates a saturated channel and confirms that a concurrent successful login still clears both rows (synchronous path is not affected by channel saturation); a test confirms a 257-byte username returns 400 without creating a `login_attempts` row.
  - **Known limitation (F-R6-002, accepted):** The 300-second janitor prune window erases row state, so an attacker who makes ≤4 attempts per 60-second burst and waits > 5 minutes between bursts can make unlimited aggregate attempts (~48/hour). Slow, persistent brute-force (< 5 attempts per minute, sustained) is **out of scope for Phase 9**. Mitigations for this pattern are deferred: strong password policy, monitoring/alerting on `auth.login-failed` rates via the audit log, and account lockout policy are Phase 2 concerns. This limitation MUST be documented in `docs/api/README.md`.
  - Feature: T1.14 (mitigates F005, F-R2-005, F-R2-008, F-R3-005, F-R3-007, F-R5-002, F-R5-003, F-R7-003).

- [ ] **Session cookie — `Secure` flag policy documented and consistently applied.**
  - Acceptance: The session cookie MUST carry `HttpOnly; Secure; SameSite=Strict; Max-Age=<ttl>`. The `Secure` flag MUST be set unconditionally; modern browsers honour `Secure` on `http://127.0.0.1` as a localhost exception. Non-browser HTTP clients (curl, reqwest, etc.) do NOT honour the localhost Secure exception and will silently drop the cookie — those callers MUST use bearer-token auth (the `Authorization: Bearer <token>` path) rather than session cookies. `docs/api/README.md` MUST document this distinction explicitly: session-cookie auth is for browser-based UI access; bearer-token auth is for CLI tools, test harnesses, and programmatic callers.
  - Done when: a unit test asserts the `Set-Cookie` header contains `Secure` on a loopback-bound test server; integration tests for non-browser flows use bearer-token auth exclusively (no session-cookie flows in non-browser tests).
  - Feature: T1.14 (mitigates F008, F-R2-007).

### HTTP endpoints

- [ ] **Implement auth endpoints.**
  - Acceptance: `POST /api/v1/auth/login`, `POST /api/v1/auth/logout`, and `POST /api/v1/auth/change-password` MUST be implemented.
  - Done when: integration tests cover each endpoint.
  - Feature: T1.14.

- [ ] **Implement `GET /api/v1/capabilities`.**
  - Acceptance: The endpoint MUST return the cached Caddy capability probe result when one is available. When the probe has not yet completed (empty cache at startup), the endpoint MUST return 503 with a machine-readable body `{"status": "probe_pending"}` — it MUST NOT return an empty capabilities object, which callers would interpret as "Caddy supports no features." Callers MUST treat a 503 from this endpoint as "probe not yet complete; retry later," not as an error.
  - Done when: an integration test against a live Caddy returns the cached payload; a test that calls the endpoint before the probe has completed asserts 503 with `{"status": "probe_pending"}` body.
  - Feature: T1.11 (mitigates F-R5-005).

- [ ] **Implement `POST /api/v1/mutations` — with `mutation.submitted` written synchronously to the DB before enqueue.**
  - Acceptance: The endpoint MUST accept any variant of the typed mutation set and return the resulting snapshot identifier and `config_version`. The HTTP handler MUST write the `mutation.submitted` audit row synchronously and directly to the database (via `AuditLogStore::append`, NOT through the async audit writer channel) before enqueuing the mutation. This guarantees the row is durably committed before any applier rows can be inserted, making ordering deterministic regardless of Tokio scheduling. All post-apply audit rows (`mutation.applied`, `config.applied`) belong to the applier's transaction. The envelope `{ "expected_version": <i64>, "body": { ... } }` is required; 409 on stale version; 400 + `mutation.rejected.missing-expected-version` audit row if `expected_version` absent. If the synchronous audit write fails, the mutation MUST NOT be enqueued (the audit row is the precondition, not the consequence). If `mutation.submitted` is successfully committed but the subsequent `enqueue()` call returns an error (channel closed, send failure), the HTTP handler MUST immediately write a `mutation.rejected` audit row with `notes = {"error_kind": "enqueue-failed"}` before returning 503 — this closes the audit trail for the failed submission and prevents an orphaned `mutation.submitted` row without a terminal event.
  - **Write timeout (F-R6-004, F-R8-003):** The synchronous `AuditLogStore::append` call MUST be wrapped in a 500 ms timeout. If the write does not complete within 500 ms, the HTTP handler MUST return 503 without enqueuing the mutation. This bounds the maximum time any mutation handler task holds the SQLite write lock, preventing cascading latency on health, auth, and drift endpoints. A 503 from `POST /mutations` is ALWAYS a transient, retriable condition — it may indicate WAL write contention from a concurrent large snapshot commit (not necessarily an audit store fault). The endpoint SHOULD include a `Retry-After: 1` header on 503 responses. `docs/api/README.md` MUST document that 503 from this endpoint is safe to retry after a short delay.
  - Done when: integration tests cover at least one mutation per Tier 1 variant; a test asserts `mutation.submitted` appears before `mutation.applied` for the same `correlation_id` under concurrent load (not just in serial test execution); a test asserts that if the audit write fails the mutation is not enqueued and the endpoint returns 500; a test simulates an enqueue failure after a successful `mutation.submitted` write and asserts a `mutation.rejected` row with `error_kind = "enqueue-failed"` is present in the audit log; a test simulates a blocked SQLite writer (> 500 ms) and asserts the endpoint returns 503 without enqueuing.
  - Feature: T1.6, T1.8 (mitigates F-R2-001, F-R3-004, F-R4-004, F-R6-004).

- [ ] **Implement snapshot read endpoints — with redaction and summary projection.**
  - Acceptance: `GET /api/v1/snapshots` MUST return summary rows only (`id`, `parent_id`, `config_version`, `created_at`, `actor_kind`, `actor_id`, `intent`) — the `desired_state_json` blob MUST NOT be included in list responses. `GET /api/v1/snapshots/{id}` MUST return the full snapshot but MUST pass `desired_state_json` through the same redactor used by the diff endpoint before returning it; a `secrets.revealed` audit row MUST be written when `redaction_sites > 0`. `GET /api/v1/snapshots/{id}/diff/{other_id}` MUST return the redacted diff.
  - Done when: integration tests cover all three; a test asserts list responses never contain `desired_state_json`; a test asserts a snapshot with a `secret`-tagged field returns a redacted placeholder and writes a `secrets.revealed` audit row.
  - Feature: T1.2 (mitigates F010, F015).

- [ ] **Implement `GET /api/v1/audit` with filters — restricted to Operator and Owner roles.**
  - Acceptance: The endpoint MUST accept filters for time range, actor, event type, and correlation identifier with default page 100, maximum 1000. Access MUST be restricted to `Operator` and `Owner` roles; `Reader`-role requests MUST receive 403.
  - Done when: integration tests assert pagination and every filter; a test asserts a Reader-role session receives 403.
  - Feature: T1.7 (mitigates F014).

- [ ] **Implement drift endpoints — with conflict-retrying resolution, distinct defer kinds, terminal state on exhaustion, and queue visibility.**
  - Acceptance: `GET /api/v1/drift/current`, `POST /api/v1/drift/{event_id}/adopt`, `POST /api/v1/drift/{event_id}/reapply`, `POST /api/v1/drift/{event_id}/defer` MUST be implemented. `GET /api/v1/drift/current` returns the oldest unresolved drift event (or 204 if none); `DriftCurrentResponse` MUST include a `pending_event_count: u32` field containing the count of unresolved drift events excluding the one returned (0 when only one event is queued). This allows clients and operators to determine whether more work is queued without additional requests; the resolution protocol is poll-until-204. Adopt and reapply: derive `expected_version` from the latest committed snapshot; on `ConflictError`, retry up to 3 times (each retry writes a `mutation.conflicted` audit row); after 3 consecutive failures, the `config.drift-auto-deferred` audit row append AND the `DriftEventRow` resolution update (setting `resolution = Deferred` and `resolved_at = now`) MUST execute within a **single committed SQLite transaction** — these are not two sequential writes. The 500 ms `AuditLogStore::append` timeout MUST NOT apply to this combined terminal transaction; partial completion would leave the event permanently unresolvable (audit row committed but resolution still `None`) with unbounded `config.drift-auto-deferred` rows accumulating on every subsequent retry. After the terminal transaction commits, surface 409. `POST /api/v1/drift/{event_id}/defer` (explicit operator deferral) MUST write `config.drift-deferred` (NOT `config.drift-auto-deferred`) and transition the event to resolved/deferred state. The two distinct kinds preserve forensic separation between operator intent and system automation. Both `config.drift-deferred` and `config.drift-auto-deferred` have already been added to `AUDIT_KINDS` in `audit_vocab.rs` and to `AuditEvent` in `audit.rs` (as `DriftDeferred` and `DriftAutoDeferred` respectively) — no further vocabulary additions are required before implementing these endpoints.
  - `POST /api/v1/drift/{event_id}/defer` MUST return 404 if `event_id` does not exist. It MUST return 409 (with a body identifying the current resolution state and kind) if `DriftEventRow.resolution` is already set — whether by auto-defer, explicit defer, reapply, or any other path. This prevents a second resolution audit row from being written for the same event and preserves audit integrity.
  - **System actor convention (F-R6-003, F-R8-002):** All audit rows written by background tasks carry `actor_kind = "system"` and `actor_id` set to the task identifier. This applies uniformly to EVERY audit row the drift applier produces — including each `mutation.conflicted` row written on a conflict retry, `config.drift-auto-deferred`, and any other drift-resolution audit rows. `actor_id = "drift-applier"` for drift resolution rows; `actor_id = "bootstrap"` for bootstrap-flow rows. HTTP-handler-originated rows carry the authenticated caller's identity. The system-actor convention must not be assumed to cover only terminal-state rows — it applies to all rows produced by background tasks regardless of audit event kind.
  - Done when: integration tests cover each transition; a test asserts that a reapply that races with a concurrent mutation succeeds on retry rather than immediately returning 409; a test asserts that after 3 consecutive conflicts the drift event enters the `Deferred` state and a `config.drift-auto-deferred` audit row exists with `actor_kind = "system"` and `actor_id = "drift-applier"`, and that `DriftEventRow.resolution = Deferred` is set atomically in the same transaction as the audit row (simulate crash between writes to confirm partial state is not possible); a test asserts that an explicit `POST /defer` writes `config.drift-deferred` (not `config.drift-auto-deferred`); a test with 3 queued drift events asserts `pending_event_count = 2` in the first response, `pending_event_count = 1` after resolving the first, and 204 after all are resolved; a test asserts that `POST /defer` on an already-auto-deferred event returns 409; a test asserts `POST /defer` with a nonexistent ID returns 404.
  - Feature: T1.4 (mitigates F011, F-R2-004, F-R3-008, F-R4-006, F-R5-006, F-R6-003, F-R7-002).

- [ ] **Implement `GET /api/v1/health` — minimal unauthenticated payload.**
  - Acceptance: The endpoint MUST always return 200 once the daemon is fully started. The unauthenticated response body MUST contain only `{ "status": "ready" }`. Fields `apply_in_flight` and `trilithon_version` MUST NOT appear in the unauthenticated response; they MAY appear in a richer response returned when the caller is authenticated.
  - Done when: an integration test confirms 200 within five seconds of `trilithon run`; an unauthenticated request to `/api/v1/health` returns a body containing only `status`.
  - Feature: T1.13 (mitigates F009).

- [ ] **Serve the OpenAPI document at `/api/v1/openapi.json` — authenticated.**
  - Acceptance: The OpenAPI document MUST be generated from typed handlers via `utoipa`. The endpoint MUST require a valid session or bearer token (any authenticated caller); unauthenticated requests MUST receive 401. This prevents full API surface and authentication schema reconnaissance on non-loopback bindings.
  - Done when: an integration test fetches the document with a valid session and validates it against the OpenAPI 3.1 schema; an unauthenticated request returns 401.
  - Feature: T1.13 (mitigates F-R2-006).

### Concurrency and conflicts

- [ ] **Surface `409 Conflict` on stale `config_version`.**
  - Acceptance: A mutation with a stale `config_version` MUST return a typed 409 conflict response, satisfying H8 substrate.
  - Done when: an integration test simulating concurrent mutations asserts the 409.
  - Feature: T1.8 (substrate for T2.10).

- [ ] **`mutation.applied` and `config.applied` audit rows in the applier's transaction; `mutation.submitted` in the HTTP handler.**
  - Acceptance: Audit row ownership is strictly partitioned: the HTTP handler writes `mutation.submitted` synchronously before enqueue; the applier writes `mutation.applied` + `config.applied` in the same SQLite transaction as the snapshot INSERT. The HTTP handler MUST NOT write any other mutation audit rows. This satisfies ADR-0012's single-transaction invariant while preserving the submission-time audit record.
  - Done when: an integration test confirms all three audit rows (`mutation.submitted`, `mutation.applied`, `config.applied`) share `correlation_id` and appear in the audit log for a successful mutation; a crash-simulation test confirms `mutation.submitted` survives if the applier crashes before committing.
  - Feature: T1.6, T1.7 (mitigates F007, F-R2-001).

### Session security

- [ ] **Session expiry enforced at the SQL predicate level.**
  - Acceptance: The `SessionStore::touch` query MUST include `AND expires_at > ? AND revoked_at IS NULL` as SQL predicates, not only as application-code checks.
  - Done when: an integration test confirms that an authenticated request bearing an expired session cookie returns 401 (not just that `touch` returns `None` in unit isolation).
  - Feature: T1.14 (mitigates F013).

### Tests

- [ ] **Unauthenticated mutation returns 401.**
  - Acceptance: Any unauthenticated request to a mutation endpoint MUST return 401.
  - Done when: an integration test asserts the response.
  - Feature: T1.14.

- [ ] **Bootstrap flow creates the credentials file with mode 0600.**
  - Acceptance: An integration test on a fresh data directory MUST observe the file with mode `0600`.
  - Done when: the test passes on macOS and Linux runners.
  - Feature: T1.14.

- [ ] **Successful mutation produces a snapshot, all three audit rows, and 200 response.**
  - Acceptance: An integration test exercising a successful mutation MUST observe one new snapshot row, THREE new audit rows (`mutation.submitted`, `mutation.applied`, `config.applied` all sharing `correlation_id`), and a 200 response.
  - Done when: the test passes.
  - Feature: T1.6, T1.7.

### Vocabulary additions (required before implementation)

- [x] **Add `config.drift-deferred` and `config.drift-auto-deferred` to `audit_vocab.rs` and `AuditEvent`.** *(DONE — pre-implementation)*
  - `DriftDeferred` and `DriftAutoDeferred` variants added to `AuditEvent`; both kinds added to `AUDIT_KIND_VOCAB` in `audit.rs` and to `AUDIT_KINDS` in `audit_vocab.rs`. `AUDIT_EVENT_VARIANT_COUNT` updated to 43. 100 core tests pass.
  - Feature: T1.4 (mitigates F-R2-004, F-R3-001, F-R3-008).

- [x] **Add `auth.bootstrap-credentials-created` to `AuditEvent` and `AUDIT_KIND_VOCAB`.** *(DONE — pre-implementation)*
  - `auth.bootstrap-credentials-created` was present in `audit_vocab.rs` but absent from `audit.rs`. `AuditEvent::AuthBootstrapCredentialsCreated` variant added; kind added to `AUDIT_KIND_VOCAB`; `AUDIT_EVENT_VARIANT_COUNT` updated to 44; added to `all_variants()`. 100 core tests pass.
  - Feature: T1.14 (mitigates F-R4-002).

### Documentation

- [ ] **Document the API surface in `docs/`.**
  - Acceptance: A `docs/api/README.md` MUST link the OpenAPI document and describe: (a) authentication — session-cookie auth is for browser UI; bearer-token auth is for CLI tools, test harnesses, and programmatic callers — non-browser clients MUST use bearer tokens; (b) loopback default and the bootstrap flow; (c) startup window — the health endpoint returns `{ "status": "starting" }` with HTTP 503 during the startup sequence (migration, bootstrap, capability probe); container orchestrator liveness probes SHOULD use an initial delay of at least 10 seconds or use a dedicated startup probe to avoid triggering restarts during the bootstrap window.
  - Done when: the README exists, documents the session-cookie vs bearer-token distinction, and includes the startup/liveness probe guidance.
  - Feature: T1.13 (mitigates F-R2-007, F-R3-009).

## Design decisions (Rounds 1 and 2 adversarial)

| Finding | Resolution |
|---------|-----------|
| F001 Bootstrap atomicity | INSERT user row first (committed), file write second. Idempotent on restart. `bootstrap_if_empty` call-once pre-Tokio. |
| F002 Bearer-token Argon2id DoS | `token_prefix` index (first 16 hex chars); Argon2id runs only on O(1) candidate. Per-source rate limit on 401s. |
| F003 Argon2id parameter contradiction | RFC 9106 first-recommendation (`m=19456 KiB, t=2, p=1`). Single named constant. Arch doc updated in same commit. |
| F004 `must_change_pw` bypass via token | Token auth path loads owning user row and checks `must_change_pw`; 403 if set. |
| F005 Rate-limiter reset on restart | Rate-limit state in `login_attempts` table; survives restarts. |
| F006 Bootstrap race | `bootstrap_if_empty` call-once pre-Tokio. |
| F007 `mutation.applied` split-brain | `mutation.applied` + `config.applied` in applier's transaction. |
| F008 `Secure` cookie flag | `Secure` set unconditionally; non-browser callers use bearer tokens. |
| F009 Health endpoint leakage | Unauthenticated health returns only `{ "status": "ready" }`. |
| F010 Snapshot raw secret exposure | `GET /api/v1/snapshots/{id}` passes `desired_state_json` through redactor. |
| F011 Drift resolution perpetual 409 | Adopt/reapply retry up to 3 times on conflict. |
| F012 Token table full scan | `token_prefix` index eliminates O(n) scan. |
| F013 Session expiry app-code only | SQL predicate `AND expires_at > ? AND revoked_at IS NULL`. |
| F014 Audit log username exposure | `GET /api/v1/audit` restricted to Operator and Owner roles. |
| F015 Snapshot list loads full JSON blobs | List returns `SnapshotSummary` projection, no `desired_state_json`. |
| F-R2-001 `mutation.submitted` disappears | HTTP handler writes `mutation.submitted` before enqueue; applier writes the other two. Ownership explicit. |
| F-R2-002 `token_prefix` collision denial | Retry up to 5 times on unique constraint violation; `token_prefix` column documented as sensitive. |
| F-R2-003 Bootstrap restart non-atomic | Temp-file + rename pattern for rotation; `auth.bootstrap-credentials-rotated` audit row mandatory. |
| F-R2-004 Drift exhaustion + `defer` vocab gap | 3-retry exhaustion transitions drift to `Deferred` state + writes `config.drift-deferred`; kind added to vocabulary pre-implementation. |
| F-R2-005 IP-only rate limit + unbounded table | Dual-keyed rate limit (IP + username); successful login clears IP row; fire-and-forget writes. |
| F-R2-006 OpenAPI unauthenticated | `/api/v1/openapi.json` requires valid session or bearer token; unauthenticated → 401. |
| F-R2-007 `Secure` cookie breaks non-browser clients | Documented: session-cookie auth is browser-only; bearer-token auth is the non-browser path. |
| F-R2-008 Rate-limit writes contend with WAL writer | All rate-limit state writes are fire-and-forget via a dedicated low-priority async channel. |
| F-R3-001 `config.drift-deferred` absent from vocab files | Added `DriftDeferred` and `DriftAutoDeferred` to `audit.rs` and both kinds to `audit_vocab.rs`. Variant count updated to 43. |
| F-R3-002 TODO slice 9.6 fabricated finding | No implementation TODO exists yet; finding was based on non-existent file. Guard: when TODO is written, it MUST use prefix-based token lookup and must_change_pw check per this design. |
| F-R3-003 Bootstrap rotation ordering inverted | Corrected to: write temp file → commit DB + audit row → rename. If rename fails, retry rename (DB is consistent). "Roll back" language removed. |
| F-R3-004 `mutation.submitted` ordering not guaranteed by async channel | `mutation.submitted` written synchronously via direct `AuditLogStore::append` call (not through async channel) before enqueue. Ordering is now deterministic. |
| F-R3-005 Successful login clears only IP row | Successful login sends single `LoginClearBothKeys { ip, username }` message, deleting both rows atomically (or both dropped together). |
| F-R3-006 TODO LoginRateLimiter fabricated finding | No implementation TODO exists yet. Guard: when written, MUST use SQLite-backed store (not DashMap) per this design. |
| F-R3-007 Fire-and-forget channel capacity unspecified | Channel capacity explicitly set to 4096; dropped sends emit `tracing::warn!` and increment a counter. |
| F-R3-008 Explicit vs auto-defer indistinguishable | Distinct kinds: `config.drift-deferred` for explicit operator action; `config.drift-auto-deferred` for exhaustion-triggered auto-deferral. |
| F-R3-009 Health endpoint live before startup complete | Documented in `docs/api/README.md`: health returns 503 during startup; liveness probes need initial delay ≥ 10s or a startup probe. |
| F-R4-002 `auth.bootstrap-credentials-created` absent from `audit.rs` | `AuditEvent::AuthBootstrapCredentialsCreated` variant added; kind added to `AUDIT_KIND_VOCAB`; count updated to 44. |
| F-R4-003 Bootstrap rename unbounded retry + EXDEV unhandled | Rename retried up to 3 times with logarithmic backoff. On EXDEV or retry exhaustion, fallback to copy-then-delete (open+write+fsync+delete temp). |
| F-R4-004 Enqueue failure leaves orphaned `mutation.submitted` row | If enqueue fails after `mutation.submitted` commit, HTTP handler writes `mutation.rejected` with `notes = {"error_kind": "enqueue-failed"}` before returning 503. |
| F-R4-006 `drift/current` no queue visibility | `DriftCurrentResponse` carries `pending_event_count: u32`; resolution protocol is poll-until-204. |
| F-R5-001 `must_change_pw` blocks logout | Exemption extended to include `POST /auth/logout`; logout must never return 403 regardless of `must_change_pw`. |
| F-R5-002 Rate-limit "per minute" not enforced | Added `window_started_at` column; `failure_count` resets to 0 when `now - window_started_at >= 60`. True sliding-window semantics. |
| F-R5-003 Unbounded username string exhausts storage | `POST /auth/login` rejects username > 255 bytes with 400 before touching `login_attempts`; schema adds `CHECK(length(username) <= 255)`. |
| F-R5-004 Bearer-token 401 channel capacity unspecified | Bearer-token rate-limit channel explicitly bounded to 4096 with drop-and-warn semantics matching the login-attempts channel. |
| F-R5-005 `GET /capabilities` undefined on empty cache | Returns 503 `{"status": "probe_pending"}` when probe has not completed; MUST NOT return empty capabilities payload. |
| F-R5-006 `POST /defer` on resolved event unspecified | `POST /drift/{id}/defer` returns 409 if resolution already set; returns 404 if event_id not found. |
| F-R6-001 Rotation total-failure deletes temp file, locking out bootstrap account | On total copy failure, temp file is NOT deleted; error includes path so operator can complete rotation manually. |
| F-R6-002 Janitor prune enables unbounded slow brute-force | Accepted as known limitation for Phase 9 — slow brute-force (< 5/burst, > 5 min between bursts) is out of scope; mitigated by password policy + audit log monitoring. Documented. |
| F-R6-003 Background-task audit rows have no specified actor | System-generated audit rows MUST carry `actor_kind = "system"` and `actor_id = "<task-name>"` (e.g., `"drift-applier"`, `"bootstrap"`). |
| F-R6-004 Synchronous mutation.submitted write serialises all handler tasks | `AuditLogStore::append` in HTTP handler wrapped in 500 ms timeout; returns 503 if exceeded, preventing cascading latency. |
| F-R7-001 Crash after step (c) leaves DB and file out of sync | Startup checks for `bootstrap-credentials.tmp` before idempotency eval; if present, completes the interrupted rename/copy without generating a new password. |
| F-R7-002 Auto-defer audit + DriftEventRow update not atomic | Auto-defer writes (`config.drift-auto-deferred` + `DriftEventRow` update) MUST be a single SQLite transaction; 500 ms timeout does NOT apply to this terminal write. |
| F-R7-003 `LoginClearBothKeys` droppable on channel saturation | Successful-login row deletion is now synchronous (direct DELETE in handler), not fire-and-forget — correctness invariant, not accounting write. |
| F-R8-001 Recovery renames temp without verifying step (c) committed | Recovery gates on DB state: queries for recent `auth.bootstrap-credentials-rotated` audit row; if absent, crash was pre-(c) → delete temp, re-run full rotation. |
| F-R8-002 `mutation.conflicted` during drift retries lacks stated actor | System-actor convention explicitly extended to every audit row the drift applier produces, including each `mutation.conflicted` retry row. |
| F-R8-003 503 from mutations indistinguishable as retriable vs. fatal | 503 documented as always-retriable (WAL contention, not just fault); `Retry-After: 1` SHOULD be included; documented in `docs/api/README.md`. |
| F-R8-004 Token role semantics undefined; role checks ambiguous for token auth | `tokens` table carries `role TEXT NOT NULL` frozen at issuance; middleware reads role from token row, not user row (snapshot semantics). |

## Cross-references

- ADR-0011 (loopback-only by default with explicit opt-in for remote access).
- ADR-0012 (optimistic concurrency on monotonic `config_version`).
- ADR-0009 (audit log invariants).
- PRD T1.13 (web UI delivery — server side), T1.14 (authentication and session management), T1.8 (route CRUD), T1.6 (typed mutation API), T1.7 (audit log).
- Architecture: "HTTP surface," "Authentication," "Bootstrap flow," "Loopback binding."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] No mutation endpoint is reachable without an authenticated session or a valid tool-gateway token.
- [ ] Sessions are stored server-side and are revocable via `POST /auth/logout` and via an admin operation.
- [ ] The bootstrap account flow satisfies every clause of H13.
- [ ] Loopback-only binding is the default; remote binding requires an explicit flag and logs a warning.
- [ ] Opening `http://127.0.0.1:7878/api/v1/health` after first start returns a 200 within five seconds of `trilithon run`.
- [ ] Bearer-token authentication does not invoke Argon2id for requests whose prefix matches no row in `tokens`.
- [ ] `mutation.submitted` appears in the audit log for every enqueued mutation; `mutation.applied` + `config.applied` appear in the same applier transaction.
- [ ] Rate-limit state survives a daemon restart; rate-limit writes are fire-and-forget.
- [ ] `must_change_pw` enforcement applies to both session and token auth paths.
- [ ] `config.drift-deferred` and `config.drift-auto-deferred` are both in `AUDIT_KINDS` and `AuditEvent` (already done; verify with `cargo test -p core`).
- [ ] `/api/v1/openapi.json` requires authentication.
- [ ] `docs/api/README.md` documents that non-browser callers must use bearer-token auth.
