# Adversarial Review — Phase 05 — Round 11

**Design summary:** A content-addressed, append-only snapshot store backed by SQLite (WAL mode) for a Caddy reverse-proxy configuration daemon. Snapshots are identified by SHA-256 of canonical JSON, protected from mutation by database-layer triggers, and written through an OCC protocol that assigns monotonically increasing `config_version` values per `caddy_instance_id`. Three-layer Rust workspace enforces I/O separation.

**Prior rounds:** 10 prior rounds reviewed — all previously identified issues are marked as addressed. No prior findings are re-raised below.

---

## Findings

### [HIGH] Step-7 pre-transaction dedup check is non-atomic with step-13 INSERT, creating a window where two writers both skip the dedup path and race to INSERT

**Category:** Race Conditions

**Trigger:** Two concurrent callers submit the same `desired_state` for the same `caddy_instance_id`. Both execute step 7 simultaneously; both find no row and proceed to step 8. Both acquire `BEGIN IMMEDIATE` sequentially (SQLite serialises these). The first commits successfully. The second then executes the plain `INSERT` at step 13. The `id` (SHA-256) is already present in the table — but as a PRIMARY KEY collision across the whole table, not a UNIQUE violation on `(caddy_instance_id, config_version)`. The INSERT fails with a PRIMARY KEY constraint error.

**Consequence:** Step 13 dispatches on "UNIQUE violation on `id`" vs "any other constraint error". A PRIMARY KEY collision is also a UNIQUE constraint error at the SQLite level, but the dispatch logic (step 14) checks `SELECT id FROM snapshots WHERE id = ? AND config_version = ?` — the second writer's `config_version` is different (it incremented after the first committed), so that query returns no row. The design then falls through to the instance-scoped length check, finds the row under the same `caddy_instance_id`, compares lengths, byte-compares, and correctly returns `Deduplicated`. So the outcome is correct — but only by accident of the step-14 fallback path. The actual dispatch pivot ("did my INSERT land?") uses `config_version` from *this write attempt*, which is correct, but the reasoning is fragile: if the step-14 query were ever changed to check only `id` (not `id AND config_version`), it would misclassify a deduplicated write as a landed insert. The structural flaw is that the design relies on step-14's fallback to rescue a scenario that step-7 was supposed to prevent, with no explicit comment documenting why this is safe.

**Design assumption violated:** Step 7 is described as a "pre-transaction id check" that eliminates the dedup-after-insert path as the common case. The design implicitly assumes that when step 7 finds no row, the subsequent INSERT is the first insertion of that `id`. This holds for single writers but not for concurrent writers submitting identical payloads.

**Suggested mitigation:** Document explicitly in step 14 that the `config_version` pivot in the "did my INSERT land?" query is load-bearing for the concurrent-dedup case — specifically, that the second writer will NOT find a row at `(id, new_version_for_writer_2)` because writer 1 committed at `new_version_for_writer_1`, and this causes correct fallthrough to the instance-scoped dedup check. Add an explicit note: "The step-14 existence check MUST bind both `id` AND `config_version` — checking only `id` would incorrectly classify this case as a landed insert." Add a regression test `tests::concurrent_identical_writes_both_return_correct_outcome` with a multi-thread Tokio test pinning both tasks through steps 7–13 via a Barrier, asserting one returns `Inserted` and the other returns `Deduplicated`, and the DB contains exactly one row.

---

### [MEDIUM] `children_of` and `in_range` OFFSET pagination is non-deterministic when new children are inserted between pages

**Category:** Logic Flaws

**Trigger:** A caller fetches page 0 of `children_of(parent, page=0)` and receives 500 rows. While the caller processes those rows, another writer inserts a new child of the same `parent` with a `created_at_ms` value that sorts before row 200 of the already-returned page (e.g., a snapshot written by a host with a drifted wall clock — only `created_at_monotonic_nanos` is process-local-monotonic, and it resets on restart). The caller then fetches page 1 (`OFFSET 500`). SQLite re-executes the query from scratch. The newly inserted row shifts the result set by one position, so the row at physical position 500 is one that was already returned on page 0 — causing the caller to see a duplicate row and miss one row entirely.

**Consequence:** A consumer iterating all children via `children_of` receives a duplicate row and misses one. For Phase 7 chain traversal (rollback targets), this means a rollback point is silently skipped. Since the store is append-only, inserts only shift rows — they do not remove them — so the worst case is a duplicate, not a missing row. However, because `created_at_ms` is the primary sort key for `children_of` and can be non-monotonic across hosts/restarts, a concurrent insert can genuinely sort into the middle of a previous result set. The design's pagination caveat already mentions this for keyset pagination but does not specify what callers should do instead.

**Design assumption violated:** OFFSET-based pagination assumes the result set is stable between pages. The design bounds performance via `MAX_PAGE_OFFSET` but does not guard against result-set instability from concurrent inserts. The pagination caveat in `SnapshotFetcher`'s doc comment recommends keyset pagination for completeness but does not make it mandatory.

**Suggested mitigation:** Add an explicit warning to both `children_of` and `in_range`: "Offset-based pagination is subject to insertion skew: a concurrent write may cause duplicate or missing rows across pages. For completeness-guaranteed traversal, use keyset pagination with the last-seen `(created_at_ms, created_at_monotonic_nanos, daemon_run_id, config_version)` tuple as the cursor." Strengthen the existing pagination caveat in `SnapshotFetcher`'s doc comment by specifying the keyset cursor column set explicitly so Phase 7 implementers do not have to derive it.

---

### [MEDIUM] `regen-snapshot-hashes` read-only scan has no explicit read transaction — concurrent inserts cause inconsistent multi-statement reads in WAL mode

**Category:** Logic Flaws

**Trigger:** The design states "no transaction needed (read-only)" and opens a connection in read-only mode. In SQLite WAL mode, each SQL statement is implicitly wrapped in its own read transaction. A `regen` run that iterates with multiple SELECT statements — one per row or in batches — sees a potentially different WAL checkpoint on each statement. A writer commits a new snapshot between two `regen` SELECT batches. The new row is visible in the second batch but not the first. More critically, if the new row's parent was in the first batch (already processed), and `regen` tracks parent-child consistency, the parent appears to exist but the child appears parentless.

**Consequence:** `regen` may report false-positive integrity anomalies — rows that appear to lack parents they actually have, or counts that do not add up because the snapshot of the DB changed mid-scan. An operator sees exit-non-zero from `regen` and investigates a non-existent corruption. In `--strict` mode this is guaranteed to be triggered whenever a writer is active during the scan. For a verification tool whose entire value is "tells you the truth about the DB," inconsistent reads are a correctness failure.

**Design assumption violated:** The design assumes that read-only access with no transaction is sufficient for a consistency-checking tool. In WAL mode, multiple implicit read transactions within a single tool invocation can observe different DB states.

**Suggested mitigation:** Wrap the entire `regen` scan in a single explicit `BEGIN DEFERRED` read transaction. SQLite allows reads within a `BEGIN` on a read-only connection; this pins the WAL read snapshot for the duration of the run, ensuring all SELECT statements within one `regen` invocation see a consistent point-in-time view. Add a note: "Even in read-only mode, `regen` MUST begin an explicit read transaction before the first SELECT and commit/rollback after the last. This prevents mid-run inserts from producing false positive integrity anomalies."

---

### [LOW] `daemon_clock::override_run_id_for_current_thread` is unsound under Tokio's work-stealing scheduler — thread migration silently loses the override

**Category:** State Manipulation

**Trigger:** `override_run_id_for_current_thread` sets a thread-local. An integration test annotated `#[tokio::test(flavor = "multi_thread")]` uses this override, then `await`s a future. Tokio's work-stealing scheduler may migrate the task to a different OS thread at any `.await` point. On the new thread, the thread-local is unset. The next call to `daemon_clock::run_id()` checks the thread-local (not set), falls through to the `OnceLock` — which may be set to the global `run_id` from `daemon_clock::init()`, returning the wrong run ID silently. Or if `init()` was never called, `run_id()` panics.

**Consequence:** Integration tests using `override_run_id_for_current_thread` with `flavor = "multi_thread"` silently receive the wrong `run_id` after any `await` point, causing the cross-restart ordering tests (`cross_restart_ordering_uses_compound_key`) to write rows with the wrong `daemon_run_id` and potentially assert incorrect ordering. The failure mode is a silent wrong result, not a panic.

**Design assumption violated:** The design makes the override `#[cfg(test)] pub` for integration test use but does not specify that it is only valid in single-threaded Tokio contexts. The test `cross_restart_ordering_uses_compound_key` requires this override but does not specify `flavor = "current_thread"`.

**Suggested mitigation:** Add a doc comment to `override_run_id_for_current_thread`: "This function is ONLY safe when called from a single-threaded Tokio runtime (`flavor = "current_thread"`) or from a non-async context. Under `flavor = "multi_thread"`, task migration across `.await` points loses the thread-local silently. All tests that use this function MUST specify `#[tokio::test(flavor = "current_thread")]`." Enforce this by adding a `#[cfg(test)]` assertion in the function body that logs a warning if `tokio::runtime::Handle::try_current()` indicates a multi-thread runtime (best-effort detection, not foolproof).

---

## Summary

**Critical:** 0 &nbsp; **High:** 1 &nbsp; **Medium:** 2 &nbsp; **Low:** 1

**Top concern:** The step-7/step-14 concurrent-dedup correctness relies on the `config_version` pivot in the step-14 existence query — a fact that is structurally correct but not documented, making future refactoring unsafe. The fix is documentation and a targeted regression test, not a code change.
