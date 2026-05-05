# Adversarial Review — Phase 05 — Round 8

**Design summary:** A content-addressed, append-only snapshot store backed by SQLite that uses optimistic concurrency control, SHA-256 content addressing, and instance-scoped versioning to safely record desired-state snapshots for a reverse-proxy configuration daemon.

**Prior rounds:** 7 prior rounds reviewed — all previously identified issues are marked as addressed in the design. No prior findings are re-raised below.

---

## Findings

### [HIGH] `Deduplicated` path inside `BEGIN IMMEDIATE` calls `tx.commit()` after INSERT was suppressed — silently consumes the `new_version` slot and creates a version gap

**Category:** Race Conditions

**Trigger:** Writer A completes step 7 (no row), enters `BEGIN IMMEDIATE`, allocates `new_version = N+1`, issues `INSERT OR IGNORE`. The insert is suppressed by the `id` UNIQUE index because Writer B concurrently inserted the same content. Step 14 finds no row for `(id, N+1)`, queries by `id` alone, finds the row, body matches — calls `tx.commit()` and returns `Deduplicated`. The transaction commits with no row written but with `new_version = N+1` allocated. Version `N+1` is permanently absent from the `snapshots` table; the next write for this instance will receive version `N+2`, creating a gap.

**Consequence:** The instance's `config_version` sequence has a permanent gap (`N+1` missing). The invariant established for the HashCollision branch (explicit `tx.rollback()` to prevent version gaps) is violated by the same-content `Deduplicated` branch, which routes to `tx.commit()` instead of `tx.rollback()`. The test `version_gap_not_created_after_hash_collision` covers the HashCollision branch but not this path.

**Design assumption violated:** The design assumes committing a transaction in which `INSERT OR IGNORE` was suppressed has no side effects. It has one: the version slot allocated at step 12 is consumed even though no row was written.

**Suggested mitigation:** In step 14, when the id-present branch determines same-content (`Deduplicated`), call `tx.rollback().await?` instead of `tx.commit().await?` before returning. The commit serves no purpose — nothing was written — and rollback releases the version slot. Add `tests::version_gap_not_created_after_concurrent_dedup` alongside the existing `version_gap_not_created_after_hash_collision`.

---

### [HIGH] `in_range` ordering by `(created_at_monotonic_nanos, config_version)` is non-deterministic across daemon restarts — monotonic nanos reset to ~0 on every restart

**Category:** Logic Flaws

**Trigger:** `in_range` orders by `created_at_monotonic_nanos ASC, config_version ASC`. `created_at_monotonic_nanos` is `elapsed_since_boot().as_nanos()` — it resets to 0 at every daemon restart. Two snapshots S1 (written in run A, 100s after boot) and S2 (written in run B immediately after restart) have monotonic nanos 100_000_000_000 and ~0 respectively. `in_range` orders S2 before S1, even though S1 was written first in wall-clock time. Adding `config_version` as a secondary key does not help: config_version is monotonically increasing within an instance and does not encode cross-run ordering.

**Consequence:** `in_range` consumers that span a restart boundary receive results in wrong order. For config-history replay or audit log reconstruction, this is a correctness failure — earlier snapshots appear after later ones across a daemon restart, with no signal to the consumer that ordering is unreliable.

**Design assumption violated:** The design uses `created_at_monotonic_nanos` as the primary sort key for `in_range`, implicitly assuming it provides a stable ordering signal across the dataset lifetime. It does not — it resets on every restart.

**Suggested mitigation:** Add `daemon_run_id` as an intermediate sort key: `ORDER BY created_at_monotonic_nanos ASC, daemon_run_id ASC, config_version ASC`. This makes cross-restart ordering deterministic (run IDs are ULIDs, lexicographically sortable by creation time) and stable. Document in the API that ordering is ULID-based between restart boundaries and monotonic-nanos-based within a restart boundary. Add a test verifying cross-restart ordering using `override_run_id_for_current_thread`.

---

### [MEDIUM] `with_limits` timeout warning formula omits SQLite `busy_timeout` — callers who follow the formula still see spurious `Timeout` under concurrent write load

**Category:** Timeouts & Retries

**Trigger:** `with_limits` warns when `write_timeout < 1ms_per_KiB(max_desired_state_bytes) + 1s`. Under concurrent writes, `BEGIN IMMEDIATE` acquisition blocks until the current writer commits — bounded by SQLite's `busy_timeout` (configured separately on the pool). If `busy_timeout = 5000ms` and the formula produces `write_timeout = 2s`, callers following the formula will hit `Timeout` under any contention because lock acquisition alone may take 5s. The warning formula does not mention `busy_timeout`.

**Consequence:** Callers use the warning threshold as a sizing guide, configure accordingly, and still see `WriteError::Timeout` under load. The timeout appears to be an infrastructure problem rather than a configuration issue.

**Design assumption violated:** The design assumes 1 second of base headroom covers all non-serialisation costs including lock acquisition. It does not account for `busy_timeout`.

**Suggested mitigation:** Document the relationship between `write_timeout`, the pool's `PRAGMA busy_timeout`, and the expected lock-wait duration. Update the warning formula: `write_timeout < 1ms_per_KiB(max) + busy_timeout_ms + 500ms_transaction_overhead`. Require that `busy_timeout < write_timeout` — if the configured pool `busy_timeout` exceeds `write_timeout`, lock acquisition will always outlast the Tokio timeout.

---

### [MEDIUM] `regen-snapshot-hashes` partial-failure behaviour is unspecified — partial success can leave the DB in a mixed state

**Category:** Rollbacks

**Trigger:** `regen-snapshot-hashes` processes 50 rows successfully, then fails on row 51 (e.g., `CanonicalError`). The design specifies exit-code behaviour only for the case where `N_processed == 0 AND M_skipped > 0`. It does not specify whether the 50 already-processed rows are rolled back, whether the command exits non-zero, or whether it can be re-run safely after partial failure.

**Consequence:** If rows 1–50 were updated (e.g., `canonical_json_version` bumped) and row 51 failed, the database is in a mixed state. Re-running the command would process only the un-updated rows, but the set of rows to process depends on the filter `WHERE canonical_json_version = CANONICAL_JSON_VERSION` — if the batch updated `canonical_json_version` for rows 1–50, re-running correctly targets only row 51. However, if the batch updated `id` values (for a hash-regeneration use case), parent-pointer links from rows that weren't yet updated would point to the new id, breaking the parent chain.

**Design assumption violated:** The design assumes re-hashing is atomic (all-or-nothing). It does not specify the transaction boundary for the batch.

**Suggested mitigation:** Specify that `regen-snapshot-hashes` wraps all row updates in a single SQLite transaction. On any error, roll back entirely and exit non-zero. This makes the tool idempotent: re-running after failure starts from a clean state. Document this as a slice 5.7 exit condition.

---

### [MEDIUM] `by_id_global` has no audit trail — internal uses are indistinguishable from accidental cross-instance reads

**Category:** Authentication & Authorization

**Trigger:** `by_id_global` is `pub(crate)` within `crates/adapters`. Future maintainers adding a new admin command to `crates/adapters` can call it without noticing the cross-instance scoping risk. The only guard is the function name. There is no lint test that asserts the function is called from exactly one location, no doc comment naming the single permitted caller, and no type-level enforcement.

**Consequence:** A second call site added during future maintenance work silently becomes a cross-tenant read path. Because the function name (`by_id_global`) suggests "by id, globally" without indicating the security risk, accidental use is plausible.

**Design assumption violated:** The design assumes maintainers will notice the `pub(crate)` boundary and the function name as sufficient guards. In a growing codebase, convention-based guards erode.

**Suggested mitigation:** Add a `grep`-based integration test (or a `#[cfg(test)]` module test) that fails if `by_id_global` appears more than N times in the adapters source tree. Alternatively, add a `#[doc(hidden)]` attribute and a comment in the function doc: "PERMITTED CALLERS: only `crates/adapters/src/integrity/`. Add no new callers without an architecture review."

---

### [LOW] `elapsed_since_boot` saturation silently produces identical ordering values for all writes after ~584 years

**Category:** Logic Flaws

**Trigger:** `elapsed_since_boot().as_nanos()` is `u128`; it is clamped to `u64::MAX` (stored as `INTEGER NOT NULL` in SQLite, which is a signed i64 — max ~9.2e18 ns ≈ 292 years) with `tracing::warn!`. After saturation, every subsequent snapshot receives `created_at_monotonic_nanos = u64::MAX`, making the `in_range` ordering by monotonic nanos non-deterministic for all rows written after saturation.

**Consequence:** All writes after ~292 years of continuous operation sort identically on the primary key of `in_range`, falling through to `config_version` as tiebreaker (which still works). In practice this cannot occur because the daemon is restarted periodically — but the silent-clamp path is an undocumented ordering degradation with no consumer-visible signal.

**Suggested mitigation:** Document that `created_at_monotonic_nanos` ordering is only guaranteed within a single daemon run (resets on restart regardless); saturation is a theoretical concern. Low severity — no action required beyond this documentation note, which is already partially captured in the `elapsed_since_boot()` accumulation note.

---

## Summary

**Critical:** 0 &nbsp; **High:** 2 &nbsp; **Medium:** 3 &nbsp; **Low:** 1

**Top concern:** The `Deduplicated` branch in step 14 calls `tx.commit()` after `INSERT OR IGNORE` was suppressed by the `id` UNIQUE constraint — identical in structure to the HashCollision bug fixed in R7, but producing `Deduplicated` instead of `HashCollision`. The fix is symmetric: replace `tx.commit()` with `tx.rollback()` in that branch, matching the invariant already established for the HashCollision case.
