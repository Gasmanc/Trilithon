# Phase 9 Adversarial Review — Round 3

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 2 |
| HIGH | 4 |
| MEDIUM | 3 |
| LOW | 2 |

## Mitigations verified

The following Round 2 findings are cleanly addressed with no residual gaps:

- **F-R2-006** — `/api/v1/openapi.json` requiring auth is correctly specified.
- **F-R2-007** — session-cookie auth documented as browser-only; bearer tokens for non-browser callers. Documented and consistent.

---

## Findings

### F-R3-001 — `config.drift-deferred` absent from both vocabulary files CRITICAL

**Technique:** Incomplete mitigation of F-R2-004.

**Scenario:** Round 2 finding F-R2-004 required `config.drift-deferred` to be added to `AUDIT_KINDS` in `crates/core/src/storage/audit_vocab.rs` and a corresponding `AuditEvent::DriftDeferred` variant to `crates/core/src/audit.rs` before the drift endpoints are implemented. Neither file has been updated: `AUDIT_KINDS` contains `config.drift-resolved` and `config.drift-detected` but not `config.drift-deferred`; `audit.rs` has `DriftDetected` and `DriftResolved` but no `DriftDeferred`. When the drift adopt/reapply endpoint exhausts its 3 retries and attempts to write `config.drift-deferred`, the audit insert fails at runtime with `StorageError::AuditKindUnknown`. The endpoint returns 500, but the drift event is already left with 3 `mutation.conflicted` rows and no terminal state. The F-R2-004 mitigation is currently non-functional.

**Impact:** The auto-defer path — the entire recovery mechanism for perpetually-conflicting drift — crashes at runtime the first time it is triggered. Every subsequent retry generates 3 more `mutation.conflicted` rows and crashes again.

**What the design must address:** Add `config.drift-deferred` to `AUDIT_KINDS` and `AuditEvent::DriftDeferred` to `audit.rs` before any drift endpoint code is written. Update any `AUDIT_EVENT_VARIANT_COUNT` or `all_variants()` test helper to include the new variant.

---

### F-R3-002 — Implementation TODO slice 9.6 still performs full Argon2id scan; contradicts both F002 and F004 mitigations CRITICAL

**Technique:** Incomplete mitigation — design divergence between phase reference and implementation TODO.

**Scenario:** The phase reference specifies token authentication using a prefix index: extract the first 16 hex chars, query by prefix, Argon2id only on the candidate row. The TODO's slice 9.6, algorithm step 4, reads verbatim: "Hash the token via Argon2id and compare against `tokens.token_hash`. On match, attach `AuthContext::Token`." This is the original pre-mitigation algorithm — no prefix extraction, no O(1) index query. Additionally, the `AuthContext::Token` variant in the TODO carries no `user_id` field, so the `must_change_pw` check from F004's mitigation is also absent from the middleware algorithm. A developer implementing from the TODO faithfully reproduces both F002 (per-request Argon2id DoS vector) and F004 (no `must_change_pw` enforcement for token auth) — both previously marked as mitigated.

**Impact:** The `token_prefix` index, prefix-based O(1) lookup, and `must_change_pw` gating for token auth are all missing from the implementer's specification. An implementation following the TODO restores the F002 DoS and F004 privilege bypass simultaneously.

**What the design must address:** Rewrite the TODO's slice 9.6 algorithm step 4 to match the phase reference: (1) extract prefix from bearer value; (2) query `tokens` by `token_prefix`; (3) Argon2id only on candidate. Add the step for loading the owning user row and checking `must_change_pw`. Update `AuthContext::Token` to include `user_id` and `must_change_pw`.

---

### F-R3-003 — Bootstrap rotation ordering inverted; "roll back the DB update" is logically impossible HIGH

**Technique:** Incomplete mitigation of F-R2-003.

**Scenario:** The design says: "write new password to temp file, UPDATE `password_hash` + write `auth.bootstrap-credentials-rotated` audit row in one transaction, then `rename()` temp file. If the rename fails, roll back the DB update." A committed SQLite transaction cannot be rolled back. The phrase describes a compensating UPDATE that would require re-knowing the old password — which is not stored anywhere after the commit. If the rename fails after the DB transaction commits, the audit log already contains `auth.bootstrap-credentials-rotated` describing a rotation that didn't complete, and there is no safe recovery path: the DB has the new hash, the file was never renamed, and the old password is irretrievable.

The correct ordering is: write temp file FIRST, commit DB update SECOND, rename THIRD. If DB commit fails → delete temp file (no audit row was written). If rename fails → DB is consistent (new hash committed); the temp file still exists and rename can be retried without re-computing the password.

**Impact:** If the rename fails after DB commit, the bootstrap account is permanently locked (DB has a hash for a password never persisted to disk), with a misleading `auth.bootstrap-credentials-rotated` audit row. No recovery path without manual DB surgery.

**What the design must address:** Reorder to: (1) write temp file; (2) commit DB UPDATE + `auth.bootstrap-credentials-rotated` in one transaction; (3) `rename()`. Failure handling: step 2 fails → delete temp file, done. Step 3 fails → DB consistent, retry the rename (the temp file still has the correct password). Update the phase doc's bootstrap rotation task to reflect this ordering.

---

### F-R3-004 — `mutation.submitted` ordering before `mutation.applied` is not guaranteed by the async audit channel HIGH

**Technique:** New composition failure introduced by F-R2-001 fix.

**Scenario:** The HTTP handler writes `mutation.submitted` to the audit channel before enqueuing the mutation. The applier processes the mutation and writes `mutation.applied` + `config.applied` to the same audit channel. Both paths are async. The Tokio scheduler can execute the applier task between the HTTP handler's channel send and the audit writer's drain of that message. If the applier commits and sends its audit rows before the audit writer processes `mutation.submitted`, the physical ordering in the `audit_log` table (by insertion time) will show `mutation.applied` before `mutation.submitted` for the same `correlation_id`. The acceptance test ("a test asserts `mutation.submitted` appears before `mutation.applied`") will pass in serial test execution but is not deterministic under concurrent load.

**Impact:** Forensic queries for "submitted but not yet applied" mutations — filtering on `mutation.submitted` rows without a matching `mutation.applied` — produce false positives when the insertion order is reversed. The audit log's defined ordering invariant is probabilistic, not guaranteed.

**What the design must address:** Either (a) write `mutation.submitted` synchronously to the DB (not through the audit channel) before enqueuing the mutation, guaranteeing the row exists before any applier rows can be inserted; or (b) explicitly accept that ordering is probabilistic and remove the ordering requirement from the acceptance test, relying on `correlation_id` linkage instead. Option (a) is the sound choice; option (b) weakens the forensic guarantee.

---

### F-R3-005 — Successful login clears only the IP row; username backoff survives and is asymmetrically bypassable HIGH

**Technique:** Incomplete mitigation of F-R2-005.

**Scenario:** The phase reference states: "A successful login MUST delete the `login_attempts` row for the source IP." The `login_attempts` table has two logical keys: `ip` and `username`. Deleting the IP row on successful login leaves the username row intact. An operator who successfully logs in after triggering username-based backoff (5 failed attempts targeting their username from different IPs) finds themselves still subject to username backoff on their next failed attempt. Conversely, an attacker who exhausted their IP budget can log in successfully from a different IP (or path) to clear their IP row, then immediately retry against the username without the IP backoff. Additionally, if the fire-and-forget channel is full at login time, neither deletion goes through and both rows survive with pre-login `failure_count` values.

**Impact:** Legitimate operators can be trapped in permanent username backoff after a successful login cleared only the IP row. An attacker can engineer username backoff for a targeted account from other IPs while evading the per-IP limit.

**What the design must address:** Successful login MUST delete both the `ip` row and the `username` row for the authenticated user. Both deletions should be a single fire-and-forget channel message (`LoginClearBothKeys { ip, username }`) so they are atomically delivered or atomically dropped together.

---

### F-R3-006 — TODO slice 9.3 implements in-memory rate limiter; contradicts F005 SQLite-persisted requirement MEDIUM

**Technique:** Design divergence between phase reference and implementation TODO.

**Scenario:** The phase reference requires rate-limit state to be persisted in the `login_attempts` SQLite table so restarts do not reset budgets. The TODO's slice 9.3 declares `pub struct LoginRateLimiter { /* DashMap<IpAddr, BucketState> */ }` — a purely in-memory structure with no SQLite interaction. If a developer implements from the TODO, they build an in-memory rate limiter that resets on every daemon restart, exactly reproducing F005. The acceptance test "a test restarts the daemon mid-backoff and asserts the backoff is still enforced" will fail against this implementation.

**Impact:** The F005 mitigation (persisted rate-limit state) is absent from the implementer's specification. A restart (including ordinary `systemctl restart`) resets all rate-limit state.

**What the design must address:** Rewrite TODO slice 9.3's `LoginRateLimiter` to use the `login_attempts` SQLite table with fire-and-forget writes (per F-R2-008). The dual-keyed schema (IP + username) must be explicit in the algorithm.

---

### F-R3-007 — Fire-and-forget channel capacity is unspecified; at saturation rate limiting silently stops MEDIUM

**Technique:** Incomplete mitigation of F-R2-008.

**Scenario:** The design specifies rate-limit state writes as "fire-and-forget via a dedicated low-priority async channel" but never specifies the channel's capacity. A bounded mpsc channel that fills up silently drops sends — by design. Under a sustained credential-stuffing attack, if the channel fills, every rate-limit increment is dropped. After 300 seconds the `login_attempts` rows are pruned by the janitor. The attacker — whose increments have been dropped throughout — has net zero entries and encounters no backoff. There is no metric, no log line, and no observable signal that the rate limiter has ceased to function. An unbounded channel avoids drops but accumulates unbounded pending messages under attack, reintroducing memory pressure.

**Impact:** Under sustained high-rate attack, the rate limit silently stops functioning with no observable indication. The F002 original DoS vector returns under load without any alert.

**What the design must address:** Specify the channel capacity (e.g. 4096) and require a `tracing::warn!` emission and a counter increment when a send is dropped due to a full channel. This converts silent degradation into an observable event.

---

### F-R3-008 — Explicit defer and auto-defer produce identical audit kind; forensic ambiguity LOW

**Technique:** Audit integrity gap.

**Scenario:** Both `POST /api/v1/drift/{event_id}/defer` (explicit operator action) and the auto-defer path after 3 consecutive failures write `config.drift-deferred`. An operator reviewing the audit log after an incident cannot determine whether a drift event was explicitly deferred by an operator decision or silently auto-deferred by the retry exhaustion logic. The two outcomes have different operational implications: explicit deferral is a conscious choice; auto-deferral means the system gave up due to contention and the operator may not have noticed.

**Impact:** The audit log cannot distinguish operator intent from system automation for the same state transition. For a tool whose audit log is a forensic record, conflating the two is a silent integrity gap.

**What the design must address:** Use distinct audit kinds (`config.drift-deferred` for explicit operator deferral; `config.drift-auto-deferred` for exhaustion-triggered deferral), or require a `trigger` field in the `notes` JSON (`"explicit"` vs `"exhaustion"`). Both kinds would need to be added to `AUDIT_KINDS`.

---

### F-R3-009 — Health endpoint live before startup checks complete; aggressive liveness probe can trigger restart loop LOW

**Technique:** Race condition — startup ordering.

**Scenario:** The HTTP server starts and begins serving `/api/v1/health` before the startup sequence (bootstrap flow, migration check, capability probe) completes. During the startup window the endpoint correctly returns 503. However, container orchestrators configured with a liveness probe (not a startup probe) treat any non-2xx as "unhealthy" and restart the container. If the liveness probe fires during the bootstrap window (200–500ms due to Argon2id hashing), a restart is triggered. `bootstrap_if_empty` is called again on a partially-bootstrapped state, triggering the rotation path, which generates a new Argon2id hash — extending the startup window further. This creates a progressive restart loop with exponentially compounding startup latency.

**Impact:** In container environments with aggressive liveness probe configuration, the bootstrap window can trigger a restart loop causing permanent startup failure. The failure is non-obvious and environment-dependent.

**What the design must address:** Document the startup window explicitly in `docs/api/README.md` (already required by the design): liveness probes should use an initial delay of at least 10 seconds, or operators should use a startup probe distinct from the liveness probe. No code change required; documentation is sufficient.

---

## Top concern

**F-R3-002** — the implementation TODO's slice 9.6 directly contradicts the two most important security mitigations in the entire Phase 9 design (F002 bearer-token DoS and F004 must_change_pw bypass). The TODO is the document developers implement from. This is the most likely path to critical vulnerabilities in the final code.

**Recommended to address before implementation:** F-R3-001 (vocabulary gap — runtime crash on first drift exhaustion), F-R3-002 (TODO divergence — rebuilds F002 + F004 in the code), F-R3-003 (rotation ordering inversion — corrupts bootstrap account on rename failure), F-R3-006 (TODO in-memory rate limiter — rebuilds F005).
