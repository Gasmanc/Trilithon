# Adversarial Review — Phase 7 — Round 2

**Prior rounds:** Round 1 (10 findings, all unaddressed).

---

## Summary

Round 2 probed surfaces Round 1 left untouched — `AuditWriter` failure handling, the `get_running_config` failure path after a successful `POST /load`, the `mutation.conflicted` vocabulary mismatch, the undefined `DiffEngine` ignore-list, and the interaction effects when multiple Round 1 issues compound simultaneously. The most dangerous new finding (F017) shows that F001 + F004 + F007 can fire simultaneously, producing two version-6 snapshots and two `config.applied` audit rows, making the snapshot chain permanently ambiguous.

---

## Findings

### F011 — AuditWriter failure on the success leg is unspecified; no terminal row written but apply returns `Succeeded`
**Severity:** HIGH
**Category:** assumption-violation
**Slice:** 7.4 / 7.7

**Attack:** Caddy returns 200 and `advance_config_version_if_eq` commits. Then `audit.record(config.applied)` fails — SQLite out of disk, WAL checkpoint lock held, `busy_timeout` exceeded. The design's step 6 specifies "write one `config.applied` audit row; emit `apply.succeeded`; return `ApplyOutcome::Succeeded`" as a single linear sequence without branching on audit failure. No documented error path exists.

**Why the design doesn't handle it:** All three possible responses to an audit write failure are wrong: failing the apply retroactively is wrong because Caddy already accepted the load; succeeding silently breaks the single-terminal-row invariant; treating it as a no-op makes the invariant permanently unfulfillable for that `correlation_id`. The design chooses none of them.

**Blast radius:** The phase invariant "every `correlation_id` from an apply MUST have exactly one terminal audit row" is violated silently. The version pointer advanced, Caddy is serving the new config, but the audit log has a permanent hole. Forensic and compliance tooling see no record of the apply.

**Recommended mitigation:** Specify explicitly: audit write failure on the success leg returns `ApplyOutcome::Succeeded` (because Caddy's state is committed and cannot be rolled back), emits a `tracing::error!` event, and increments a `trilithon_audit_write_failures_total` metric. The missing row is flagged to out-of-band alerting. Document this as a deliberate lossy-audit tradeoff.

---

### F012 — `get_running_config` failure after successful `POST /load` surfaces `ApplyError::Unreachable`; Caddy is serving the new config but pointer is not advanced and audit row records failure
**Severity:** HIGH
**Category:** assumption-violation
**Slice:** 7.4 step 5

**Attack:** `POST /load` returns 200. Step 5 issues `client.get_running_config().await?`. Caddy's socket is briefly overloaded; the call returns `CaddyError::Unreachable`. The `?` propagates this as `ApplyError::Unreachable`. The version pointer is not advanced (that happens after step 5). Caddy is serving the new config; Trilithon believes the old version is current. A `config.apply-failed` terminal row is written for a successful apply.

**Why the design doesn't handle it:** Steps 4 and 5 are treated as one unit in the "on success" branch. The design does not distinguish between step-4 failure (Caddy rejected the load) and step-5 failure (Caddy accepted the load but we could not verify it).

**Blast radius:** System enters permanently inconsistent state: old version pointer, new Caddy state. The next drift-detection cycle fires a false `config.drift-detected` alarm. The audit log records the apply as failed when it actually succeeded.

**Recommended mitigation:** Treat step-5 failure after step-4 success as a distinct error branch: record `config.applied` (because the load succeeded), emit a `tracing::warn!` about the equivalence-check failure, do not advance the version pointer, and schedule an immediate re-apply or drift check. Never let a step-5 network error propagate as `ApplyError::Unreachable` when step 4 already returned 200.

---

### F013 — `mutation.conflicted` audit kind in Slice 7.5 is semantically wrong for an apply-time concurrency abort
**Severity:** MEDIUM
**Category:** assumption-violation
**Slice:** 7.5

**Attack:** Slice 7.5 specifies: "abort, write `mutation.conflicted` audit row". The `mutation.conflicted` kind was designed for mutation-pipeline conflicts (a change submitted against a stale snapshot). An apply-time version race between two concurrent appliers is a different event. An operator querying `kind = 'mutation.conflicted'` to diagnose edit-conflict failures will see spurious rows from apply-time races and vice versa.

**Why the design doesn't handle it:** The design reuses the mutation-layer vocabulary for an adapter-layer concurrency event. No `config.apply-conflicted` kind exists in the closed vocabulary.

**Blast radius:** Forensic queries on `mutation.conflicted` return noise from apply-time races. Queries on `config.apply-*` miss the conflict record entirely. The single-terminal-row invariant is met in letter but the row carries the wrong kind.

**Recommended mitigation:** Add `"config.apply-conflicted"` to the `AUDIT_KINDS` vocabulary and `AuditEvent` enum in the same commit as Slice 7.5. Use it exclusively for optimistic-concurrency aborts at apply time. Update the property test to assert the correct kind.

---

### F014 — `DiffEngine` ignore-list is undefined; an over-broad list makes the post-load equivalence check a no-op security bypass
**Severity:** HIGH
**Category:** abuse-case
**Slice:** 7.4 step 5

**Attack:** Step 5 runs `diff_engine.structural_diff(rendered_config, running_config)` against "the architecture ignore list". The design does not define what is on this list. If the list includes `/apps/http/servers/*` to suppress Caddy's `@id`-annotation normalisation (the F002 workaround), then an adversary with loopback socket access who intercepts `GET /config/` can substitute any config they like in the response. The equivalence check passes; Trilithon emits `config.applied` and records the attacker's payload as the authorised state.

**Why the design doesn't handle it:** The `DiffEngine` ignore list is described only as "the architecture ignore list" without enumerating its contents, bounding its scope, or specifying its security properties.

**Blast radius:** With an over-broad ignore list, the post-load equivalence check provides no security value. An adversary can inject arbitrary routes, TLS bypass rules, or reverse-proxy targets, and Trilithon will record the injection as a successful authorised apply.

**Recommended mitigation:** Enumerate the ignore-list entries exhaustively in the Slice 7.4 spec. Any path whose exclusion would allow an undetected semantic change to routes, upstreams, or TLS config must NOT appear on the list. The ignore list must be a compile-time constant, not a runtime-configurable value.

---

### F015 — F001 reclaim mitigation (`BEGIN IMMEDIATE` for DELETE+INSERT) creates a livelock when the reclaimer and a new acquirer race
**Severity:** HIGH
**Category:** composition-failure
**Slice:** 7.6

**Attack:** The Round 1 mitigation wraps the lock-reclaim `DELETE`+`INSERT` in a single `BEGIN IMMEDIATE`. Correct — but if the reclaimer's `BEGIN IMMEDIATE` COMMITs the `DELETE` of the stale row before the new `INSERT` (because they were separate statements in separate transactions), a concurrent new acquirer's `BEGIN IMMEDIATE` sees no lock row and inserts its own row. The reclaimer then attempts its `INSERT` and gets a uniqueness violation against the new acquirer's row. The reclaimer cannot acquire its own reclaim slot. Both the reclaimer and the new acquirer may now believe they do not hold the lock.

**Why the design doesn't handle it:** The mitigation note describes wrapping the reclaim path without specifying that the `DELETE` and `INSERT` are a single atomic pair within one `BEGIN IMMEDIATE`, with no COMMIT between them.

**Blast radius:** A lock-reclaim event races a legitimate new acquirer and both end up failing. The apply loop permanently cannot acquire the lock after any crash-reclaim event, requiring a daemon restart.

**Recommended mitigation:** The reclaim transaction must be a single `BEGIN IMMEDIATE` containing both the `DELETE` of the old row and the `INSERT` of the new row. The `DELETE rowcount == 1` assertion acts as the compare-and-swap. No COMMIT between the two statements. Alternatively, use OS-level file locking (`fs2::FileExt`) that the OS automatically releases on crash, replacing the SQLite advisory lock entirely.

---

### F016 — F004 two-phase mitigation has a TOCTOU: version check outside the `BEGIN IMMEDIATE` does not protect against a concurrent writer advancing the version between the check and the write
**Severity:** HIGH
**Category:** composition-failure
**Slice:** 7.5

**Attack:** The Round 1 two-phase mitigation reads: (1) read-only version check; (2) `BEGIN IMMEDIATE` only for the version-advance write. Two callers both read `config_version = 5`, both pass the pre-check, and both reach the write transaction. Caller A's `BEGIN IMMEDIATE` succeeds first, advances to version 6, COMMITs. Caller B's `BEGIN IMMEDIATE` is now unblocked — but only if the `SELECT` inside B's transaction is re-executed inside the `BEGIN IMMEDIATE`. If the implementation uses the pre-check value (5) and skips a second SELECT inside the transaction, B also believes version is 5 and also advances to 6.

**Why the design doesn't handle it:** The design says `advance_config_version_if_eq` contains both a SELECT and a write, but the Round 1 mitigation's two-phase split suggests the SELECT can happen before the transaction. If the implementation splits them, the version check inside `BEGIN IMMEDIATE` is omitted and the CAS is broken.

**Blast radius:** Two parallel applies both advance to version 6, both issue `POST /load`, and both write `config.applied`. Two terminal rows for different `correlation_id`s claim the same `config_version`. The `snapshots` table unique index on `(caddy_instance_id, config_version)` fires a constraint violation on the second INSERT — which is the correct guard — but this error is raised after `POST /load` already returned 200 for both callers, leaving Caddy and the DB in an inconsistent state.

**Recommended mitigation:** The version-advance check and increment must be a single atomic `BEGIN IMMEDIATE` → read-current-version → compare-to-expected → write-new-version → COMMIT sequence with no outer pre-check that splits the read from the write. The `advance_config_version_if_eq` function must enforce this internally; calling the read outside the transaction must be documented as a caller bug and detected (e.g., by asserting that the function is always called without an active transaction on the connection).

---

### F017 — F001 + F004 + F007 compound: two processes simultaneously advance the version pointer to the same value
**Severity:** CRITICAL
**Category:** cascade-construction
**Slice:** 7.5 / 7.6 / 7.7

**Attack:** Two Trilithon processes P1 and P2 point at the same SQLite file. F001: P2 reclaims P1's stale advisory lock row and both believe they hold the lock. F007: the snapshot insert and version advance are separate transactions. F004: the IMMEDIATE transaction spans the Caddy HTTP call, so P1's long-running transaction may not commit before P2 begins its version check. Sequence: (a) P1 writes snapshot (insert commits, version 5 → 6 in one transaction). (b) P2 reads version — still 5 because P1's snapshot insert hasn't been seen yet. (c) P2 passes its version check. (d) P1's version-advance transaction (separate from the insert) opens `BEGIN IMMEDIATE` and advances to 6. (e) P2's version-advance transaction opens `BEGIN IMMEDIATE` — P1 has committed, version is now 6 ≠ P2's expected 5 — P2 should see a conflict. But if F007 means P1's snapshot insert and version advance are BOTH separate transactions, step (b) sees version 5, step (d) sees version 5 too (the version advance in the schema lives in the snapshot table's `config_version` column, not a separate table). Both P1 and P2 insert snapshots with `config_version = 6`; the `UNIQUE INDEX snapshots_config_version ON snapshots(caddy_instance_id, config_version)` fires a UNIQUE CONSTRAINT VIOLATED error on the second INSERT — but only after both `POST /load` calls have already reached Caddy.

**Why the design doesn't handle it:** F001, F004, and F007 are individually addressable but form a compound window that is wider than any single fix. The in-process mutex protects only intra-process concurrency. The advisory lock has no crash-safe semantics (F001). The version check is not atomic with the snapshot insert (F007).

**Blast radius:** Two `POST /load` calls reach Caddy; the second overwrites the first. Two `config.applied` audit rows claim version 6. The `snapshots` unique constraint fires on the second INSERT, producing a `StorageError` — which the applier may surface as `ApplyOutcome::Failed` even though Caddy is serving the second config. The snapshot chain is permanently ambiguous: every `latest_desired_state()` call is now a coin flip between two version-6 rows (until the constraint fires and one is rolled back, but the audit row already exists).

**Recommended mitigation:** Address F001, F004, and F007 in a coordinated fix: (1) replace the SQLite advisory lock with an OS-level `fs2` file lock (crash-safe by construction); (2) require the snapshot insert and version-advance to be a single `BEGIN IMMEDIATE` transaction (version 5→6 and the new snapshot row are atomic); (3) issue `POST /load` only after that transaction commits. With these three changes, the compound attack window collapses to zero.

---

### F018 — `unknown_extensions` has no size bound; crafted entries can exhaust heap during render and leave the advisory lock held on OOM kill
**Severity:** MEDIUM
**Category:** abuse-case
**Slice:** 7.1

**Attack:** `DesiredState.unknown_extensions: BTreeMap<JsonPointer, Value>` has no documented maximum size. A caller constructs a `DesiredState` with 10 000 entries, each a 1 KiB nested `Value::Object`. The renderer allocates ~10 MiB on the heap during `render()`. An OOM kill during render drops the `MutexGuard` (never dropped) and the `AcquiredLock` (best-effort DROP, which sends a blocking SQLite DELETE — also fails under OOM). On restart, the advisory lock row persists until the 5-minute TTL; all applies are blocked.

**Why the design doesn't handle it:** No maximum is specified for `unknown_extensions` entry count, per-entry size, or total rendered document size.

**Blast radius:** Daemon OOM-killed during render; advisory lock row stuck for 5 minutes after restart; all applies blocked during that window.

**Recommended mitigation:** Add to `CaddyJsonRenderer`: validate that `unknown_extensions.len() <= 256` and total rendered document size ≤ 1 MiB. Return `RenderError::DocumentTooLarge` if either bound is exceeded. Document the bounds in the phase spec.

---

### F019 — `TlsIssuanceObserver` reads the "previous snapshot" from live storage at spawn time, not from the lineage — a concurrent apply invalidates the baseline
**Severity:** MEDIUM
**Category:** assumption-violation
**Slice:** 7.8

**Attack:** The algorithm reads "previous snapshot's hostnames" from `Storage::latest_desired_state()` at observer-spawn time. If another apply completes between `POST /load` returning 200 and the hostname-comparison call, `latest_desired_state()` returns the *newer* snapshot. The hostname diff is computed against the wrong baseline. Hostnames already present may appear as "new"; hostnames removed by the concurrent apply may appear as "added by this apply", causing the observer to poll for a cert that will never appear and emit a false `config.apply-failed` after 120 s.

**Why the design doesn't handle it:** The design reads the previous snapshot from live storage at a different point in time than when the apply started. The snapshot already in scope at `apply()` entry has a `parent_id` that is the correct baseline; the design ignores it.

**Blast radius:** Spurious `TlsIssuanceObserver` spawns; false `config.apply-failed` rows for hostnames never at risk; phantom failure entries confuse Phase 8 drift detection.

**Recommended mitigation:** Pass the parent snapshot directly into the observer-spawn logic (loaded from storage once at `apply()` entry) rather than re-fetching it. The `parent_id` on the new snapshot is the correct, race-free baseline.

---

## Severity summary

| Severity | Count |
|----------|-------|
| CRITICAL | 1 (F017) |
| HIGH     | 4 (F011, F012, F014, F015, F016) |
| MEDIUM   | 3 (F013, F018, F019) |
| LOW      | 0 |

**Top concern:** F017 — coordinated compound of F001 + F004 + F007 allows two processes to simultaneously advance the version pointer to version 6, issue two `POST /load` calls, and produce two `config.applied` audit rows, leaving the snapshot chain permanently ambiguous.

**Must address before implementation:** F017 (requires coordinated fix of F001 + F004 + F007), F012 (step-5 failure after step-4 success), F014 (undefined DiffEngine ignore-list), F015 (reclaim livelock).
