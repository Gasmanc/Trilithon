# Design Decision — Phase 9

**Date:** 2026-05-13
**Rounds:** 8
**Final approach:** axum HTTP server in `crates/adapters/` with session-cookie auth (browser) and bearer-token auth (non-browser), Argon2id password hashing with RFC 9106 first-recommendation parameters, dual-keyed SQLite-persisted sliding-window rate limiting, optimistic concurrency via `config_version`, and a fully audited bootstrap-account flow with atomic rotation and crash-recovery guarantees.

---

## Rejected Approaches

| Approach | Rejected because |
|----------|-----------------|
| Full-table Argon2id scan on every bearer-token request | R1-F002: O(n) scan enables per-request DoS proportional to token count. Replaced with `token_prefix` index (first 16 hex chars) for O(1) candidate lookup; Argon2id only on the single matching row. |
| Async audit writer channel for `mutation.submitted` | R2-F-R2-001 + R3-F-R3-004: async channel provides no ordering guarantee — applier could write `mutation.applied` before the channel drains `mutation.submitted`. Replaced with synchronous `AuditLogStore::append` (500 ms timeout) before enqueue. |
| Single-keyed (IP-only) rate limiter | R2-F-R2-005: distributed attackers rotating IPs bypass IP-only limits. Replaced with dual-keyed (IP + username) sliding-window rate limiter persisted in `login_attempts`. |
| File-first bootstrap ordering | R1-F001: file write before DB INSERT means a crash after file write leaves an inaccessible account with no DB row. Replaced with DB-first: INSERT + audit row in one transaction, file write second. |
| DB-first rotation ordering | R3-F-R3-003: the original "commit DB then write file" rotation path meant a failed file write after DB commit had no safe rollback path. Replaced with temp-file-first: write `.tmp`, commit DB + audit row, then rename. |
| `LoginClearBothKeys` via fire-and-forget channel | R7-F-R7-003: a full channel drops the clear message, leaving a successfully-authenticated user exposed to lockout from a single subsequent failed attempt within the 5-minute janitor window. Replaced with synchronous DELETE in the login handler. |
| Rename-only recovery (no DB-state check) | R8-F-R8-001: renaming `.tmp` without verifying step (c) committed produces a mismatch (new plaintext in file, old hash in DB) if the crash occurred between step (b) and step (c). Recovery now gates on a recent `auth.bootstrap-credentials-rotated` audit row. |
| Two sequential writes for drift auto-defer | R7-F-R7-002: a timeout firing between the audit append and the `DriftEventRow` update leaves the event permanently unresolvable with unbounded `config.drift-auto-deferred` accumulation. Both writes must execute in a single SQLite transaction; the 500 ms timeout does not apply to terminal transaction writes. |
| `config.drift-deferred` as the single kind for both auto-defer and explicit defer | R3-F-R3-008: a single kind prevents forensic distinction between operator intent (explicit) and system automation (exhaustion-triggered). Two separate kinds: `config.drift-deferred` (explicit) and `config.drift-auto-deferred` (auto). |
| Unauthenticated `/api/v1/openapi.json` | R2-F-R2-006: on non-loopback bindings, the OpenAPI document exposes the full API surface, authentication schema, and bearer-token format to unauthenticated callers. Now requires a valid session or bearer token. |
| Gateway tokens without a role column | R8-F-R8-004: role-restricted endpoints (`GET /api/v1/audit`, etc.) have no defined access decision for token-authenticated callers if tokens carry no role. Tokens now carry `role TEXT NOT NULL` frozen at issuance (snapshot semantics). |

---

## Key Constraints Surfaced

The adversarial process revealed these constraints that any implementation must respect:

1. **Token authentication is prefix-first, Argon2id-second.** Extract the first 16 hex chars of the bearer value, query `tokens` by `token_prefix` (O(1)), run Argon2id only on the single candidate. A prefix miss returns 401 immediately with no hash computation. Prefix collision on INSERT: retry up to 5 times.

2. **`mutation.submitted` is written synchronously with a hard timeout.** `AuditLogStore::append` in the HTTP handler MUST complete within 500 ms or the handler returns 503 without enqueuing. 503 is always a transient, retriable condition (`Retry-After: 1`). The timeout does NOT apply to terminal transaction writes (e.g. drift auto-defer).

3. **Bootstrap rotation ordering is temp-file-first.** (a) generate + hash password; (b) write `.tmp` mode 0600; (c) commit DB UPDATE + audit row in one transaction; (d) rename/copy-then-delete. Step (c) failure: delete `.tmp`, no audit row. Step (d) failure: retry rename up to 3 times (logarithmic backoff), then copy-then-delete fallback. EXDEV: skip retries, go directly to copy-then-delete. Total failure: preserve `.tmp`, include path in error message.

4. **Bootstrap crash recovery gates on DB state.** If `bootstrap-credentials.tmp` exists at startup, check for a recent `auth.bootstrap-credentials-rotated` audit row (timestamp > credentials file mtime). If present: step (c) committed — complete step (d). If absent: crash was pre-(c) — delete `.tmp` and re-run the full rotation sequence.

5. **Drift auto-defer is a single atomic transaction.** The `config.drift-auto-deferred` audit append and `DriftEventRow.resolution = Deferred` update MUST execute together in one SQLite transaction. The 500 ms HTTP-handler timeout does not apply to this write.

6. **Successful login clears both rate-limit rows synchronously.** The login handler MUST issue a direct synchronous DELETE of both the IP row and the username row from `login_attempts`. This is a correctness invariant, not a fire-and-forget accounting write — it must not be routed through the droppable channel.

7. **Rate-limit writes use a bounded fire-and-forget channel (capacity 4096, drop-and-warn).** Failure-count increments and bearer-token 401 rate-limit writes go through this channel. Dropped sends emit `tracing::warn!` and increment a counter. Successful-login deletes do NOT use this channel (see constraint 6).

8. **Slow brute-force is explicitly out of scope for Phase 9.** The sliding-window mechanism prevents burst attacks (≤ 5/60 s). Slow-and-steady attacks (< 5/burst, > 5 min between bursts) are mitigated by password policy and `auth.login-failed` monitoring; full account-lockout policy is deferred to Phase 2.

9. **System-generated audit rows always carry `actor_kind = "system"`.** Every audit row produced by a background task (drift applier, bootstrap flow) — including each `mutation.conflicted` retry row — MUST carry `actor_kind = "system"` and `actor_id = "<task-name>"` (`"drift-applier"` or `"bootstrap"`). This applies to ALL audit rows from background tasks, not only terminal-state rows.

10. **Token role is frozen at issuance.** The `tokens` table carries `role TEXT NOT NULL` set to the issuing user's role at creation time. The middleware resolves a token's role from the token row, not the current user row. Subsequent role changes to the issuing user do not affect existing tokens.

11. **Login input is bounded at 255 bytes.** Requests with a username exceeding 255 bytes return 400 without touching `login_attempts`. The schema enforces `CHECK(length(username) <= 255)`.

12. **`POST /drift/{id}/defer` is idempotent-safe.** Returns 404 if the event does not exist; returns 409 if `DriftEventRow.resolution` is already set. Only one resolution audit row per drift event.

13. **The `GET /capabilities` endpoint returns 503 `{"status": "probe_pending"}` until the Caddy probe completes.** It MUST NOT return an empty capabilities object (which callers would interpret as "no features supported").

---

## Unaddressed Findings

Findings that were raised but explicitly accepted as known risk:

| ID | Severity | Finding | Accepted because |
|----|----------|---------|-----------------|
| F-R5-002 / F-R6-002 | HIGH→accepted | Janitor prune (300 s) erases sliding-window state; slow brute-force (< 5/burst, > 5 min gap) is unbounded | Out of scope for Phase 9. Mitigated by password policy and `auth.login-failed` monitoring. Account-lockout policy is Phase 2. Documented in `docs/api/README.md`. |

---

## Round Summary

| Round | Critical | High | Medium | Low | Outcome |
|-------|----------|------|--------|-----|---------|
| 1 | 3 | 5 | 5 | 3 | Bootstrap atomicity, token DoS, Argon2id params, must_change_pw, rate-limiter restart, and 10 more mitigations added |
| 2 | 1 | 3 | 4 | 2 | mutation.submitted ownership, token_prefix collision retry, bootstrap non-atomic rotation, drift exhaustion vocab gap, dual-keyed rate limit, fire-and-forget channel |
| 3 | 2† | 4 | 3 | 2 | config.drift-deferred vocab added, bootstrap ordering corrected, mutation.submitted sync, dual clear on login, channel capacity specified (†2 findings fabricated and discarded) |
| 4 | 1† | 3 | 1 | 1 | auth.bootstrap-credentials-created added to audit.rs, rename retry + EXDEV fallback, enqueue-failure mutation.rejected, drift pending_event_count (†1 finding fabricated and discarded) |
| 5 | 0 | 1 | 4 | 1 | must_change_pw logout exemption, sliding-window semantics, username 255-byte limit, bearer-token channel capacity, capabilities probe_pending, defer 409/404 |
| 6 | 0 | 2 | 2 | 0 | Temp file preserved on total failure, slow brute-force accepted, system actor convention, audit write timeout 500 ms |
| 7 | 0 | 2 | 1 | 0 | Crash-recovery gates on .tmp presence, auto-defer atomicity, login clear moved to synchronous |
| 8 | 0 | 1 | 2 | 1 | Recovery gates on DB state (not just filesystem), drift-applier actor for all rows, 503 retriable + Retry-After, token role frozen at issuance |
