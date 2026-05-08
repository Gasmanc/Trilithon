# Phase 9 Adversarial Review — Round 6

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 2 |
| MEDIUM   | 2 |
| LOW      | 0 |

## Mitigations verified

The following Round 5 findings are cleanly addressed:

- **F-R5-001** — `POST /auth/logout` exempted from the `must_change_pw` 403 gate alongside `change-password`.
- **F-R5-002** — `window_started_at INTEGER` column added to `login_attempts`; `failure_count` resets when `now - window_started_at >= 60`. True sliding-window semantics specified.
- **F-R5-003** — `POST /auth/login` rejects username > 255 bytes with 400 before touching `login_attempts`; schema has `CHECK(length(username) <= 255)`.
- **F-R5-004** — Bearer-token 401 rate-limit channel explicitly bounded to 4096 with drop-and-warn matching the login-attempts channel.
- **F-R5-005** — `GET /capabilities` returns 503 `{"status": "probe_pending"}` when no probe result cached.
- **F-R5-006** — `POST /drift/{id}/defer` returns 409 if resolution already set, 404 if event not found.

---

## Findings

### F-R6-001 — Rotation total-failure path deletes the temp file, permanently locking out the bootstrap account HIGH

**Technique:** Cascade construction.

**Scenario:** The design states rotation step (c): "UPDATE password_hash AND write auth.bootstrap-credentials-rotated audit row in a single committed SQLite transaction." Step (d): "rename() the temp file." Failure handling: "If rename() returns EXDEV, or if all 3 retries are exhausted → fall back to copy-then-delete." Total failure: "On total failure (copy also fails) → return error; DB is still consistent; temp file cleaned up."

A concrete failure sequence: step (c) commits the new hash to the DB. The target filesystem is full; all 3 rename retries fail. The copy-then-delete fallback also fails (same root cause: no free disk space). The design says to clean up the temp file and return an error. The DB now contains the new hash. The bootstrap-credentials.txt file still contains the old password (it was never replaced). The temp file — the only location where the new plaintext password was written — has been deleted. The old password no longer matches the committed hash. The new password is gone. The operator cannot log in with either value.

**Impact:** Permanent denial of access to the only privileged account with no in-band recovery path. The design says "DB is still consistent" — this is true but irrelevant. Consistency here means the DB has an irrecoverable hash committed for a password that no longer exists anywhere.

**What the design must address:** On total failure (copy-then-delete fails after all rename retries), do NOT delete the temp file. Instead, leave the temp file in place as a recovery artifact and include its path in the error message returned to the caller. This lets an operator manually copy the file and retry. The design should state: "On total failure (copy also fails) → do NOT delete the temp file; return an error that includes the temp file path so the operator can complete the rotation manually."

---

### F-R6-002 — Janitor prune erases sliding-window state, enabling unbounded slow brute-force HIGH

**Technique:** Composition failure.

**Scenario:** The design states: "at most five failures per source address within any 60-second sliding window AND at most five failures per username within any 60-second sliding window." It also states: "A janitor task MUST prune rows where `last_attempt_at < now - 300`."

An attacker targets a username. They submit 4 failed attempts (one below the 5-failure threshold). They wait 301 seconds. The janitor prunes the row because `last_attempt_at < now - 300`. A fresh row is created on the next attempt with `failure_count = 0`. The attacker submits 4 more failed attempts. They repeat this pattern indefinitely: 4 attempts every 301 seconds = ~48 attempts per hour = ~1,152 attempts per day — all below the lockout ceiling, all erased by the janitor.

The sliding-window mechanism only bounds per-minute burst rate. It does not bound the total number of attempts against a given username over any longer horizon because the janitor erases all memory of prior failures.

**Impact:** A slow, persistent brute-force attack against a specific username makes unbounded progress. The design's stated goal of "at most five failures per minute" is met — each 60-second window has at most 4 attempts — but the aggregate rate is unlimited.

**What the design must address:** Either (a) accept that slow brute-force (< 5 per burst, inter-burst gaps > 5 minutes) is explicitly out of scope for Phase 9 and document this as a known limitation — rely on password strength, monitoring, and account lockout in a future phase; or (b) extend the janitor prune window to a longer horizon (e.g., 24 hours) and add an aggregate failure threshold per username (e.g., if total failures in the retention window exceed 50, apply a 24-hour lockout). Option (a) is acceptable for Phase 9 as long as the limitation is documented explicitly.

---

### F-R6-003 — Background-task audit rows (config.drift-auto-deferred) have no specified actor identity MEDIUM

**Technique:** Assumption violation.

**Scenario:** The design specifies: "after 3 consecutive failures, write config.drift-auto-deferred." This write originates from the drift applier background task — not from an authenticated HTTP caller. Audit rows presumably carry `actor_kind` and `actor_id` fields (the design's snapshot summary projection lists `actor_kind` and `actor_id` as standard fields). The drift applier has no session, no bearer token, and no user identity. The design does not specify what actor attribution system-generated audit rows should use.

If the implementation uses `actor_kind = null` and `actor_id = null`, this may violate a schema NOT NULL constraint. If it uses the last conflicting mutation's actor, the audit log misleadingly attributes the auto-deferral to that operator. If it uses an ad-hoc sentinel, different implementations may choose different values.

**Impact:** System-generated audit rows are either schema-invalid (null where NOT NULL is expected), misleading (wrong actor), or inconsistent across implementations. An operator reviewing the audit log cannot reliably identify which rows originated from background tasks vs. authenticated operators.

**What the design must address:** Define a system-actor convention: background-task-generated audit rows MUST carry `actor_kind = "system"` and `actor_id = "<task-name>"` (e.g., `"drift-applier"` for the drift resolution task, `"bootstrap"` for the bootstrap flow). This convention MUST be stated in the design so all implementations agree on the values.

---

### F-R6-004 — Synchronous `mutation.submitted` write in the HTTP handler serialises all mutation requests through the SQLite writer MEDIUM

**Technique:** Composition failure.

**Scenario:** The design states: "HTTP handler MUST write mutation.submitted audit row synchronously and directly to the database (via AuditLogStore::append, NOT through async audit writer channel) before enqueuing the mutation." The design also states: "Invalid-token floods MUST NOT contend with the SQLite WAL writer used by mutations and snapshots" — justifying the fire-and-forget channel for bearer-token rate-limit writes.

The mutation handler now holds a synchronous SQLite write lock in the HTTP handler layer. SQLite's WAL mode serialises all writers. Under concurrent mutation requests, each HTTP handler task holds the write lock for the duration of the `AuditLogStore::append` call. If the applier is simultaneously committing a snapshot (another SQLite write), all mutation handler tasks queue behind it. Under high mutation request volume, all Tokio worker tasks may be simultaneously blocked on SQLite writes, saturating the thread pool and delaying health checks, auth requests, and drift reads.

**Impact:** A legitimate burst of mutation requests causes cascading latency across all API endpoints. The fire-and-forget isolation design for bearer-token floods protects mutations from auth noise, but the design does not bound how long the mutation handler can hold the SQLite write lock.

**What the design must address:** Specify a maximum write timeout on `AuditLogStore::append` in the HTTP handler (e.g., 500 ms). If the append does not complete within this timeout, the HTTP handler MUST return 503 (without enqueuing the mutation). This bounds the maximum time any mutation handler task holds the write lock, preventing thundering-herd serialisation under burst load.

---

## Top concern

**F-R6-001** — The rotation total-failure path deletes the only copy of the new plaintext password after the DB commit, permanently locking out the bootstrap account with no recovery path.

**Recommended to address before implementation:** F-R6-001 (HIGH — preserve temp file on total failure), F-R6-003 (MEDIUM — system actor convention must be specified before any background-task audit writes are implemented).
