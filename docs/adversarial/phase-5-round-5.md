# Adversarial Review — Phase 05 — Round 5

**Design summary:** Phase 05 implements a snapshot writer with content-addressed storage using SHA-256, canonical JSON serialisation, SQLite-backed persistence via a three-layer Rust workspace, and OCC-based version conflict detection. The writer uses `INSERT OR IGNORE` plus `changes()` to distinguish deduplication from version races, and a monotonic daemon clock for cross-restart ordering.

**Prior rounds:** 4 rounds reviewed — all findings listed as addressed. No prior findings re-raised below.

---

## Findings

### HIGH — `changes()` is connection-scoped, not query-scoped; a trigger firing on INSERT silently zeroes the count

**Category:** Logic Flaws

**Trigger:** SQLite's `changes()` function returns the number of rows changed by the most recent statement that directly modified rows. If any trigger fires during the `INSERT OR IGNORE` and itself modifies a row (e.g., a hypothetical audit-log trigger, or even the immutability `BEFORE UPDATE` trigger on another table firing due to a cascading constraint), `changes()` on the same connection will reflect the trigger's last modification, not the INSERT. More concretely and without needing a new trigger: SQLite documents that `changes()` counts rows changed by the outermost SQL, but `total_changes()` counts all changes including those from triggers. If the design ever adds a trigger that fires an UPDATE or INSERT on any table after the snapshot INSERT succeeds, `changes()` returns 0 (from the trigger's last write) even though the row was inserted.

**Consequence:** A successful insert is misclassified as a collision. The code then executes the diagnostic SELECT, finds the row (just inserted), does a length comparison, and returns `Deduplicated` instead of `Inserted`. The caller believes the write was a no-op when it was actually a new record. All downstream processing that keys on `WriteOutcome::Inserted` (indexing, alerting, configuration pushes) is silently skipped.

**Design assumption violated:** The design assumes `changes()` always returns 1 after a successful INSERT OR IGNORE on the snapshots table, but this is only true if no row-modifying trigger fires after the statement. The current migration 0004 triggers only abort — they raise errors and never modify rows — but the design provides no architectural guard against future triggers breaking this invariant.

**Suggested mitigation:** Replace `SELECT changes()` with `SELECT id FROM snapshots WHERE id = ? AND config_version = ?` (using the just-computed id and new_version). This directly confirms the row exists with the expected version rather than relying on `changes()` semantics. Alternatively, document as an invariant in the migration guide that no future trigger may modify rows after a snapshot INSERT, and add a test that fails if `changes()` after INSERT OR IGNORE does not equal 1 in a clean DB.

---

### HIGH — Pre-transaction `expected_version = None` OCC predicate is unspecified; implementation may permit two root nodes for the same instance under a concurrent race

**Category:** Race Conditions

**Trigger:** Two callers independently perform root creation for the same `caddy_instance_id` with `expected_version = None`. Both enter step 7, both find no row for their respective ids (different DesiredState content → different SHA-256 hashes). Both enter `BEGIN IMMEDIATE` sequentially. Caller A acquires the lock, reads step 10 (no rows, `current_max = -1`), OCC check: `expected = None`, `current = -1`. If the implementation checks `expected_version.map_or(true, |v| v == current_max)`, this passes (None maps to true). A inserts at version 0. Commits. B acquires the lock, reads step 10 (`current_max = 0`, `current_id = Some(A.id)`). OCC check: `expected = None`, maps to true again — B also passes. B inserts at version 1 with `expected_version = None` accepted. Two root nodes exist for the same instance.

**Consequence:** Two root snapshots exist for one `caddy_instance_id`. `by_config_version(0)` returns one root; `by_config_version(1)` returns another with `parent_id = None`. Phase 7 chain traversal finds two parentless roots and cannot reconstruct a linear history. Data integrity is broken with no error ever surfaced.

**Design assumption violated:** The design specifies "None only for root creation" as a semantic constraint on the caller but does not specify the exact OCC SQL predicate for the `None` case. The implementation must infer that `expected_version = None` means "no rows must exist for this instance", but the algorithm text says only "if `expected_version.is_none()` AND `current_max != -1`: return VersionConflict" — which correctly rejects B if the step-10 SELECT returns `current_max = 0`. However, this relies on `-1` being the sentinel for "no rows exist", which is not stated as a design invariant and could be broken if the sentinel is misimplemented as `0`.

**Suggested mitigation:** The design MUST explicitly specify the OCC predicate for `expected_version = None` as "the instance MUST have no existing snapshots" — i.e., the step-10 SELECT must return no rows (not merely `current_max == -1`). The sentinel `-1` MUST be documented as a local variable default, never stored in the database. Add a test: `tests::concurrent_root_creation_produces_one_root` with `flavor = "multi_thread"` + Barrier between steps 7 and 8 for two writers with distinct content and `expected_version = None`, asserting exactly one row exists after both complete.

---

### MEDIUM — `write_timeout` wraps the entire body including the pre-transaction id check; a slow step-7 full-body fetch consumes the budget before `BEGIN IMMEDIATE` is attempted

**Category:** Timeouts & Retries

**Trigger:** The entire write algorithm body is wrapped in `tokio::time::timeout(self.write_timeout, ...)`. Step 7 may execute two SELECTs (length check, then full-body fetch for up to 10 MiB) outside the transaction. Under high read contention or temporarily slow disk, step 7 takes 4.8 seconds of a 5-second `write_timeout`. The `BEGIN IMMEDIATE` acquisition then has 200ms remaining. If another writer holds the lock, step 8 blocks until the Tokio timeout fires. The caller receives `Timeout(5s)`.

**Consequence:** The caller cannot distinguish "timed out before acquiring the write lock" (safe to retry) from "timed out during the write lock" (write did not land — also safe to retry, but the caller does not know this). More critically, the step-7 full-body fetch for a near-collision (same length, different content) may itself load 10 MiB and take several seconds — consuming the entire budget before the actual write is attempted. Callers will see `Timeout` errors that are caused by dedup-path IO, not by write contention.

**Design assumption violated:** The design assumes the pre-transaction step-7 check is cheap (common path: no row, proceed). The length-match/full-body path is expensive and shares the same timeout budget as the transaction.

**Suggested mitigation:** Split the timeout into two phases: a short pre-check timeout (e.g., 1s) covering steps 1–7, and the full `write_timeout` covering steps 8–16. Alternatively, document that `write_timeout` covers the entire operation including dedup checks, so callers can set it appropriately when large configs are expected.

---

### MEDIUM — `daemon_clock::init()` idempotency prevents testing cross-restart ordering of the `(created_at_ms, daemon_run_id, created_at_monotonic_nanos)` sort key

**Category:** Logic Flaws

**Trigger:** `OnceLock<String>` is process-global. `daemon_clock::init()` panics on a different `run_id`. A test that wants to verify cross-restart ordering — "records with run_id A sort before records with run_id B" — cannot call `init()` twice with different values in the same test binary. The compound sort key is designed for cross-restart ordering but cannot be integration-tested within a single binary.

**Consequence:** The cross-restart ordering invariant (the entire reason `daemon_run_id` was added to the schema) has no integration test coverage. A future implementation that stores the ULID incorrectly, sorts wrong, or uses `created_at_monotonic_nanos` alone across restarts will pass all tests.

**Design assumption violated:** The design assumes the "idempotent for same value, panic for different value" semantics are sufficient for all test scenarios. This is true for correctness tests but makes the cross-restart ordering invariant untestable.

**Suggested mitigation:** Add a `#[cfg(test)]` seam — either a thread-local override for `run_id()` or an explicit `run_id` parameter on `SnapshotWriter::write` in test builds — so that tests can write rows with distinct simulated run_ids and assert the compound sort key produces the correct ordering.

---

### MEDIUM — `regen-snapshot-hashes` does not filter by `canonical_json_version`; it will produce false-positive mismatches for rows written by prior canonical versions

**Category:** Logic Flaws

**Trigger:** A future canonical JSON version bump (version 2) changes the serialisation algorithm. The `regen-snapshot-hashes` binary is built with version 2. It reads all rows from the DB, including rows with `canonical_json_version = 1`. It re-canonicalises each row's `desired_state_json` using the current (version-2) algorithm and compares the result against the stored `id` (a SHA-256 of the version-1 bytes). For version-1 rows, the recomputed hash differs. The binary reports false-positive corruption for all legacy rows.

**Consequence:** An operator runs `regen-snapshot-hashes` after a version bump and concludes the database is corrupt. Worse: if the tool has a repair mode that updates `id` values, it severs parent pointer links stored by other rows (`parent_id` references the old hash).

**Design assumption violated:** The design adds a `canonical_json_version` column per row specifically to enable version-aware processing, but does not specify that `regen-snapshot-hashes` must filter or dispatch by this column. The tool's per-row version awareness is unspecified.

**Suggested mitigation:** Specify in the slice 5.7 design that `regen-snapshot-hashes` MUST filter to `WHERE canonical_json_version = CANONICAL_JSON_VERSION` (only verify rows written by the current version) and MUST report rows with other `canonical_json_version` values as "skipped (legacy version)" rather than mismatches.

---

### LOW — `SnapshotFetcher::in_range(from, to, page)` does not specify what `from` and `to` range over; three plausible column choices produce different semantics

**Category:** Logic Flaws

**Trigger:** The design names `in_range(from: UnixSeconds, to: UnixSeconds, page: Page)` but does not specify whether the range applies to `created_at` (Unix seconds), `created_at_ms`, or `config_version`. The algorithm says `WHERE created_at >= ? AND created_at <= ?` using `caddy_instance_id = self.instance_id`, which is instance-scoped. However, `created_at` has second-level granularity — two snapshots written in the same second produce identical `created_at` values, so the range endpoint may silently exclude or include the wrong row depending on whether the boundary is inclusive.

**Consequence:** Phase 9 callers building a "show snapshots from time A to time B" UI may miss snapshots at the boundary second or include snapshots just outside the window due to second-level granularity. For a high-throughput instance writing multiple snapshots per second, `in_range` over `created_at` may return incomplete results for any range that is not aligned to second boundaries.

**Design assumption violated:** The design assumes `created_at` (Unix seconds) is fine-grained enough for range queries, but several Phase 5 snapshots may share the same `created_at` second value.

**Suggested mitigation:** Change `in_range` to range over `created_at_ms` (milliseconds) for finer granularity, or add a secondary ordering/tiebreak on `config_version` and document that the range is inclusive on both ends. Alternatively, expose `in_range_ms(from_ms: i64, to_ms: i64, page: Page)` so callers can choose the granularity.

---

## Summary

**Critical:** 0 &nbsp; **High:** 2 &nbsp; **Medium:** 3 &nbsp; **Low:** 1

**Top concern:** The `changes()` trigger-fragility finding (HIGH-1) is the most dangerous — it is a silent misclassification of `Inserted` as `Deduplicated` that requires only one future row-modifying trigger to activate, produces no error, and is invisible in logs. The root-creation OCC predicate ambiguity (HIGH-2) is a close second: under a concurrent provisioning race, two callers writing different root states for the same instance both pass the OCC check if the implementation uses `None → true` shorthand rather than "no rows must exist" predicate, producing two parentless root nodes with no error surfaced.
