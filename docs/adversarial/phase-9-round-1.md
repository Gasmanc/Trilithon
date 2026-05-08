# Phase 9 Adversarial Review — Round 1

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 3 |
| HIGH | 5 |
| MEDIUM | 5 |
| LOW | 3 |

**Design summary:** Phase 9 adds an axum HTTP server to the Trilithon daemon, exposing authentication (login/logout/change-password), mutation, snapshot, audit, drift, capabilities, and OpenAPI endpoints. It introduces Argon2id password hashing, a bootstrap credential file, server-side sessions, tool-gateway token auth, per-source login rate limiting, and enforces the optimistic concurrency protocol from ADR-0012 at the HTTP layer.

---

## Findings

---

### F001 — Bootstrap file written before user row is committed CRITICAL

**Technique:** Abuse case

**Scenario:** The bootstrap algorithm opens a transaction to count users, then generates a password, then calls `UserStore::create_user` (which internally does its own INSERT), then writes `bootstrap-credentials.txt` to disk. If the daemon process is killed — by the OS OOM killer, SIGKILL from a service manager, or a panic in a concurrent startup task — after the file is written but before the user row is committed (or after the file is written but the file-creation call returned `Ok` while the DB INSERT is still in flight), the file contains a password that no user row possesses. On the next restart, `bootstrap_if_empty` checks `COUNT(*) FROM users`. If the crash occurred after the INSERT committed, the count is 1 and bootstrap is skipped — but the file still exists with the correct password, which is fine. If the crash occurred before the INSERT committed (e.g. connection pooling returned a rolled-back connection), the count is 0, bootstrap reruns and writes a **new** password to `bootstrap-credentials.txt`, silently invalidating the file the operator may have already read. If `create_new(true)` was used for the file, the second run fails with `Err(AlreadyExists)` → `BootstrapError::Io` on an otherwise-healthy startup. The design does not specify what to do when `create_new` fails because the file already exists, nor does it specify whether the DB INSERT and the file write happen atomically or in which order.

**Impact:** On a crashed-then-restarted daemon the operator either cannot log in (stale credentials file from the first attempt), cannot bootstrap (second run errors on `create_new`), or is silently presented a new password with no indication the prior file is defunct. In a container restart loop this manifests as a daemon that perpetually errors at bootstrap.

**What the design must address:** Specify a strict ordering: INSERT first (committed), file write second. On restart, if `users` has exactly one bootstrap user with `must_change_pw = true` and `bootstrap-credentials.txt` already exists, skip bootstrap entirely (the file is valid). If the file does not exist but the bootstrap user does, recreate the file with a forced password reset instead of failing. The algorithm must be idempotent across restarts.

---

### F002 — Tool-gateway token verification via per-request Argon2id hash is a DoS vector CRITICAL

**Technique:** Abuse case / Resource exhaustion

**Scenario:** The authentication middleware verifies `Authorization: Bearer <token>` by hashing the submitted token with Argon2id and comparing against `tokens.token_hash`. Argon2id at `m=19456 KiB, t=2, p=1` uses ~19 MiB of memory and takes ~0.3–0.5 s of CPU per invocation on typical hardware. A caller — or an internet scanner that reaches a non-loopback binding — can send a stream of requests with arbitrary `Authorization: Bearer <random>` headers. Each request causes the auth middleware to invoke a full Argon2id hash before returning 401. At 10 concurrent connections this means ~190 MiB of transient memory allocations and ~3–5 CPU-seconds per second of sustained load. The existing login rate limiter only applies to `POST /auth/login` keyed by source IP; there is no rate limiter for bearer-token attempts, and the design explicitly defers bearer-token rate limiting to a later phase.

**Impact:** An unauthenticated caller can exhaust the daemon's CPU and memory, potentially preventing Caddy configuration updates from reaching their apply deadline — a de facto denial of service against the configuration management plane.

**What the design must address:** Do not hash bearer tokens in the auth middleware hot path. Instead, index tokens by a fast lookup key (e.g. store the first 16 bytes of the raw token as a public lookup prefix in the `tokens` table, retrieve the full hash, then verify only when a candidate row is found). Alternatively, apply a per-source rate limit to bearer-token 401 responses, identical in shape to the login rate limiter, before invoking the hash.

---

### F003 — Argon2id parameter mismatch between architecture doc and phase TODO CRITICAL

**Technique:** Assumption violation

**Scenario:** A direct contradiction exists: the architecture doc states `m=64MiB, t=3, p=1`, while the Phase 9 TODO specifies `m_cost=19456 KiB, t_cost=2, p_cost=1` (RFC 9106 first-recommendation). These are different parameter sets: `64 MiB / t=3` is approximately 3× more expensive per hash than `19456 KiB / t=2`. If implementation uses the architecture-doc parameters, the login rate limiter and per-request token-verification cost calculations will be wrong. If implementation uses the TODO parameters, existing password hashes (if any from earlier test data) computed with the architecture-doc params will fail to verify. The constant naming in the TODO implies the TODO wins, but no explicit decision is recorded.

**Impact:** If different parts of the codebase or test fixtures are written against different parameters, password verification fails silently (returns `Ok(false)`) for valid credentials, locking all users out. At `m=64MiB`, login round-trip is ~1.5–3 s on typical CI hardware, which breaks the "0.5–2 second hash time" sanity check in tests.

**What the design must address:** Resolve the contradiction explicitly before writing any code. Pick one parameter set, record it in a single named constant in `crates/adapters/src/auth/passwords.rs`, and update the architecture doc to match. Tests must assert the encoded hash string contains the chosen params (the PHC string format embeds them, so this is verifiable without approximation).

---

### F004 — `must_change_pw` step-up bypass through tool-gateway token auth HIGH

**Technique:** Assumption violation / Composition failure

**Scenario:** The `must_change_pw` enforcement is conditioned only on `AuthContext::Session.must_change_pw`. The middleware's step-up check reads the session's flag and returns 403 if true and the path is not `change-password`. The `AuthContext::Token` variant has no `must_change_pw` field. If a tool-gateway token is created for a user whose `users.must_change_pw = true`, requests authenticated via `Authorization: Bearer <token>` bypass the step-up gate entirely. The design does not specify whether tool-gateway tokens should be blocked while `must_change_pw = true` for their owning user, nor how the token auth path accesses the user row.

**Impact:** A tool-gateway token pre-dating a forced password-change event can continue to apply mutations while the human operator is blocked. This is most dangerous in the bootstrap scenario: if a token is somehow created before bootstrap finishes (e.g. test scaffolding, fixture state), the change-password gate is invisible to it.

**What the design must address:** When resolving `AuthContext::Token`, the middleware must load the owning user row and check `must_change_pw`. If true, token-authenticated requests to protected non-change-password endpoints must return 403, same as the session path. Alternatively, explicitly document that tool-gateway tokens are not subject to `must_change_pw` and state the rationale (e.g. tokens are pre-scoped and non-interactive), so the implementation is intentional.

---

### F005 — Rate limiter state is in-process memory; restarts reset all buckets HIGH

**Technique:** Assumption violation

**Scenario:** The `LoginRateLimiter` is `DashMap<IpAddr, BucketState>` — purely in-process memory. When the daemon restarts (crash, systemd restart, container restart), all buckets are cleared. An attacker who triggers a daemon restart immediately regains a full 5-attempt budget per source IP. Combining with F001's crash-on-bootstrap scenario: an attacker can crash-then-restart the daemon repeatedly, gaining 5 attempts per restart cycle. The design does not state whether in-process state survives a hot-reload of configuration.

**Impact:** An attacker with the ability to trigger process restarts effectively has an unbounded login attempt budget, defeating the rate limiter entirely. The primary target is the bootstrap account (known username `admin`, credentials in a file that may be world-readable on a misconfigured host).

**What the design must address:** Persist rate-limit bucket state to SQLite (a dedicated `login_attempts` table with `ip`, `failure_count`, `next_allowed_at`, `last_attempt_at`, a TTL-based cleanup row) that survives restarts. Alternatively, explicitly scope the threat model: the design already requires loopback-by-default, so rate limiting is primarily a local-user protection; document this and accept the restart-reset behavior as a known limitation with a tracked issue.

---

### F006 — Concurrent `bootstrap_if_empty` calls are not serialised HIGH

**Technique:** Race condition

**Scenario:** If two concurrent calls to `bootstrap_if_empty` execute simultaneously (e.g. if the startup sequence calls it in a `JoinSet` alongside other tasks, or if two daemon instances race despite advisory locks), both observe `COUNT = 0` before either commits. Both then attempt `create_user` for username `admin`. SQLite's serialised writer lets one succeed and fails the second with a UNIQUE constraint violation. The second bootstrap call returns a `UserStoreError`. Whether the daemon treats this as fatal or logs-and-continues is unspecified. If both calls also attempt `create_new(true)` on `bootstrap-credentials.txt`, the second gets `Err(AlreadyExists)` → `BootstrapError::Io` — a startup failure even though bootstrap actually succeeded.

**Impact:** A container restart or a test harness that launches two daemon instances against the same data directory can produce a perpetually-failing bootstrap that terminates the daemon on startup.

**What the design must address:** Serialise bootstrap access with a startup-phase mutex or simply ensure `bootstrap_if_empty` is called exactly once from the daemon's synchronous startup sequence before the Tokio runtime spawns any concurrent tasks. Document this as a "call once" invariant on the function signature.

---

### F007 — `mutation.applied` audit row is non-atomic with the snapshot write HIGH

**Technique:** Composition failure / Cascade construction

**Scenario:** The `mutation.applied` audit row is to be written at the HTTP handler level ("the mutation-row write happens here at the HTTP layer"). The snapshot INSERT and `config.applied` row are written by the applier in a single SQLite transaction. The HTTP handler then writes `mutation.applied` in a separate, subsequent database call. If the process crashes or the SQLite connection fails between the applier's commit and the HTTP handler's audit write, the snapshot exists and `config.applied` is written, but `mutation.applied` is missing. ADR-0012 states "The increment, the snapshot insert, and the audit append SHALL execute in ONE SQLite transaction."

**Impact:** The audit log is inconsistent: the configuration was applied but the `mutation.applied` row is absent. Forensic review cannot distinguish "mutation applied and audit row lost" from "mutation never applied." The `config.applied` row exists but the `mutation.applied` row — which carries the actor and intent — does not, breaking audit accountability.

**What the design must address:** Move the `mutation.applied` audit row write into the applier's transaction, alongside `config.applied`. The HTTP handler should not be responsible for writing audit rows about the applied mutation; that responsibility belongs in the same transaction that commits the snapshot and increments `config_version`.

---

### F008 — `Secure` cookie flag absent on loopback, but loopback is the default MEDIUM

**Technique:** Assumption violation

**Scenario:** The `Secure` flag on the session cookie is only set when the binding is non-loopback. On the default loopback deployment the cookie is `HttpOnly; SameSite=Strict; Max-Age=<ttl>` but not `Secure`. This is technically acceptable for `http://127.0.0.1:7878`, but the design does not account for: SSH-tunnel access (ADR-0011's recommended headless pattern), a reverse proxy that speaks HTTPS externally but plain HTTP to the backend (Caddy in front of Trilithon), or the Phase 11 web UI being served via HTTPS while the backend is plain HTTP. In any of these cases, a session cookie without `Secure` may be transmitted over cleartext, or a browser may refuse to send it back over HTTPS.

**Impact:** In a reverse-proxy-in-front-of-loopback deployment the session cookie is transmitted unencrypted. If the Phase 11 web UI is served via HTTPS and the backend is plain HTTP, the browser will not send the `Secure`-less cookie over HTTPS — the UI will appear to always be logged out.

**What the design must address:** Set `Secure` always. On loopback the attribute is harmless (browsers send `Secure` cookies to `http://127.0.0.1` as a localhost exception). The `secure` parameter to `build_cookie` should default to `true` with an explicit opt-out, not default to `false` with an opt-in.

---

### F009 — `/api/v1/health` is unauthenticated and leaks `apply_in_flight` state MEDIUM

**Technique:** Data exposure

**Scenario:** `GET /api/v1/health` is on the public whitelist (no auth required) and returns `"apply_in_flight": <bool>` plus `"trilithon_version": "<semver>"`. On a non-loopback binding an unauthenticated caller can poll this endpoint to observe timing patterns: `apply_in_flight = true` reveals when the daemon is actively pushing configuration to Caddy. The software version enables targeted exploitation.

**Impact:** Version information enables targeted exploitation. `apply_in_flight` leakage on a public binding allows an unauthenticated observer to infer operator activity patterns.

**What the design must address:** Remove `trilithon_version` from the unauthenticated response (move it behind auth). Either require auth for health details (returning only `{ "status": "ready" }` unauthenticated), or explicitly accept the exposure and document the threat model consequence for non-loopback deployments.

---

### F010 — Snapshot `desired_state_json` returned verbatim at `GET /api/v1/snapshots/{id}` leaks secrets MEDIUM

**Technique:** Data exposure

**Scenario:** `GET /api/v1/snapshots/{id}` returns the full row including `desired_state_json`. The `desired_state_json` may contain basic-auth credentials, API keys, or TLS private key paths stored in the Caddy configuration. The diff endpoint correctly runs the redactor (confirmed: "the diff endpoint returns the redacted diff (never plaintext secrets)"), but the raw snapshot GET does not. The design applies secret redaction only to diffs, not to direct snapshot content access.

**Impact:** Any authenticated caller (including a `Reader`-role user) can retrieve raw secrets embedded in any historical snapshot. These secrets are presumably managed by the Phase 10 secrets vault, but the vault's redaction boundary at the HTTP layer is not established in Phase 9.

**What the design must address:** Apply the same redactor used for diffs to the full snapshot body before returning it at `GET /api/v1/snapshots/{id}`, or require a higher privilege level (`Owner`-only) for raw snapshot access. The `secrets.revealed` audit kind in the vocabulary implies the architecture expects secret reveals to be logged; this endpoint silently delivers secrets without writing that audit row.

---

### F011 — Drift resolution endpoints auto-derive `expected_version` from "latest committed snapshot" MEDIUM

**Technique:** Race condition / Logic flaw

**Scenario:** Drift resolution reads `latest_desired_state()` to derive `expected_version`, then submits the mutation to the applier. Between the read and the mutation submit, another client can successfully apply a mutation, advancing `config_version`. The drift resolver's derived `expected_version` is now stale. The applier returns `OptimisticConflict`. The drift endpoint returns 409. The unresolved drift event remains open.

**Impact:** In a system with active mutation traffic (a plausible scenario during incident response), drift resolution can perpetually fail with 409 conflicts. The operator cannot resolve drift without first freezing all mutation traffic — operationally untenable.

**What the design must address:** Drift resolution should retry the `expected_version` derivation on conflict (up to N times, with the conflict documented in the audit row) rather than surfacing a 409 to the caller. Alternatively, drift resolution endpoints should accept a caller-supplied `expected_version` (matching the mutations API contract) so the caller can handle conflict resolution explicitly, consistent with ADR-0012's "UI SHALL NOT silently retry" principle.

---

### F012 — Tool-gateway token lookup requires full-table scan with per-row Argon2id hash MEDIUM

**Technique:** Resource exhaustion / Logic flaw

**Scenario:** Because the hash includes a per-token salt (embedded in the PHC string), the middleware cannot look up a matching row by hash — it must iterate over all rows in the `tokens` table, hashing the submitted token against each row's stored hash until it finds a match or exhausts the table. As the `tokens` table grows, authentication time grows linearly with the number of tokens. This compounds with F002's CPU exhaustion: each unauthenticated bearer-token request now invokes O(n) Argon2id operations.

**Impact:** With 50 active tool-gateway tokens, every bearer-token request invokes up to 50 Argon2id hash operations before returning 401. Authentication latency becomes O(n × hash_time).

**What the design must address:** Use a fast lookup key. Store a non-secret prefix of the raw token (e.g. the first 8 bytes as hex, 16 characters) as a plaintext `token_prefix` column with an index. The middleware reads the prefix from the submitted bearer value, fetches the matching row(s) by prefix, then verifies with Argon2id only on the O(1) candidate.

---

### F013 — Session expiry enforced in application code, not at the SQL predicate level LOW

**Technique:** Composition failure

**Scenario:** The auth middleware's `SessionStore::touch` returns `None` if `expires_at < now`, checked in application code. The test covers `touch` returning `None` — but only in isolation, not whether the middleware rejects a request bearing an expired session cookie. If `touch`'s expiry check has a one-character bug (e.g. `>=` vs `>` at exactly the expiry boundary), expired sessions are valid for up to an hour (the janitor grace period).

**Impact:** A boundary condition bug allows expired sessions to authenticate for up to 1 hour. Low probability but high impact if exploited.

**What the design must address:** The database query backing `touch` must include `AND expires_at > ? AND revoked_at IS NULL` as SQL predicates, not just application-code checks. Add an integration test confirming an authenticated request with an expired session cookie returns 401.

---

### F014 — `GET /api/v1/audit` exposes full `actor_id` strings including usernames LOW

**Technique:** Data exposure

**Scenario:** The `AuditRowResponse` includes `actor_id` verbatim. The `actor_id` for session-based mutations is the username. The design does not restrict audit log access by role — any authenticated user can query `GET /api/v1/audit`. A `Reader`-role user can enumerate all usernames of actors who ever performed mutations or logged in.

**Impact:** Username enumeration for all accounts that have ever performed authenticated actions. Reduces brute-force cost.

**What the design must address:** Restrict the audit log to `Owner` and `Operator` roles, or redact `actor_id` to a stable pseudonym for `Reader`-role callers. At minimum, document the exposure in `docs/api/README.md`.

---

### F015 — `GET /api/v1/snapshots` list endpoint returns full `desired_state_json` in summary rows LOW

**Technique:** Resource exhaustion

**Scenario:** If the list endpoint returns full snapshot rows including `desired_state_json`, loading 200 full snapshots (max page size) into memory in a single query response — each potentially megabytes of Caddy configuration JSON — can push the daemon into memory pressure. Two concurrent callers each requesting limit=200 cause 400 full snapshot deserializations simultaneously.

**Impact:** Recoverable (200 is the hard cap) but can cause memory pressure in a high-traffic environment.

**What the design must address:** The list endpoint should return summary rows (id, parent_id, config_version, created_at, actor, intent) without `desired_state_json`. The full state is accessible at `GET /api/v1/snapshots/{id}`.

---

## Top concern

**F002** — the per-request Argon2id hash for bearer-token verification with no rate limiting creates a trivially exploitable CPU/memory exhaustion path on any non-loopback binding, and the design explicitly defers the mitigation to a later phase. This is the most likely to cause an outage in the first non-loopback deployment.

**Recommended to address before implementation:** F001 (bootstrap atomicity), F002 (bearer-token hash DoS), F003 (Argon2 parameter contradiction), F007 (audit row split-brain). F001 and F003 are design specification gaps that will be copied into code and become load-bearing assumptions.
