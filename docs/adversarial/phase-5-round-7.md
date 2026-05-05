# Adversarial Review — Phase 05 — Round 7

**Design summary:** A content-addressed, append-only snapshot store backed by SQLite that uses OCC (optimistic concurrency control) with a per-instance monotonic config_version, SHA-256 content addressing for deduplication, and daemon_clock for monotonic timing across restarts.

**Prior rounds:** 6 prior rounds reviewed — all listed findings are addressed in the current design. No re-raises below.

---

## Findings

### [HIGH] `HashCollision` returned from inside the transaction permanently burns the allocated `config_version` number

**Category:** Race Conditions

**Trigger:** Writer A passes step 7 (no row found), enters `BEGIN IMMEDIATE`, allocates `new_version = N+1` at step 12. Between step 7 and step 13, an unrelated writer inserts a row with a SHA-256 id that collides with Writer A's computed id (a HashCollision scenario). Writer A's `INSERT OR IGNORE` at step 13 is silently suppressed. Step 14 finds no row for `(id, N+1)`, queries by id alone, finds the collision row, compares bodies — mismatch → returns `HashCollision`. **But `new_version = N+1` was already allocated inside the open transaction.** The design does not specify that the transaction is explicitly rolled back before returning `HashCollision` from step 14. The transaction is rolled back by the `?` propagation path only if the rollback is explicit — otherwise it is committed or left dangling, and the connection returns to the pool without committing a row, leaving `config_version = N+1` permanently consumed (the version slot exists nowhere in the DB but is forever past the returned `current_max`).

**Consequence:** The instance's config_version sequence develops a permanent gap (e.g. 0, 1, 2, 4 — version 3 was allocated but no row was inserted). Any downstream code that iterates by config_version and expects a gapless sequence (replay, integrity verification, `regen-snapshot-hashes`) will misinterpret the gap as data loss or corruption.

**Design assumption violated:** The design assumes `new_version` is only allocated if the INSERT succeeds or the id is a true dedup. It does not explicitly mandate that the transaction must be rolled back before returning `HashCollision` from inside step 14.

**Suggested mitigation:** Step 14 must explicitly call `tx.rollback().await?` (or equivalent) before returning any `HashCollision` error. Already-stated rollback in the step-14 dispatch text covers the `id`-not-found path but the HashCollision path's rollback must be explicit. Add a note: "In all error-return paths from inside the transaction, `tx.rollback().await` MUST be called before returning. Relying on Drop to roll back is not sufficient — sqlx may panic or log a warning if an uncommitted transaction is dropped on a closed connection." Add `tests::version_gap_not_created_after_hash_collision` — after simulating a hash collision, SELECT all config_versions for the instance and assert they are gapless.

---

### [HIGH] `with_limits` returns `Self` without `#[must_use]` — discarding the return value silently reverts to defaults

**Category:** Logic Flaws

**Trigger:** `with_limits` follows the builder pattern and returns `Self` by value. A caller who writes:
```rust
let writer = SnapshotWriter::new(pool, id).await?;
writer.with_limits(1_024, Duration::from_millis(500));  // return value discarded
writer.write(inputs).await  // uses DEFAULT_MAX_DESIRED_STATE_BYTES (10 MiB) and DEFAULT_WRITE_TIMEOUT (5s)
```
The Rust compiler emits no warning because `Self` is not `#[must_use]`. The caller's intent to enforce a 1 KiB cap and 500 ms timeout is silently lost.

**Consequence:** A caller intending to enforce a small `max_desired_state_bytes` inadvertently accepts 10 MiB payloads. A caller intending a short timeout blocks for 5 s. No compile error, no runtime error — silent misconfiguration.

**Design assumption violated:** The design assumes the builder pattern is safe because `with_limits` returns `Self`. Rust's move semantics enforce consumption only if `#[must_use]` is present.

**Suggested mitigation:** Annotate `with_limits` with `#[must_use = "with_limits returns a new SnapshotWriter; the original is unchanged"]`. Optionally annotate the struct itself with `#[must_use]`.

---

### [MEDIUM] `in_range` orders by `created_at_ms ASC` — wall-clock is not monotonic; NTP backward steps produce incorrect result ordering

**Category:** Logic Flaws

**Trigger:** `in_range` orders results by `created_at_ms ASC, id ASC`. On a host that experiences an NTP correction (or admin `date` command), two snapshots written in real-time order can receive `created_at_ms` values where the second is less than the first. Snapshot A written at wall time 1000, snapshot B written immediately after at wall time 998 — `in_range` returns them ordered B, A (reverse of insertion order).

**Consequence:** Any consumer of `in_range` that assumes results are in write order (replaying config change logs, auditing desired-state transitions) will reconstruct events in the wrong order. For a snapshot store whose purpose is to record configuration history, ordering is a correctness property.

**Design assumption violated:** The design uses `created_at_ms ASC` as the primary sort key, implicitly assuming wall-clock time is non-decreasing. The `created_at_monotonic_nanos` column exists specifically to avoid this problem but `in_range` does not use it.

**Suggested mitigation:** Change `in_range`'s ORDER BY to `(created_at_monotonic_nanos, config_version)`. `created_at_monotonic_nanos` is monotonically non-decreasing within a single daemon run; `config_version` handles cross-restart ordering (per the compound sort key established in prior rounds). The `from_ms`/`to_ms` range filter on `created_at_ms` can remain for selection purposes — only the ORDER BY needs to change.

---

### [MEDIUM] `regen-snapshot-hashes` silently no-ops when the entire DB contains only legacy-version rows

**Category:** Logic Flaws

**Trigger:** `regen-snapshot-hashes` filters `WHERE canonical_json_version = CANONICAL_JSON_VERSION`. If the DB contains only rows at version 0 (a legacy version), the tool finds zero matching rows, processes zero, reports "0 mismatches" — and exits 0. An operator concludes the DB is clean when in reality the tool had nothing to check.

**Consequence:** False confidence in DB integrity after a canonical JSON version bump. An operator running the tool expecting it to verify all 50,000 legacy rows gets a silent no-op.

**Design assumption violated:** The design assumes "skipped (legacy version N)" rows are a minority edge case. The tool's success signal is misleading when skipped rows are the majority.

**Suggested mitigation:** After processing, emit a summary: "N rows at version CURRENT processed; M rows at legacy versions skipped." If `M > 0` and `N == 0`, exit with a non-zero code or emit `tracing::warn!` so the operator knows the tool had nothing to verify.

---

### [MEDIUM] `override_run_id_for_current_thread` is `pub(crate)` — integration tests in `crates/adapters` and `crates/cli` cannot use it, making cross-restart ordering untestable at the integration level

**Category:** Abuse Cases

**Trigger:** `override_run_id_for_current_thread` is declared `#[cfg(test)] pub(crate)` inside `daemon_clock`. Integration tests in `crates/adapters/tests/` or `crates/cli/tests/` are separate crates — `pub(crate)` means "visible within the same crate," which excludes integration test binaries compiled as separate crates. Those tests cannot call `override_run_id_for_current_thread` and therefore cannot simulate multiple daemon runs to test the compound sort key.

Additionally, `SnapshotWriter::new` panics (rather than returning `Err`) if `daemon_clock::init()` was never called. Integration tests that construct `SnapshotWriter` without calling `init()` first receive an unrecoverable panic — not a `StorageError` — breaking the design's own convention that construction errors are typed.

**Consequence:** The cross-restart ordering invariant (`(created_at_ms, daemon_run_id, created_at_monotonic_nanos)`) is untestable from adapter or CLI integration tests. The `SnapshotWriter::new` panic on missing `init()` is also unrecoverable as a typed error from outside the crate.

**Design assumption violated:** The design assumes `pub(crate)` visibility is sufficient for all test scenarios. It is not: integration tests are external crates.

**Suggested mitigation:** Either (a) expose `override_run_id_for_current_thread` as `#[cfg(test)] pub` at the `crates/adapters` crate root so integration test binaries can import it, or (b) add an explicit `run_id_override: Option<String>` parameter to `SnapshotWriter::new` (test-builds only, via `#[cfg(test)]`) that bypasses the OnceLock — and change the missing-init panic to return `Err(StorageError::DaemonClockNotInitialized)`.

---

### [LOW] `children_of` lacks a `caddy_instance_id` filter, allowing cross-instance child enumeration given a known parent `SnapshotId`

**Category:** Authentication & Authorization

**Trigger:** The `children_of` algorithm queries `WHERE parent_id = ? AND caddy_instance_id = ?` — but the design description lists the signature without explicit instance scoping, and the `SnapshotFetcher` is constructed with its own `instance_id`. If the implementation omits the `AND caddy_instance_id = self.instance_id` predicate (the spec does not make it explicit in the algorithm text as it does for `by_id`), a caller with a known `SnapshotId` from instance A can enumerate all children across all instances.

**Consequence:** Cross-instance snapshot enumeration for any caller who knows a parent id. In a multi-tenant Caddy fleet, this is a cross-tenant read.

**Design assumption violated:** The design scopes `by_id` and `by_config_version` to `WHERE … AND caddy_instance_id = ?`, but the `children_of` algorithm text does not make this explicit, leaving the instance-scoping requirement implicit.

**Suggested mitigation:** Make the instance-scoping requirement explicit in the `children_of` algorithm text: "Query: `WHERE parent_id = ? AND caddy_instance_id = ? ORDER BY created_at_ms ASC, config_version ASC LIMIT ? OFFSET ?`, binding `self.instance_id`." Add a test: `tests::children_of_does_not_return_children_from_other_instances` — insert a parent for instance A with children, call `children_of` scoped to instance B, assert empty result.

---

## Summary

**Critical:** 0 &nbsp; **High:** 2 &nbsp; **Medium:** 3 &nbsp; **Low:** 1

**Top concern:** The `HashCollision` return path inside an open transaction permanently burns the allocated `config_version` number, creating a version gap that downstream consumers cannot distinguish from data loss. The fix is explicit: mandate `tx.rollback().await` before every error-return inside the transaction and add a test verifying no gap appears after a simulated collision.
