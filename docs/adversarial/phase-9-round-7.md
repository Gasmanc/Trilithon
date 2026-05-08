# Phase 9 Adversarial Review — Round 7

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 2 |
| MEDIUM   | 1 |
| LOW      | 0 |

## Mitigations verified

The following Round 6 findings are cleanly addressed:

- **F-R6-001** — On total copy failure the temp file is NOT deleted; error includes temp file path.
- **F-R6-002** — Slow brute-force explicitly documented as out-of-scope for Phase 9.
- **F-R6-003** — System-generated audit rows carry `actor_kind="system"`, `actor_id="<task-name>"`.
- **F-R6-004** — `AuditLogStore::append` in HTTP handler wrapped in 500 ms timeout; 503 on timeout.

---

## Findings

### F-R7-001 — Crash after step (c) commits leaves DB and credentials file permanently out of sync HIGH

**Technique:** Cascade construction.

**Scenario:** The design states rotation ordering: "(c) UPDATE password_hash AND write auth.bootstrap-credentials-rotated audit row in one committed transaction; (d) rename() temp file to final path." And: "bootstrap_if_empty called exactly once before Tokio runtime spawns concurrent tasks" with idempotency: "user exists AND file exists → skip."

Consider: step (c) succeeds (DB has new hash, audit row committed, temp file exists at `bootstrap-credentials.tmp`). The process crashes between step (c) completing and step (d) being attempted. On restart, the idempotency check evaluates: user exists (yes) AND file exists (`bootstrap-credentials.txt` was never overwritten — rename never happened, so the old file is still there — yes) → **skip**. The bootstrap flow exits without completing the rotation. The DB holds the new hash; `bootstrap-credentials.txt` holds the old password; the old password no longer authenticates. The bootstrap account is locked out. The temp file (`bootstrap-credentials.tmp`) is left in place but the design provides no startup check for its presence.

**Impact:** A process crash in the rename window — a small but real window — permanently locks out the bootstrap account with no automatic recovery. The operator must manually inspect the filesystem and notice the temp file, understand it contains the new password, and manually rename it.

**What the design must address:** Add a startup check before the idempotency evaluation: if `<data_dir>/bootstrap-credentials.tmp` exists, treat this as evidence of an interrupted rotation and attempt to complete step (d) (rename, or copy-then-delete fallback) before evaluating idempotency. This is the natural recovery path — the temp file is the signal that the prior rotation committed to the DB but did not complete the file write.

---

### F-R7-002 — Auto-defer audit write and DriftEventRow update are not stated to be atomic; timeout gap leaves drift permanently unresolvable HIGH

**Technique:** Composition failure.

**Scenario:** The design states: "after 3 consecutive failures → write config.drift-auto-deferred, set DriftEventRow.resolution = Deferred, resolved_at = now, surface 409." These are two distinct writes: an `AuditLogStore::append` and a `DriftEventRow` UPDATE. The design also states: "AuditLogStore::append wrapped in 500 ms timeout; returns 503 if exceeded, without enqueuing."

The design does not state that the auto-defer writes (audit row + DriftEventRow update) execute within a single SQLite transaction. If the 500 ms timeout fires between the audit append completing and the DriftEventRow UPDATE being issued, the audit log contains `config.drift-auto-deferred` but `DriftEventRow.resolution` remains `None`. On the next call to `GET /api/v1/drift/current`, the event is returned as unresolved (resolution = None). The caller retries adopt/reapply, hits 3 more failures, writes a second `config.drift-auto-deferred` row, times out again. This cycle repeats indefinitely.

A process crash between the two writes produces the same stuck state without any timeout involvement.

**Impact:** Unbounded `config.drift-auto-deferred` audit rows for a single drift event. The event can never enter the `Deferred` terminal state. The drift resolution mechanism is permanently broken for that event.

**What the design must address:** Explicitly state that the auto-defer operation — audit row append (`config.drift-auto-deferred`) and `DriftEventRow` resolution update — MUST execute within a single committed SQLite transaction. The 500 ms `AuditLogStore::append` timeout MUST NOT apply to this combined terminal operation; it is a terminal state write, not a pre-enqueue gate, and partial failure is worse than latency.

---

### F-R7-003 — `LoginClearBothKeys` travels through the droppable fire-and-forget channel; a dropped clear leaves a locked-out authenticated user MEDIUM

**Technique:** Abuse case.

**Scenario:** The design states: "Successful login: LoginClearBothKeys { ip, username } fire-and-forget message, deletes both rows atomically." And: "Writes fire-and-forget via bounded mpsc channel capacity 4096; dropped sends emit tracing::warn! + counter." The design does not distinguish `LoginClearBothKeys` from failure-count write messages — both travel through the same bounded channel.

Under a burst of failed login attempts from other sources that saturates the channel to 4096, a `LoginClearBothKeys` message for a concurrent successful login is dropped (the design's stated drop behaviour). The authenticating user gets a valid session, but their `login_attempts` rows retain the prior failure count. If the user had 4 failures before their successful login and makes one more failed attempt (e.g., a parallel login from another device with the wrong password) within the same 60-second window, the rate limiter observes 5 total failures and applies the lockout — even though they have an active authenticated session. The janitor won't prune the stale rows for up to 300 seconds.

**Impact:** A legitimately authenticated user can be locked out of subsequent login attempts within a 5-minute window due to a dropped clear message. The design explicitly provides no priority distinction between clear messages and increment messages on the channel.

**What the design must address:** `LoginClearBothKeys` MUST NOT travel through the droppable fire-and-forget channel. Clearing stale rate-limit state on successful login is a correctness invariant, not a best-effort accounting write. The design must specify one of: (a) the successful-login handler performs a synchronous DELETE of both `login_attempts` rows directly (bypassing the channel entirely); or (b) `LoginClearBothKeys` messages are sent via a separate bounded channel with a higher capacity or non-dropping semantics. Option (a) is simpler and sufficient: the successful-login path is not on the hot path for brute-force floods and a synchronous delete adds negligible latency.

---

## Top concern

**F-R7-002** — the auto-defer operation's two writes (audit row + DriftEventRow update) are not stated to be atomic, and the existing 500 ms timeout on `AuditLogStore::append` creates a concrete path where only one write completes, leaving drift events permanently unresolvable with unbounded audit row accumulation.

**Recommended to address before implementation:** F-R7-002 (MUST be atomic transaction — the timeout must explicitly not apply), F-R7-001 (resume-from-temp-file startup check — prevents silent bootstrap lockout on crash), F-R7-003 (synchronous delete on successful login — correctness invariant, not accounting write).
