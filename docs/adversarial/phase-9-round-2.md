# Phase 9 Adversarial Review — Round 2

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 1 |
| HIGH | 3 |
| MEDIUM | 4 |
| LOW | 2 |

## Mitigations verified

The following Round 1 findings are adequately addressed by the revised design:

- **F001** — INSERT-first, file-second ordering with idempotent restart is correctly specified.
- **F002 / F012** — `token_prefix` index eliminates full-table Argon2id scan; O(1) prefix lookup is sound for the common case.
- **F003** — single named constant `ARGON2_PARAMS` for RFC 9106 first-recommendation parameters is sound.
- **F004** — token auth path loading the owning user row and gating on `must_change_pw` is correctly specified.
- **F005** — `login_attempts` table persisted across restarts is correctly specified.
- **F006** — `bootstrap_if_empty` called once pre-Tokio is correctly specified.
- **F007** — both audit rows in the applier's transaction is correctly specified.
- **F008** — `Secure` flag set unconditionally is correctly specified.
- **F009** — unauthenticated health payload reduced to `{ "status": "ready" }` is correctly specified.
- **F010** — `desired_state_json` passes through redactor before returning is correctly specified.
- **F011** — retry up to 3 times on `ConflictError` is correctly specified.
- **F013** — SQL predicate `AND expires_at > ? AND revoked_at IS NULL` is correctly specified.
- **F014** — `GET /api/v1/audit` restricted to Operator and Owner roles is correctly specified.
- **F015** — `GET /api/v1/snapshots` returns `SnapshotSummary` projection is correctly specified.

---

## Findings

### F-R2-001 — `mutation.submitted` audit event disappears entirely CRITICAL

**Technique:** Composition failure

**Scenario:** Round 1 finding F007 moved both `mutation.applied` and `config.applied` into the applier's SQLite transaction, and explicitly states "HTTP handler writes no mutation audit rows." In the prior design, the HTTP handler wrote `mutation.submitted` when it enqueued the mutation. With that write removed, and the applier's transaction writing only `mutation.applied` + `config.applied`, no component in the revised design writes `mutation.submitted` at all. The `AuditEvent::MutationSubmitted` variant exists in the vocabulary and `"mutation.submitted"` appears in `AUDIT_KINDS`. A mutation that is queued, applied, and committed leaves no `mutation.submitted` row — only `mutation.applied` and `config.applied`. More critically, a mutation that is queued but then fails or is rejected before applying would have zero audit rows for the submission event.

**Impact:** Complete audit gap for the submission event of every mutation. Forensic queries against `mutation.submitted` return empty results. The design silently breaks the audit pattern without replacing it.

**What the design must address:** Clarify explicitly where `mutation.submitted` is written. Sound approach: the HTTP handler writes `mutation.submitted` synchronously before enqueuing (this write does not belong in the applier transaction); the applier writes `mutation.applied` + `config.applied`. The design must name the owner of every audit event kind it declares in-scope for this phase.

---

### F-R2-002 — `token_prefix` unique index makes prefix collision a token-creation denial HIGH

**Technique:** Assumption violation + abuse case

**Scenario:** The unique index on `token_prefix` (first 16 hex chars = 8 bytes = 64 bits) means that if two tokens share the same 8-byte prefix, the second INSERT violates the unique constraint and fails. With a small number of tokens the probability of natural collision is negligible. However, an authenticated adversary (Owner or Operator role) who can create tokens can probe the prefix space; and more practically, if the operator tries to rotate a token by creating a new one before revoking the old one, a random collision causes the INSERT to fail with no actionable error.

The deeper issue: the prefix is the first 8 bytes of the raw bearer token. If an attacker intercepts one bearer token in transit (TLS MITM on a misconfigured client, or via logs), they know the prefix. They can then identify exactly which `token_prefix` row in the database corresponds to that token, enabling targeted row identification if the database is ever readable — even though this doesn't enable Argon2id preimage, it collapses the search space from all tokens to one.

**Impact:** (a) Token-creation fails non-deterministically on prefix collision with no actionable error. (b) Token prefix is linkable to a specific database row, enabling targeted oracle attacks if the database is ever readable.

**What the design must address:** Specify error response and retry behaviour when the token-creation INSERT violates the unique constraint (generate a new token and retry, up to N times). Acknowledge that `token_prefix` rows are sensitive if database read access is ever compromised.

---

### F-R2-003 — Bootstrap idempotent restart generates new password without atomic file write or audit row HIGH

**Technique:** Assumption violation + abuse case

**Scenario:** The design specifies "if bootstrap user exists + file missing → recreate file (forced password reset)." This path must: (1) generate a new random password; (2) hash it; (3) UPDATE `password_hash` in `users`; (4) write the new file. If steps 3 and 4 are not atomic and step 4 fails (disk full, permission denied) after step 3 commits, the bootstrap user's password has been changed to a value never written anywhere. The operator cannot log in with the old password (hash changed in DB) and cannot log in with the new password (file was never written). The account is locked with no recovery path except manual DB surgery.

Additionally, the design does not specify that `auth.bootstrap-credentials-rotated` is written during this path. The kind exists in `audit_vocab.rs` and `AuditEvent::AuthBootstrapCredentialsRotated` exists in `audit.rs`. If omitted, an operator has no forensic evidence that credentials were silently rotated by a daemon restart.

An attacker who can delete `bootstrap-credentials.txt` (same filesystem access required to read it) can force a daemon restart to generate a new password, invalidating the operator's stored credentials, with no audit trail.

**Impact:** (a) If file write fails after DB update: bootstrap account permanently locked, no recovery without manual DB surgery. (b) If audit row is omitted: silent credential rotation with no forensic trail. (c) Filesystem write access enables credential churn to lock out legitimate operators.

**What the design must address:** Make the rotation path atomic at the application level: write the file first to a temp path, UPDATE the DB, then rename the temp file. If the DB update fails, delete the temp file. If the rename fails, roll back the DB update. Mandate that `auth.bootstrap-credentials-rotated` is written atomically with the DB update in the same transaction.

---

### F-R2-004 — Drift `adopt`/`reapply` exhaustion produces 3 `mutation.conflicted` rows but no terminal state HIGH

**Technique:** Composition failure

**Scenario:** After 3 consecutive `ConflictError` retries, the design surfaces a 409. At this point the drift event is NOT resolved — `DriftEventRow.resolution` remains `None` and no `config.drift-resolved` audit row has been written. `GET /api/v1/drift/current` continues to return the same unresolved drift event indefinitely. The operator retries adopt/reapply, generates 3 more `mutation.conflicted` rows, gets another 409, and the cycle repeats. There is no circuit breaker, no terminal state, and no cleanup path for a permanently-failing drift resolution.

Additionally, the `POST /api/v1/drift/{event_id}/defer` endpoint exists but has no audit kind in the vocabulary. No `config.drift-deferred` kind appears in `AUDIT_KINDS` or `AuditEvent`. If the implementation uses `config.drift-resolved` for the defer case, the audit log cannot distinguish deferred from resolved. If it uses a new kind, `record_audit_event` returns `StorageError::AuditKindUnknown` and the defer endpoint fails at runtime.

**Impact:** Permanently unresolvable drift event with no recovery action other than a direct mutation bypassing drift resolution. Unbounded `mutation.conflicted` audit row accumulation against a single drift event ID. `defer` endpoint likely broken at runtime due to missing audit kind.

**What the design must address:** (a) Define a terminal state for a drift event after 3 consecutive adopt/reapply failures (write `config.drift-deferred` or a new `drift.abandoned` kind and set `resolution`). (b) Add `config.drift-deferred` to `AUDIT_KINDS` and `AuditEvent` in the same commit that implements the defer endpoint.

---

### F-R2-005 — `login_attempts` rate limit keyed by IP only; distributed attack bypasses it trivially MEDIUM

**Technique:** Cascade construction

**Scenario:** The rate-limit key is `ip TEXT` only. An attacker using a botnet or a large IPv6 /64 block has effectively unlimited source IPs; each new IP gets a fresh 5-attempt budget. Combined with Argon2id running on every login attempt (password hash verification), the effective global login rate is bounded only by SQLite write throughput, not by the per-source rate limiter. At 100 req/s from distinct IPs, 30,000 `login_attempts` rows accumulate in steady state (each pruned after 300 seconds of inactivity). The janitor's 300-second TTL means rows from a sustained distributed attack are never pruned while the attack is active, causing unbounded table growth. A successful login does not reset or delete the row for that IP, so legitimate users in the same IPv6 /64 as an attacker share blackout windows.

**Impact:** Distributed credential-stuffing attack is not rate-limited at the global level. `login_attempts` table grows unbounded under sustained attack. Legitimate users behind NAT or in a shared IP range can be locked out by attackers sharing their prefix.

**What the design must address:** Add a username-keyed secondary rate limit (5 failures per username per minute) alongside the IP-keyed limit; a username-keyed limit catches distributed IP-rotating attacks targeting a single account. Specify whether a successful login resets or deletes the `login_attempts` row for that IP.

---

### F-R2-006 — `/api/v1/openapi.json` authentication status unspecified MEDIUM

**Technique:** Assumption violation

**Scenario:** The design specifies "OpenAPI at `/api/v1/openapi.json` generated via `utoipa`" with no authentication requirement stated. OpenAPI documents generated by `utoipa` from the axum router include: all endpoint paths, HTTP methods, request/response schemas (including mutation field names), security scheme definitions (bearer token format), and server URLs. On a non-loopback binding (`network.allow_remote_binding = true`), this becomes an unauthenticated remote reconnaissance tool. The document will describe the bearer token format (prefix + Argon2id-hashed tail), making the authentication mechanism fully documented to an attacker.

**Impact:** Full API surface and authentication schema exposed to any unauthenticated caller on non-loopback bindings. Reduces attacker reconnaissance cost to zero.

**What the design must address:** Specify that `/api/v1/openapi.json` requires authentication (any valid session or token). If unauthenticated access is intentional, document this decision explicitly and note the tradeoff.

---

### F-R2-007 — `Secure` cookie on HTTP loopback breaks all non-browser HTTP clients MEDIUM

**Technique:** Assumption violation

**Scenario:** F008 mitigation sets `Secure` unconditionally. The design notes "browsers honour `Secure` on `http://127.0.0.1` as a localhost exception." This is correct for modern Chromium-based browsers and Firefox, but is a client-side browser policy, not a server-side guarantee. The following realistic callers do NOT honour the localhost Secure exception: (a) `curl` — silently discards `Set-Cookie` when `Secure` is present but connection is plaintext, with no error; (b) `reqwest` and `ureq` Rust HTTP client libraries — both reject Secure cookies over non-TLS connections by default; (c) integration test harnesses using `reqwest` or `hyper`. An operator using `curl` to call `POST /api/v1/auth/login` receives a `Set-Cookie` header that `curl` silently discards; every subsequent request returns 401. The operator believes authentication is broken.

**Impact:** Session-based authentication is silently broken for all non-browser HTTP clients on the loopback interface. Integration tests using standard HTTP clients will not be able to exercise the session auth path without TLS or special cookie-jar configuration.

**What the design must address:** Either (a) serve the API over TLS even on loopback (self-signed acceptable for local use); or (b) document that session-cookie auth requires a browser or a cookie-jar client with localhost Secure exception support, and confirm that the bearer token path covers non-browser callers; or (c) emit `Secure` only when the listener is TLS-capable and document the per-transport cookie policy.

---

### F-R2-008 — Rate-limit state writes contend with mutation writes for the SQLite WAL writer LOW

**Technique:** Composition failure

**Scenario:** Both the login rate limiter and the bearer-token 401 rate limiter write to SQLite (synchronously, in the request handler, before returning 401). Under a combined attack — credential stuffing on `POST /api/v1/auth/login` and invalid bearer token flooding — both rate-limit paths contend for the single WAL writer lock. The design does not specify whether these writes are synchronous or fire-and-forget. If synchronous, high-rate invalid-request attacks cause SQLite write queue saturation, increasing response latency for legitimate callers on all endpoints that also write audit rows or snapshots.

**Impact:** Under sustained invalid-token or credential-stuffing load, legitimate mutation and snapshot write latency increases proportionally. Not a correctness failure, but a latency amplification — an attacker can degrade legitimate operations by flooding invalid requests.

**What the design must address:** Specify that rate-limit state writes are fire-and-forget (best-effort): write the increment asynchronously via a dedicated low-priority channel, accepting that a small number of writes may be lost on crash. This decouples the rate-limit write path from legitimate request latency.
