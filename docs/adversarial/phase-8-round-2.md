# Phase 8 Adversarial Review — Round 2

**Date:** 2026-05-08
**Severity summary:** 3 critical · 4 high · 3 medium · 1 low

---

## Round 1 Findings Still Load-Bearing (fix before coding)

| Finding | Severity | Blocks slices |
|---------|----------|--------------|
| F001 | CRITICAL | 8.6 — any implementation of `record()` will bake the wrong commit-point into the hash; also makes F021 worse because the hash is set whether the write succeeded or not |
| F002 | CRITICAL | 8.1, 8.6 — `apply_diff` taking `&Diff` means the type gap between raw diff and redacted diff is present at every call site; fixing it after code exists requires signature changes across core and adapters |
| F004 | HIGH | 8.5 — the mutex guard drop point governs whether `apply_diff` and the Caddy fetch are atomic; any implementation written before this is specified will assume an unsafe drop point |
| F007 | HIGH | 8.4, 8.5 — `reapply_desired_state` using `before_snapshot_id` without a currency check is the only safe-but-wrong resolution path; fixing it after integration tests are written requires redesigning the snapshot lookup contract |
| F008 | HIGH | 8.5 — without a timeout on `GET /config/`, the apply mutex can stall indefinitely; every integration test that exercises the scheduler will pass without hitting this |

---

## New Findings (Round 2)

### F016 — `Mutation::ReplaceDesiredState` and `Mutation::ReapplySnapshot` do not exist in the mutation enum [CRITICAL]

**Category:** Composition failures

**Attack:** Phase 8 resolution APIs return `Mutation::ReplaceDesiredState` and `Mutation::ReapplySnapshot`. These variants do not exist in the current `Mutation` enum. Adding them requires updating `apply_mutation` (which matches exhaustively over every variant), `Mutation::expected_version()`, and the optimistic concurrency guard in the applier. The design specifies no `expected_version` on these variants, which means the OCC guard never fires for drift resolutions. A stale resolution applied to a state that has advanced since detection silently overwrites the newer state — exactly the failure hazard H8 was designed to prevent.

**Scenario:**
1. Drift detected at `config_version = 42`. `DriftEvent.before_snapshot_id = S-10`.
2. Operator applies a legitimate config change. `config_version` advances to `43`.
3. Operator (concurrently) resolves the drift via `adopt_running_state`. Emits `Mutation::ReplaceDesiredState`.
4. Because `ReplaceDesiredState` carries no `expected_version`, the OCC guard does not fire.
5. The adopt mutation overwrites version 43 with the running state from step 1.
6. The legitimate config change is silently lost.

**Design gap:** The design treats resolution mutations as first-class pipeline objects but specifies them only in terms of their production, not their pipeline integration. Every component that pattern-matches on `Mutation` must be updated. The design leaves this entirely unspecified and the OCC integration unresolved.

---

### F017 — `adopt_running_state` produces a `DesiredState` with no `version`, breaking optimistic concurrency for all subsequent mutations [CRITICAL]

**Category:** State machine gaps

**Attack:** `adopt_running_state` clones `running_state: &DesiredState` parsed from `GET /config/`. Caddy's JSON carries no Trilithon `config_version`. The resulting `DesiredState.version` is either the struct default (zero) or whatever serialised form happened to be in the Caddy response. If persisted with version 0, the next mutation carries `expected_version = 43` (from the UI's cached state), which fails the OCC check. Alternatively, if version 0 becomes the new baseline, all clients with cached version 43 generate unresolvable conflicts until they manually refresh.

**Scenario:**
1. Desired state is at `config_version = 43`. Running state parses from Caddy JSON; `version` field is absent → defaults to 0.
2. `adopt_running_state` clones it; `Mutation::ReplaceDesiredState { new_state.version = 0 }`.
3. Mutation applied. New snapshot has `config_version = 0` in SQLite.
4. Every UI client submits mutations with `expected_version = 43`. All are rejected with OCC conflict.
5. Operators cannot submit any mutations until the version discrepancy is diagnosed manually.

**Design gap:** A `DesiredState` obtained by parsing Caddy's JSON has no Trilithon-managed `version`. The adoption path must copy the current desired state's `config_version` (or explicitly advance it) before inserting a snapshot. The design specifies none of this.

---

### F018 — `adopt_running_state` copies the running Caddy state without the ownership sentinel, violating ADR-0015 [CRITICAL]

**Category:** Trust boundary violations

**Attack:** ADR-0015 mandates that every Trilithon-applied snapshot includes the ownership sentinel. If the sentinel is removed out-of-band, the drift loop detects its absence as a `Removed` diff entry. An operator who reasonably selects `adopt_running_state` to accept the current state clones a sentinel-free `DesiredState`. The next `POST /load` removes the sentinel from Caddy entirely. On the next daemon restart, `GET /id/trilithon-owner` returns 404; the daemon re-claims. If two Trilithons are running (hazard H12), the second claims the instance. ADR-0015's entire multi-instance protection is nullified by a single operator action that appears safe.

**Scenario:**
1. Sentinel removed out-of-band.
2. Drift detected. Operator sees "1 removal" drift event and chooses `adopt_running_state`.
3. `running_state.clone()` has no sentinel. `POST /load` confirms the sentinel-free config.
4. Next restart: daemon re-claims, or a second Trilithon instance claims ownership.

**Design gap:** `adopt_running_state` performs no sentinel-presence check before accepting the running state. The design needs a guard that either rejects adoption of a sentinel-free state, or re-injects the sentinel into the adopted state before persisting.

---

### F019 — `DriftEvent` and `DriftEventRow` are structurally incompatible; the redacted/unredacted split is unspecified [HIGH]

**Category:** Semantic drift between layers

**Attack:** The design's `DriftEvent` (Slice 8.3) has `redacted_diff_json: String` and `redaction_sites: u32`. The existing `DriftEventRow` in storage uses `diff_json: String` (no `redaction_sites`), `snapshot_id` (not `before_snapshot_id`), and `detected_at: UnixSeconds` (newtype, not raw `i64`). The `record()` call in Slice 8.6 passes a `DriftEvent` to `storage.record_drift_event()`, but no mapping is specified. The critical question: does `diff_json` in the row store the redacted or unredacted diff? If redacted (correct for the audit log), the unredacted diff is lost after the tick — unrecoverable for internal reconciliation. If unredacted (to preserve detail), the row contains plaintext secrets in violation of ADR-0009 and hazard H10.

**Design gap:** The design introduces `DriftEvent` as an intermediate value but specifies no mapping to `DriftEventRow`. The decision of which diff variant goes into SQLite is unanswered, and getting it wrong either leaks secrets or loses information needed for the dual-pane editor.

---

### F020 — `last_running_hash` is in-memory only; daemon restart duplicates drift audit rows indefinitely [HIGH]

**Category:** State machine gaps

**Attack:** `last_running_hash` is initialised to `None` on every daemon start. On the first post-restart tick, the dedup check always misses, and a new `config.drift-detected` row is written even if the same drift was already recorded before shutdown and was never resolved.

**Scenario:**
1. Drift detected at 09:00. Audit row written. Operator has not resolved.
2. Daemon restarted 12 times over the next 24 hours (routine deployments).
3. Each restart's first tick writes a new `config.drift-detected` row for the same unresolved drift.
4. 12 identical rows accumulate. Operator audit review shows an apparent storm of "new" detections for a drift that has been present since 09:00.

**Design gap:** Deduplication is in-memory only. On startup, `last_running_hash` should be initialised from `storage.latest_drift_event()` to resume cross-restart deduplication. The `DriftEventRow` contains the `snapshot_id` and hash needed for this. The design does not specify this initialisation step.

---

### F021 — `defer_for_manual_reconciliation` does not specify whether `mark_resolved` is called; dedup hash permanently silences re-detection [HIGH]

**Category:** Missing invariant enforcement

**Attack:** `record()` sets `*guard = Some(event.running_state_hash)`. `mark_resolved()` resets it to `None`. `defer_for_manual_reconciliation` returns a no-op `Mutation::DriftDeferred`. The design does not specify whether `mark_resolved` is called after a deferral or only after `adopt`/`reapply`. If `mark_resolved` is NOT called: `last_running_hash` stays set; every subsequent tick silently deduplicates; the drift persists without further audit rows. If `mark_resolved` IS called immediately after deferral: `last_running_hash` resets to `None`; the next tick re-detects the same drift and writes a new `config.drift-detected` row — making deferral semantically identical to doing nothing.

**Design gap:** The design must specify exactly when `mark_resolved` is called relative to each resolution path. Deferral semantics need a TTL (e.g., suppress re-detection for 24h) to be distinguishable from both "silently suppress forever" and "re-detect immediately."

---

### F022 — `apply_diff` has no transactional semantics; partial application on error leaves `DesiredState` inconsistent [MEDIUM]

**Category:** Partial failure atomicity

**Attack:** `apply_diff` walks entries, mutates the working `canonical_json()` buffer at each pointer, then reparses. If entry N of M fails (e.g., entry 3 removes a subtree and entry 7 modifies a path under that subtree), entries 1 through N-1 are already applied. The error return leaves an intermediate state — neither the desired state nor the running state — if the caller captures the partially-built buffer.

**Scenario:**
1. Diff has 10 entries. Entry 3 removes `/apps/http/servers/srv0`.
2. Entry 7 modifies `/apps/http/servers/srv0/routes/0/handle`. Parent path no longer exists.
3. `apply_diff` returns `DiffError::IncompatibleShape`. The partially-mutated buffer has entries 1–6 applied.
4. If the caller silently ignores the error and uses the partial result, subsequent applies diverge.

**Design gap:** The design must specify that `apply_diff` clones the input state before mutation and discards the working copy on any error, returning the original unchanged. The function signature `(state: &DesiredState, diff: &Diff) -> Result<DesiredState, DiffError>` implies a new value is returned, but the word "mutate" in the algorithm description implies in-place mutation of a buffer. These must be reconciled.

---

### F023 — Ownership sentinel removal is classified as `ObjectKind::Other` drift, not `caddy.ownership-sentinel-conflict` [MEDIUM]

**Category:** Observability gaps

**Attack:** ADR-0015 specifies that if the sentinel is removed, the drift detection cycle SHALL surface it with a "re-establish ownership" recommendation and write `caddy.ownership-sentinel-conflict`. The drift loop uses generic `structural_diff` which classifies sentinel removal as `ObjectKind::Other` (no classifier matches `/storage/trilithon-owner`). The audit event written is `config.drift-detected`. The `caddy.ownership-sentinel-conflict` event is never emitted.

**Scenario:**
1. Sentinel removed out-of-band. `structural_diff` finds a `Removed` entry at `/storage/trilithon-owner`.
2. Classified as `Other`. Audit row: `config.drift-detected, Other: {removed: 1}`.
3. No `caddy.ownership-sentinel-conflict` event. No "re-establish ownership" prompt.
4. Operator sees generic drift. May choose `adopt_running_state` (triggering F018).

**Design gap:** The design needs a post-diff inspection step that checks whether the diff touches ownership-sentinel paths and routes to the `caddy.ownership-sentinel-conflict` audit kind rather than (or in addition to) `config.drift-detected`.

---

### F024 — Pre-redaction hash causes a new audit row per secret rotation, filling the drift log with noise [MEDIUM]

**Category:** Backpressure and resource exhaustion

**Attack:** `running_state_hash = SHA-256(canonical_json(running))` hashes the full pre-redaction running state. If a secret field (e.g., an upstream bearer token) rotates externally every hour while the structural config is otherwise unchanged, each rotation produces a new hash. The dedup check passes each time. A new `config.drift-detected` row is written every 60 seconds per secret rotation, generating 60 rows/hour for a single credential change. This is the symmetric complement of F005.

**Scenario:**
1. Upstream bearer token rotates hourly via external credential management.
2. Each rotation changes `running_state_hash`. Dedup always passes.
3. 60 new `config.drift-detected` rows written per hour, all with identical `redacted_diff_json` (`[REDACTED]`).
4. Operators become desensitised to drift alerts.

**Design gap:** The deduplication hash should be computed over post-redaction canonical JSON, not pre-redaction. This also resolves F005: post-redaction hashes are comparable across events and surface in the audit UI alongside the diff.

---

### F025 — Drift loop starts before ownership sentinel check completes; may emit audit rows for an unowned Caddy instance [LOW]

**Category:** Trust boundary violations

**Attack:** ADR-0015 mandates: "The daemon SHALL NOT make any mutation, apply, drift detection, or capability-related write" before ownership is confirmed. The Phase 8 design specifies no ordering between the sentinel check and drift loop activation. If the drift loop is spawned as a background task before the sentinel check `await` completes, the loop's first tick (which fires "once at startup") can race the sentinel check.

**Scenario:**
1. Daemon starts. Drift loop spawned immediately. First tick fires at t=0.
2. Sentinel check is `await`-ed at t=0 but hasn't returned yet.
3. Drift detected. `config.drift-detected` row written (immutable per ADR-0009).
4. Sentinel check completes: another installation's sentinel found. Daemon should have refused to operate.
5. Audit row written for an unowned instance cannot be retracted.

**Design gap:** The design must specify that the drift loop is not started until after the sentinel check returns successfully. Startup ordering belongs in `cli/src/main.rs`; Phase 8 must declare this dependency.

---

## Summary

**Critical:** 3 · **High:** 4 · **Medium:** 3 · **Low:** 1

**Top concern:** F016 and F017 together mean the drift resolution APIs return mutation variants that do not exist and carry no `expected_version`. These two findings are not independently fixable — they require a coordinated decision about how drift resolutions enter the mutation pipeline before any Slice 8.4 code is written.
