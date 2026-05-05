# Adversarial Review — Phase 05 — Round 3

**Design summary:** Phase 5 implements a content-addressed, append-only snapshot store. The round 3 design restructures the write algorithm so that the pre-transaction id check (step 7) occurs outside `BEGIN IMMEDIATE`, the `daemon_clock` module uses `OnceLock` with a panicking `init()` enforcing process-level singleton semantics, `expected_version = Some(v)` where `v < 0` is explicitly rejected, `by_id` is instance-scoped with a separate `by_id_global`, and `regen-snapshot-hashes` requires `--require-version-bump`. All round 1 and round 2 findings are addressed or documented as accepted risk.

**Prior rounds:** Round 1 (11 findings) and round 2 (9 findings) are all addressed in the revised design. No round 1 or round 2 finding is re-raised below.

---

## Findings

---

### HIGH: Pre-transaction id check creates a new TOCTOU path — step 14 misclassifies a `snapshots.id` UNIQUE constraint fire as `VersionRace`

**Category:** Race Conditions

**Trigger:** Two concurrent callers independently serialize the same `DesiredState` to identical bytes and call `write()` simultaneously. The content-address `H` is computed by both. Step 7 (pre-transaction SELECT by id) executes for both before either has inserted — the row does not yet exist, so both receive no-row and continue. Both acquire `BEGIN IMMEDIATE` sequentially (SQLite permits only one writer at a time). Caller A inserts successfully and commits. Caller B, now inside `BEGIN IMMEDIATE`, reaches step 13 (INSERT). The INSERT fails with `UNIQUE constraint failed: snapshots.id` (primary key collision on `H`). Step 14 now states: "If the INSERT fails with any UNIQUE constraint failed error, this is a `config_version` collision." The code returns `VersionRace`. But the collision is on `id`, not on `config_version`. The correct response is `Deduplicated` — or at minimum a re-check for deduplication — not `VersionRace`.

**Consequence:** The caller receives `WriteError::VersionRace` for a scenario that is actually a clean, harmless deduplication of identical content. `VersionRace` signals to the caller "retry with a re-fetched version" — so the caller refetches state, computes the same canonical JSON, gets the same `H`, and loops back into the same race. This creates a retry loop that never terminates as long as there is concurrent traffic writing identical content. For an idempotent re-apply pattern (many callers re-applying the same large config), `VersionRace` is returned on every concurrent write, none of which would have produced a row. The loop exhausts the caller's retry budget and surfaces as a spurious write failure for an operation that should have succeeded silently.

**Design assumption violated:** Step 14 asserts "the id collision was already handled in step 7 before acquiring the lock." This is true only when step 7 ran *after* the first insert committed. When two callers execute step 7 before either inserts, both see no-row, both proceed, and the `id` constraint fires inside the lock for the second caller — a case step 14's comment explicitly claims cannot happen.

**Suggested mitigation:** Inside the `BEGIN IMMEDIATE` transaction, after the INSERT fails with `UNIQUE constraint failed`, execute a SELECT by id. If the row now exists (concurrent insert of same content), compare bytes and return `Deduplicated` or `HashCollision` accordingly. If the row does not exist (the only remaining case is `config_version` collision), return `VersionRace`. No constraint-name parsing is needed — the presence or absence of the row by id determines which path to take. This keeps the algorithm structure correct without reverting to fragile error-string parsing.

---

### HIGH: `regen-snapshot-hashes` without `--require-version-bump` silently regenerates fixtures — the flag is opt-in, not enforced

**Category:** Rollbacks

**Trigger:** The design states: "The `--require-version-bump` flag is MANDATORY: the tool refuses to regenerate unless `CANONICAL_JSON_VERSION` was incremented." However, the flag is a CLI argument. If a developer runs `cargo run --bin regen-snapshot-hashes` (omitting the flag), the tool's behaviour is unspecified by the design. The design says the flag "refuses to regenerate" when absent — but the standard CLI convention for a `--flag` is that omitting it means the check is not performed, not that the tool refuses to run at all. The design's phrasing is ambiguous: "refuses to regenerate if `CANONICAL_JSON_VERSION` has not been incremented" could mean the tool runs but rejects a no-version-bump scenario (which is the guard), or it could mean the flag is required to even start (which would be enforced by `clap`'s `required = true` attribute).

**Consequence:** If the flag is optional (the default clap behavior for `--flag`), running the tool without `--require-version-bump` bypasses the version-bump check entirely. A developer who runs `cargo run --bin regen-snapshot-hashes` in a panic to silence a failing test gets their buggy hashes baked in with no warning. The round 2 finding was intended to close this hole, but the design's stated fix only works if the flag is required or if the tool refuses to run without it. If the flag is simply an on/off toggle that defaults to off, the fix is incomplete.

**Design assumption violated:** The design presents `--require-version-bump` as a safety gate, but the flag's name and conventional CLI semantics imply it is optional. For the gate to be meaningful it must either be `required = true` in clap (so running the tool without it is an error) or the default behavior (no flag) must also enforce the version-bump check, making `--require-version-bump` redundant. The current framing — an optional flag that enables the check — is backwards: the check should be the default, not the opt-in.

**Suggested mitigation:** Invert the flag to `--skip-version-bump-check` (opt-out), making the version-bump check the default behavior of the tool. Or make `--require-version-bump` a positional marker required by clap (`required = true`). Document in `core/README.md` which it is. Without this clarification, the design leaves the tool's behavior on the common invocation path (no flags) undefined, and the likely behavior (flag optional, check skipped) defeats the round 2 fix's purpose.

---

### HIGH: `daemon_clock::init()` panic on double-call destroys test isolation — all adapter tests that need `SnapshotWriter` share one `OnceLock`

**Category:** Abuse Cases

**Trigger:** The `daemon_clock` module holds two `static OnceLock` values, which are process-wide statics. A `#[tokio::test]` annotated test that calls `daemon_clock::init("ulid-1")` succeeds. A second `#[tokio::test]` in the same test binary that calls `daemon_clock::init("ulid-2")` panics with `"daemon_clock::init() called more than once"`. Tokio's test runner by default runs tests in the same process on a per-test runtime, so the statics persist across tests. The design specifies `init()` panics on double-call and that `SnapshotWriter::write` calls `daemon_clock::run_id()` (which panics if `init()` was not called). The design does not specify how tests call `init()` without violating the OnceLock invariant.

**Consequence:** At minimum, only one test in the entire `trilithon-adapters` test suite can call `daemon_clock::init()`. Every other test that exercises `SnapshotWriter::write` must either rely on side effects of the first test (fragile, ordering-dependent), call `daemon_clock::run_id()` without `init()` (panics), or use `cfg(test)` shims that bypass the `daemon_clock` call path entirely (not specified in the design). In practice, the named tests — `tests::insert_writes_daemon_run_id_from_writer`, `tests::root_snapshot_has_null_parent`, all OCC tests — all call `write()`, which calls `daemon_clock::run_id()`. The test harness will have one survivor and many panics, or the tests must all run serially (which Rust integration tests do by default in one binary, but `#[tokio::test]` creates per-test runtimes while sharing the same process address space and statics).

**Design assumption violated:** The `daemon_clock` module design assumes it is initialized once in `main()` before any async tasks run, which is correct for the daemon. The design does not extend this assumption to the test environment, where the module-level statics cannot be reset between tests. The structural fix from round 2 (removing `daemon_run_id` from the constructor) is correct for production but introduces a new failure mode in the test harness.

**Suggested mitigation:** One of: (a) Add a `#[cfg(test)] fn reset_for_testing()` that clears the `OnceLock` values via `unsafe` (inherently fragile across concurrent tests but workable with `serial_test` crate); (b) Provide a `cfg(test)` override in `daemon_clock` where `run_id()` and `process_start()` return test-local defaults without panicking, allowing any test to call `write()` without calling `init()`; (c) Make `daemon_clock::init()` idempotent for identical values (only panic on conflicting double-init); (d) Refactor `SnapshotWriter::write` to take the `run_id` and `process_start` as injected parameters in `cfg(test)` builds. The design must specify which approach is used — the current design leaves tests with no path to call `write()` in the second-or-later test without a panic.

---

### MEDIUM: Pre-transaction SELECT in step 7 fetches full `desired_state_json` TEXT (up to 10 MiB) for every write, even when no dedupe occurs

**Category:** Resource Exhaustion

**Trigger:** Step 7 executes `SELECT desired_state_json FROM snapshots WHERE id = ? LIMIT 1`. When the id does not exist (the common case for a new snapshot), the SELECT returns no rows and the query cost is low. When the id does exist (dedupe case), the SELECT fetches the full `desired_state_json` column, which can be up to 10 MiB. For an idempotent re-apply pattern (same large config, frequent writes), every write causes a 10 MiB read even though the comparison will always produce `Deduplicated`.

**Consequence:** Under sustained idempotent-reapply traffic with large configs, every write issues a 10 MiB SELECT outside the write lock. At 10 writes/second with a 10 MiB config, this is 100 MiB/s of heap churn from the pre-check alone. The pre-check does not hold the write lock, so it does not block other writers, but it generates sustained allocation pressure in the async runtime.

**Design assumption violated:** The pre-transaction SELECT was moved outside the lock to keep write-lock hold time minimal. The full-column fetch was the round 2 design's mechanism for the in-transaction byte comparison. Moving the comparison outside the lock preserved the fetch granularity without re-evaluating the cost.

**Suggested mitigation:** Change step 7 SELECT to `SELECT length(desired_state_json) FROM snapshots WHERE id = ? LIMIT 1`. If the returned length differs from `bytes.len()`, return `HashCollision` immediately. If lengths match, issue a second SELECT for the full body for byte-comparison. For the common case (no row found), identical cost. For the dedupe case with matching length, one extra round-trip is acceptable versus routinely allocating 10 MiB per write.

---

### MEDIUM: `SELECT MAX(config_version), id` uses a non-standard SQLite aggregate-with-bare-column extension

**Category:** Logic Flaws

**Trigger:** Step 10 executes `SELECT MAX(config_version), id FROM snapshots WHERE caddy_instance_id = ? AND config_version = (SELECT MAX(config_version) FROM snapshots WHERE caddy_instance_id = ?)`. Using an aggregate function (`MAX`) alongside a non-aggregated column (`id`) without a `GROUP BY` is undefined in standard SQL. SQLite returns the `id` of the row that contains the max value as a SQLite-specific extension, but this is not guaranteed by the SQL standard or contractually stable across SQLite versions.

**Consequence:** If a future SQLite version changes this behavior, `conflicting_snapshot_id` in `VersionConflict` silently returns the wrong snapshot id. The UI uses this id to render a three-way diff (ADR-0012). An incorrect id causes the UI to display the wrong conflicting state, producing a misleading diff that could cause the user to edit against the wrong base.

**Suggested mitigation:** Rewrite step 10 as: `SELECT id, config_version FROM snapshots WHERE caddy_instance_id = ? ORDER BY config_version DESC LIMIT 1`. This is standard SQL, deterministic, index-friendly on `snapshots_config_version`, and returns the same row without relying on SQLite's bare-column aggregate extension.

---

### MEDIUM: `by_id_global` is `pub` on `SnapshotFetcher` — restriction to integrity tools is documentation-only

**Category:** Authentication & Authorization

**Trigger:** `SnapshotFetcher::by_id_global` is a `pub` method. Any code that constructs a `SnapshotFetcher` can call it, including the Phase 9 HTTP handler layer. A developer building the snapshot-fetch endpoint may reach for `by_id_global` for convenience or because the instance id is not threaded through the call site yet.

**Consequence:** When T3.1 (multi-instance fleet) ships, any code path already using `by_id_global` becomes a cross-tenant data exposure bug. Snapshot data is immutable; leaked data cannot be redacted.

**Suggested mitigation:** Move `by_id_global` to a `pub(crate)`-only function in a separate `integrity` submodule, making it structurally inaccessible from HTTP handler code unless explicitly imported. Or use `#[doc(hidden)]` with a module-visibility restriction enforced by an architecture test asserting the method is not called from HTTP handler files.

---

### LOW: Architecture §6.5 still missing `daemon_run_id` column — round 2 LOW finding not yet closed

**Category:** Orphaned Data

**Trigger:** Round 2 identified that `docs/architecture/architecture.md` §6.5 does not include the `daemon_run_id` column added by migration 0003. The round 3 design changes add prose mentioning `daemon_run_id` in slice 5.3 but do not close the round 2 LOW by updating the §6.5 schema block itself.

**Consequence:** Phase 6, 7, and 9 developers reading §6.5 will not know `daemon_run_id` exists, will not include it in ORDER BY clauses, and the cross-restart ordering rule documented in slice 5.3 will have no schema anchor in the authoritative architecture reference.

**Suggested mitigation:** Update `docs/architecture/architecture.md` §6.5 to add `daemon_run_id TEXT NOT NULL DEFAULT ''` with an inline comment referencing the ordering rule. Mark as a phase exit blocker alongside open question 6.

---

### LOW: `daemon_clock::elapsed_since_boot()` helper would eliminate silent misuse of `process_start().elapsed()`

**Category:** Logic Flaws

**Trigger:** Step 6 uses `daemon_clock::process_start().elapsed().as_nanos()`. The Rust type system prevents the worst misuses (`Instant::as_nanos()` does not exist), but the two-call chain puts the semantic contract (caller must always call `.elapsed()`) at the call site rather than in the module. A future developer adding another use of `process_start()` may not realize `.elapsed()` is always intended.

**Consequence:** Low risk — Rust's type system largely prevents misuse. The issue is documentation consistency: `process_start()` returning a bare `Instant` describes the stored value, but the described semantic in the design is "elapsed duration since daemon boot."

**Suggested mitigation:** Add `daemon_clock::elapsed_since_boot() -> Duration` that encapsulates `process_start().elapsed()`. Use this in step 6. Keep `process_start()` for code that genuinely needs the raw `Instant` for comparisons.

---

## Summary

**Critical:** 0  **High:** 3  **Medium:** 3  **Low:** 2

**Top concern:** The step-7/step-14 TOCTOU for the `id` constraint (HIGH-1) is the most dangerous finding — under any concurrent idempotent-write pattern, the second concurrent write returns `VersionRace` instead of `Deduplicated`, creating an unresolvable retry loop for callers who have written the correct content. The fix is contained: after a UNIQUE constraint failure inside the lock, SELECT by id to determine which constraint fired rather than assuming `config_version`.

The `--require-version-bump` flag semantics (HIGH-2) and `daemon_clock` test isolation (HIGH-3) must both be resolved before implementation begins, since the fix designs for both are open in the TODO file.
