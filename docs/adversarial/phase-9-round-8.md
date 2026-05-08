# Phase 9 Adversarial Review — Round 8

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 1 |
| MEDIUM   | 2 |
| LOW      | 1 |

## Mitigations verified

The following Round 7 findings are cleanly addressed:

- **F-R7-001** — Startup checks for `bootstrap-credentials.tmp` before idempotency eval; if present, completes the rename without generating a new password.
- **F-R7-002** — Auto-defer writes (audit row + DriftEventRow update) execute in a single SQLite transaction; the 500 ms timeout does not apply.
- **F-R7-003** — Successful-login DELETE of both `login_attempts` rows is synchronous in the handler, not via the droppable channel.

---

## Findings

### F-R8-001 — Interrupted-rotation recovery renames temp file without verifying step (c) committed HIGH

**Technique:** Assumption violation.

**Scenario:** The design states: "Interrupted-rotation recovery: before idempotency check, if `bootstrap-credentials.tmp` exists, attempt step (d) using existing temp file (rename or copy-then-delete) without generating new password." The design defines the rotation crash windows as: step (c) = "UPDATE password_hash + write auth.bootstrap-credentials-rotated in single transaction"; step (d) = "rename() temp to final."

The design's recovery assumes that if `bootstrap-credentials.tmp` exists, step (c) already committed. But the temp file is written at step (b) — before step (c). If the process crashes between step (b) and step (c), `bootstrap-credentials.tmp` exists but the DB still holds the old password hash. Recovery renames the temp file to `bootstrap-credentials.txt`, making the new plaintext the operator's visible credential. But the DB verifies against the old hash. Every subsequent login attempt fails: the file shows the new password, the DB expects the old one.

**Impact:** The bootstrap account becomes permanently inaccessible. The operator sees the new password in the file, tries it, fails, and has no recovery path — neither the old nor the new password authenticates. The design provides no detection mechanism for this state.

**What the design must address:** The recovery check must gate on DB state, not just filesystem state. Before completing step (d), verify whether step (c) committed by querying whether an `auth.bootstrap-credentials-rotated` audit row exists with a timestamp more recent than `bootstrap-credentials.txt`'s last-modified time. If such a row exists, step (c) committed and renaming the temp file is safe. If no such row exists (or the most recent such row predates the credentials file), the crash occurred before step (c) — the temp file must be deleted and the full rotation sequence (steps a–d) must be re-run from the beginning.

---

### F-R8-002 — `mutation.conflicted` rows written during drift resolution retries have no stated actor attribution MEDIUM

**Technique:** Composition failure.

**Scenario:** The design states: "System audit rows: actor_kind='system', actor_id='drift-applier' (or 'bootstrap')." It also states that drift adopt/reapply retries "each writes mutation.conflicted." The `mutation.conflicted` rows are written by the drift applier — a background task with no authenticated caller identity. The design explicitly assigns `actor_kind="system"` to `config.drift-auto-deferred`, but does not explicitly extend this convention to `mutation.conflicted` rows emitted during drift resolution retries.

The mutation endpoint section defines `mutation.conflicted` in the HTTP-handler context where the caller's identity is available. In the drift-applier context, no caller identity exists. Without an explicit statement that `mutation.conflicted` rows in the drift context also use `actor_kind="system"`, an implementation could leave these fields null, use the last HTTP caller's identity, or use a different sentinel — producing inconsistent audit attribution for the same audit event kind.

**Impact:** `mutation.conflicted` rows in the audit log for drift-triggered retries are attributed inconsistently, breaking forensic queries that join on `actor_kind` or `actor_id` to distinguish operator-submitted mutations from drift-resolution attempts.

**What the design must address:** Explicitly state that all audit rows written during drift resolution (including each `mutation.conflicted` row per retry) MUST carry `actor_kind="system"` and `actor_id="drift-applier"`. The system-actor convention stated for `config.drift-auto-deferred` applies equally to every audit row the drift applier produces.

---

### F-R8-003 — 503 from `POST /mutations` is indistinguishable as retriable vs. fatal; API contract is silent on backoff MEDIUM

**Technique:** Cascade construction.

**Scenario:** The design states: "AuditLogStore::append wrapped in 500 ms timeout; 503 if exceeded, without enqueuing." The design also states: "Invalid-token floods MUST NOT contend with the SQLite WAL writer" — justifying fire-and-forget writes for bearer-token rate-limit increments. However, the mutation.submitted write is synchronous and competes with the SQLite WAL writer. If the drift applier holds the write lock (e.g., committing a large snapshot), the mutation handler's append will block until the lock is released. At 500 ms the handler returns 503. This is not an audit-store fault — it is normal WAL serialisation under concurrent load.

Callers receive a 503 from `POST /mutations` and have no way to distinguish: (a) WAL contention backpressure (transient, safe to retry immediately) from (b) audit store structural fault (persistent, retry will likely fail again). The design provides no retry semantics for the 503 case.

**Impact:** Well-behaved clients following REST conventions for 503 may use exponential backoff that is unnecessarily long for the transient WAL case. Aggressively-retrying clients may flood the mutation endpoint during contention, worsening the condition. The absence of a `Retry-After` header or documented retry semantics leaves all callers to guess.

**What the design must address:** Document in `docs/api/README.md` (already required by the design) that a 503 from `POST /mutations` is always a transient condition and is safe to retry after a short delay. Optionally, specify that the endpoint SHOULD include a `Retry-After: 1` header on 503 to signal retriability. This converts an ambiguous error into a documented, actionable response.

---

### F-R8-004 — Gateway tokens have no stated role; role-restricted endpoints lack a defined access decision for token auth LOW

**Technique:** Assumption violation.

**Scenario:** The design states: "`GET /api/v1/audit`: Restricted to Operator and Owner; Reader → 403." It also states middleware validates "session cookies against sessions and tool-gateway tokens against gateway_tokens." The design defines role-based restrictions only in session-auth terms (Operator, Owner, Reader). The `tokens` / `gateway_tokens` table schema is not described in this design, and the design does not state whether gateway tokens carry a role, inherit the issuing user's role at issuance, or have a fixed privilege level.

If tokens have no stated role, the role check for `GET /api/v1/audit` has no value to compare against for token-authenticated requests. An implementation could default to "no role → allowed" (permissive), giving any token holder full audit log access regardless of the issuing user's role. Or it could default to "no role → denied," breaking tool-gateway automation that legitimately needs audit access.

**Impact:** Role-restricted endpoints are ambiguously enforced for token-authenticated callers. A permissive default constitutes a privilege escalation path for any token holder.

**What the design must address:** Specify whether `gateway_tokens` / `tokens` rows carry an explicit role field, or whether tokens inherit the creating user's role at issuance time (snapshot semantics — role frozen at creation). The role check for all role-restricted endpoints MUST apply equally to both `AuthContext::Session` and `AuthContext::Token`, and the source of the role value for each context must be unambiguous.

---

## Top concern

**F-R8-001** — the interrupted-rotation recovery renames the temp file based solely on its filesystem existence, without verifying whether the DB update committed. A crash between step (b) and step (c) leaves the file with the new password and the DB with the old hash, permanently locking out the bootstrap account with no recovery path.

**Recommended to address before implementation:** F-R8-001 (HIGH — recovery must gate on DB state), F-R8-002 (MEDIUM — drift-applier audit rows need explicit actor attribution), F-R8-004 (LOW — token role semantics must be defined before implementing role-restricted endpoints).
