# Adversarial Review — Phase 05 — Round 4

**Design summary:** Phase 5 implements a content-addressed, append-only snapshot writer backed by SQLite with optimistic concurrency (ADR-0012). The round 4 design addresses all three HIGH and four lower-severity findings from round 3: step-14 dispatches by row presence (no constraint-name parsing), step-10 uses `ORDER BY … DESC LIMIT 1`, step-7 is length-first, `daemon_clock::init()` is idempotent for identical values, `elapsed_since_boot()` encapsulates the elapsed call, `by_id_global` is `pub(crate)`, and `regen-snapshot-hashes` enforces the version-bump check by default.

**Prior rounds:** All round 1, round 2, and round 3 findings are addressed in the revised design. No prior finding is re-raised below.

---

## Findings

---

### HIGH: Step-7 length check uses SQLite `length()` on TEXT — returns codepoint count, not byte count

**Category:** Logic Flaws

**Trigger:** Step 7 executes `SELECT length(desired_state_json) AS json_len FROM snapshots WHERE id = ? LIMIT 1`. SQLite's `length()` function, when applied to a TEXT-typed column, returns the number of Unicode codepoints — not the number of UTF-8 bytes. The comparison is `json_len != bytes.len()`, where `bytes` is the Rust `Vec<u8>` produced by the canonical serialiser (byte length). For any `desired_state_json` that contains characters outside the ASCII range — a route hostname with a non-ASCII label, an intent string in a non-Latin script, any string containing multibyte characters — `length(desired_state_json)` will be smaller than `bytes.len()`. Specifically, every two-byte UTF-8 codepoint counts as 1 in `length()` but 2 in `bytes.len()`.

**Consequence:** When a snapshot row exists whose body contains multibyte UTF-8 characters and the caller writes byte-identical content, the length check `json_len != bytes.len()` evaluates as true (despite byte-equal bodies), and the code returns `WriteError::HashCollision { id }` immediately — never fetching the body to confirm actual inequality. The correct deduplication path (`Deduplicated`) is unreachable for any snapshot whose `desired_state_json` contains multibyte characters. Every subsequent write of the same config is rejected with a spurious `HashCollision` error. This is the exact bug that slice 5.4's `intent` trigger fix (`CAST(x AS BLOB)`) was introduced to prevent — that fix appears in the trigger but was not carried into step 7's inline query.

**Design assumption violated:** Step 7 assumes `length(desired_state_json)` returns a byte count comparable to Rust's `Vec<u8>::len()`. SQLite does not make this guarantee; the assumption holds only for pure-ASCII content.

**Suggested mitigation:** Replace `length(desired_state_json)` with `length(CAST(desired_state_json AS BLOB))` in the step-7 SELECT. This returns byte count, matching Rust's `.len()` semantics exactly. The pattern is already established in the slice 5.4 trigger — apply the same idiom here.

---

### MEDIUM: Step-14 diagnostic SELECT runs inside `BEGIN IMMEDIATE` — counts against `write_timeout`, compounding under database load

**Category:** Timeouts & Retries

**Trigger:** When the INSERT in step 13 fails with a UNIQUE constraint error, step 14 executes a diagnostic `SELECT length(desired_state_json) AS json_len FROM snapshots WHERE id = ?` inside the still-open `BEGIN IMMEDIATE` transaction. Under heavy database load, lock acquisition may consume most of the `write_timeout` budget. If lock acquisition consumed 4.9 of the 5-second default timeout, the step-14 SELECT has 100 ms remaining. If the SELECT takes longer (e.g., the WAL is large), the `tokio::time::timeout` fires — returning `WriteError::Timeout` instead of `Deduplicated` or `VersionRace`.

**Consequence:** A caller that wrote identical content concurrently receives `Timeout` instead of `Deduplicated`. `Timeout` is ambiguous — the caller cannot determine whether the snapshot was inserted or not. If the caller retries after `Timeout`, it may attempt re-insert with a stale `expected_version` and receive `VersionConflict`. For an idempotent re-apply pattern, this is a spurious failure for an operation that succeeded.

**Suggested mitigation:** Either (a) cap the step-14 diagnostic SELECT with a separate short timeout (e.g., 500 ms) distinct from the overall `write_timeout`; or (b) use SQLite's `INSERT OR IGNORE` followed by a check of `changes()`, which avoids the conflict-dispatch SELECT entirely. Option (b): `INSERT OR IGNORE INTO snapshots …` — if `changes() == 1`, the insert succeeded; if `changes() == 0`, a constraint prevented it. Then issue a single follow-up SELECT to determine which case applies and return `Deduplicated` or `VersionRace` accordingly.

---

### MEDIUM: `concurrent_identical_writes_both_return_deduplicated` test cannot exercise the TOCTOU race with `tokio::join!` on the default single-threaded test runtime

**Category:** Abuse Cases

**Trigger:** The test uses `tokio::join!(write_a, write_b)` under the default `#[tokio::test]` single-threaded runtime. In a single-threaded runtime, `join!` interleaves the two futures at `.await` points but does not run them truly concurrently. The race condition — both futures complete step 7 before either reaches `BEGIN IMMEDIATE` — cannot occur in a single-threaded runtime because whichever future is polled first advances through steps 7–13 without yielding until it hits an async `.await` (the `BEGIN IMMEDIATE` acquire), at which point the lock is already held, and the second future's step 7 runs after the first future has committed.

**Consequence:** The test passes without ever exercising the step-14 dispatch path. A future refactor that removes or misimplements step 14's row-presence check will not be caught.

**Suggested mitigation:** Annotate the test with `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]` and use `tokio::spawn` for both writes with a `tokio::sync::Barrier(2)` synchronization point inserted via a test seam between step 7 and step 8, forcing both tasks past the pre-check before either acquires the lock. Without this, the test provides false assurance of TOCTOU coverage.

---

### MEDIUM: `daemon_clock::init()` idempotency sets `PROCESS_START` on first call only — tests see monotonically growing `elapsed_since_boot()`, not near-zero durations

**Category:** Logic Flaws

**Trigger:** `PROCESS_START.set(Instant::now())` is called on the first `init()` invocation and discarded on subsequent ones. In a test binary, the first test captures an `Instant`; all later tests share it. `elapsed_since_boot()` returns the duration since the *first* `init()` call in the binary, which grows throughout the test run. Tests written later in the binary may see `elapsed_since_boot()` return 30+ seconds or more.

**Consequence:** Tests that assert ordering invariants using `created_at_monotonic_nanos` across independently constructed test scenarios (different tests in the same binary) will see values that are already large rather than starting near zero. Property tests checking that monotonic nanos are strictly ordered within a run are correct but misleading — all tests share a single pseudo-run with no per-test reset. Future test authors may write magnitude-based assertions (e.g., "nanos < 1_000_000_000 for a freshly-written snapshot") that silently fail in CI after the test suite grows long.

**Suggested mitigation:** Add a note to the "Test setup" section explicitly documenting: "`elapsed_since_boot()` in tests returns the time since the first `daemon_clock::init()` call in the binary — not per-test zero. Assertions on `created_at_monotonic_nanos` must use relative comparisons between snapshots written in the same test, not absolute magnitude checks." This prevents future test authors from writing brittle assertions.

---

### LOW: Step-10 `ORDER BY config_version DESC LIMIT 1` — SQLite planner may not reverse-scan the ascending unique index

**Category:** Logic Flaws

**Trigger:** The `snapshots_config_version` index is presumably `CREATE UNIQUE INDEX … (caddy_instance_id, config_version)` (ascending, the SQLite default). Step 10 queries `ORDER BY config_version DESC LIMIT 1`. SQLite can reverse-scan an ascending index, but the planner may choose a full-partition forward scan instead, depending on statistics and SQLite version.

**Consequence:** For a long-lived instance with many snapshots, every write inside `BEGIN IMMEDIATE` incurs a full forward scan of that instance's index partition. Under the 5-second `write_timeout`, this compounds with the parent lookup and step-14 diagnostic SELECT.

**Suggested mitigation:** Add an integration test to the `migrations_snapshot_extras` suite that inserts 1000 snapshots and runs `EXPLAIN QUERY PLAN` on the step-10 query, asserting the plan uses `SCAN snapshots USING INDEX snapshots_config_version` (or the descending variant). If the planner uses a full scan, add a descending index: `CREATE UNIQUE INDEX snapshots_config_version ON snapshots (caddy_instance_id, config_version DESC)`. Note that changing index order from ascending to descending is a migration change — it must be in a new migration (0005) if 0004 has already been applied.

---

### LOW: `by_id_global` is `pub(crate)` in `crates/adapters` — HTTP handler code in the same crate has unrestricted access

**Category:** Authentication & Authorization

**Trigger:** `by_id_global` is `pub(crate)` on `SnapshotFetcher` in `crates/adapters`. If Phase 9's HTTP handler also lives in `crates/adapters` (which is architecturally plausible for an adapter-layer HTTP server), `pub(crate)` provides no restriction — any module within `crates/adapters` can call it.

**Consequence:** When T3.1 (multi-instance fleet) lands, any HTTP handler code that already calls `by_id_global` becomes a cross-tenant data exposure bug. The `pub(crate)` restriction is only effective if the HTTP handler is in a different crate.

**Suggested mitigation:** Add to the slice 5.6 exit conditions: "Phase 9's HTTP handler MUST reside in `crates/cli` or a dedicated `crates/server` crate, not in `crates/adapters`, for the `pub(crate)` restriction on `by_id_global` to have compiler-enforced effect. This constraint is an architecture boundary requirement and MUST be documented in `docs/architecture/architecture.md`."

---

## Summary

**Critical:** 0  **High:** 1  **Medium:** 3  **Low:** 2

**Top concern:** Step-7 uses `length(desired_state_json)` (codepoint count) instead of `length(CAST(desired_state_json AS BLOB))` (byte count). This causes `HashCollision` to be returned spuriously for any snapshot whose config contains non-ASCII UTF-8 text — including hostnames with internationalized labels or any intent written in a non-Latin script. The fix is a single query change, identical to the one already made for the intent trigger in slice 5.4.
