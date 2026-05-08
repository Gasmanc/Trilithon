# Phase 9 Adversarial Review — Round 5

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 1 |
| MEDIUM   | 4 |
| LOW      | 1 |

## Mitigations verified

The following Round 4 findings are cleanly addressed:

- **F-R4-002** — `AuditEvent::AuthBootstrapCredentialsCreated` variant added; kind added to `AUDIT_KIND_VOCAB`; `AUDIT_EVENT_VARIANT_COUNT` updated to 44; added to `all_variants()`. 100 core tests pass.
- **F-R4-003** — Bootstrap rename retried up to 3 times with logarithmic backoff. `EXDEV` or retry exhaustion triggers copy-then-delete fallback.
- **F-R4-004** — Enqueue failure after committed `mutation.submitted` immediately writes `mutation.rejected` with `notes = {"error_kind": "enqueue-failed"}` before returning 503.
- **F-R4-006** — `DriftCurrentResponse` carries `pending_event_count: u32`; resolution protocol is poll-until-204.

---

## Findings

### F-R5-001 — `must_change_pw` 403 gate traps the logout endpoint MEDIUM

**Technique:** Assumption violation.

**Scenario:** The design states: "if [must_change_pw] is true, all endpoints except change-password MUST return 403." An operator logs in with bootstrap credentials (`must_change_pw = true`) and decides to abandon the session without changing the password — perhaps they logged in from a shared device. They POST to `POST /api/v1/auth/logout`. Logout is not on the exemption list; the endpoint returns 403, and the session remains active in the database until TTL expiry.

**Impact:** The operator cannot terminate their own session while under the must_change_pw gate. If the bootstrap credentials file is compromised during this window, an attacker who also obtains the session cookie holds an active authenticated session that the legitimate operator cannot revoke. Logout is the safest possible operation when credentials may be compromised; blocking it increases the attack surface.

**What the design must address:** Extend the exemption clause to: "all endpoints except `POST /auth/change-password` and `POST /auth/logout` MUST return 403." Logout does not advance attacker capability and must not be blocked.

---

### F-R5-002 — Rate-limit failure count has no per-minute reset; "five per minute" semantics are not enforced MEDIUM

**Technique:** Assumption violation.

**Scenario:** The design states: "Login MUST tolerate at most five failures per source address per minute AND at most five failures per username per minute." It also states: "Exponential backoff to a 60-second ceiling on exhaustion." The `login_attempts` schema has `failure_count`, `next_allowed_at`, and `last_attempt_at` — no `window_started_at` column. Nothing in the design specifies that `failure_count` resets after a full minute passes without a failure. An attacker submits 4 failures, waits for `next_allowed_at` to elapse (backoff from 4 failures is well under 60 seconds), then submits 4 more failures, indefinitely. They never hit the 5-failure threshold that triggers the 60-second lockout ceiling, yet make unlimited brute-force progress across unlimited time.

**Impact:** The stated "five per minute" protection is not enforced. The design expresses rate semantics ("per minute") but implements lockout semantics ("backoff after 5"). Staying under 5 is sufficient to defeat the mechanism entirely.

**What the design must address:** Either (a) add a `window_started_at INTEGER` column to `login_attempts`, reset `failure_count` to 0 when `now - window_started_at >= 60` to enforce true sliding-window semantics; or (b) clarify that the design intends lockout-after-5 semantics only (not per-minute rate limiting), update the acceptance criterion to "five failures triggers backoff" instead of "five per minute," and remove the misleading "per minute" language. Option (a) is the stronger protection.

---

### F-R5-003 — Login endpoint accepts unbounded-length username strings; `login_attempts` storage grows without bound HIGH

**Technique:** Abuse case / resource exhaustion.

**Scenario:** The design specifies `login_attempts` with schema `(ip TEXT, username TEXT, failure_count INTEGER, next_allowed_at INTEGER, last_attempt_at INTEGER)`. No maximum length is specified for the username field stored there, nor for the username parameter accepted by `POST /api/v1/auth/login`. An unauthenticated attacker sends 1,000 POST /auth/login requests per second, each with a unique 64 KB random string as the username. Each request creates or updates a new `login_attempts` row (unique per username). The janitor prunes rows where `last_attempt_at < now - 300`. At 1,000 req/s × 300 s × 64 KB: ~19 GB of row data accumulates before the first prune cycle completes.

**Impact:** SQLite database and WAL file grow to tens of gigabytes from an entirely unauthenticated attack path. On constrained systems (embedded hardware, small VMs) this exhausts disk space and crashes the daemon. The design's rate-limiting infrastructure becomes the attack surface for a storage-exhaustion DoS.

**What the design must address:** The design must specify a maximum username length accepted by `POST /api/v1/auth/login` (e.g., 255 bytes). Requests exceeding this length MUST return 400 without touching `login_attempts`. This bound must be specified in the design as a constraint, not left to implementation.

---

### F-R5-004 — Bearer-token 401 rate-limit channel capacity unspecified MEDIUM

**Technique:** Composition failure — two fire-and-forget channels with asymmetric safety specifications.

**Scenario:** The login-attempts write channel is explicitly specified with capacity 4096 and drop-and-warn semantics: "on a full channel the send is dropped (best-effort) and a `tracing::warn!` event MUST be emitted and a counter incremented." The bearer-token 401 rate-limit channel has no analogous capacity specification: "Per-source rate limiting for bearer-token 401 responses MUST be fire-and-forget (async write, best-effort) via a dedicated low-priority channel." Under an invalid-bearer-token flood, if the channel consumer cannot keep up: an unbounded channel accumulates pending writes in memory; an implementation-chosen small capacity drops writes silently with no warn guarantee.

**Impact:** Under sustained invalid-bearer floods, the channel either grows without bound (memory pressure) or silently drops rate-limit records — allowing the attacking source to continue beyond its budget with no observable signal. The login-attempts channel has explicit observability guarantees; the bearer-token channel does not.

**What the design must address:** Specify the bearer-token 401 rate-limit channel with the same safety properties as the login-attempts channel: capacity 4096, drop on full, `tracing::warn!` + counter increment on drop. The two channels should be symmetric.

---

### F-R5-005 — `GET /api/v1/capabilities` response unspecified when probe cache is empty MEDIUM

**Technique:** Assumption violation — startup ordering.

**Scenario:** The design states: `GET /api/v1/capabilities` "MUST return the cached Caddy capability probe result." The probe runs asynchronously after startup. During the probe window (potentially minutes if Caddy is unreachable), the cache is empty. The design does not specify what the endpoint returns in this state: 503, 204, an empty object `{}`, or something else. A client that receives an empty capabilities object may interpret it as "Caddy supports no extended features" and refuse to apply mutations that require TLS or advanced Caddy directives.

**Impact:** Callers during the probe window may silently fail to apply valid mutations, or retry aggressively, producing excess load before the probe completes. The failure mode is silent — no error is returned, just a misleading empty-capabilities response.

**What the design must address:** Specify that `GET /api/v1/capabilities` MUST return 503 with body `{"status": "probe_pending"}` (or similar machine-readable body) when no probe result is cached yet. It MUST NOT return an empty capabilities payload that is indistinguishable from "Caddy supports nothing."

---

### F-R5-006 — `POST /drift/{event_id}/defer` on an already-resolved event is unspecified LOW

**Technique:** State manipulation.

**Scenario:** The design states: after 3 consecutive auto-defer failures, the system "set[s] `DriftEventRow.resolution = Deferred` and `resolved_at = now`" and writes `config.drift-auto-deferred`. Subsequently, an operator also POSTs to `POST /api/v1/drift/{event_id}/defer` for the same already-resolved event. The design does not specify whether this should be rejected. If accepted, it writes a second audit row — `config.drift-deferred` — for an event already terminated by `config.drift-auto-deferred`, producing two conflicting resolution records for the same `event_id`.

**Impact:** Audit queries filtering by drift resolution cause return contradictory records for the same event. Operators cannot determine whether the event was auto-deferred or explicitly deferred.

**What the design must address:** The design must specify that `POST /drift/{event_id}/defer` MUST return 409 with a body indicating the current resolution state if `DriftEventRow.resolution` is already set (whether to `Deferred`, `Reapplied`, `Accepted`, or `RolledBack`). Similarly specify 404 if `event_id` does not exist.

---

## Top concern

**F-R5-003** — The login endpoint accepts arbitrary-length username strings and persists them in `login_attempts` without a size bound. An unauthenticated attacker can fill the SQLite database to disk capacity within minutes by sending unique long-string usernames at high rate. This is a storage-exhaustion DoS accessible without any credentials.

**Recommended to address before implementation:** F-R5-003 (HIGH — storage exhaustion via unbounded username), F-R5-001 (MEDIUM — logout blocked under must_change_pw), F-R5-002 (MEDIUM — rate-limit window semantics clarification), F-R5-004 (MEDIUM — bearer-token channel capacity).
