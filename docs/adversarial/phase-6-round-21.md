# Adversarial Review — Phase 6 — Round 21

**Design summary:** Phase 6 adds a tamper-evident, append-only audit log to Trilithon: an `AuditWriter` with a dedicated serialised `tokio::sync::Mutex<Option<SqliteConnection>>`, a SHA-256 `prev_hash` chain verified via a stateful batch API (`ChainVerifyState` / `verify_batch` / `verify_finish`), a `RedactedDiff` newtype for secrets-safe diffs, an `Actor` enum with `from_kind_id` reverse-mapping and unknown-kind fallback, startup integrity checks, and a query API with a 10 MB soft cap.

**Prior rounds:** Rounds 1–20 reviewed. All R20 findings were addressed before this round:

- R20-M1 (`std::sync::Mutex` unspecified; implementer default causes E0277 on multi-thread executor): CLOSED — line 64 now reads "`AuditWriter` MUST hold a `tokio::sync::Mutex<Option<SqliteConnection>>` — `std::sync::Mutex` is incorrect because `record` holds the guard across multiple `.await` points; `std::sync::MutexGuard` is `!Send`, causing `E0277` on any multi-thread Tokio executor."
- R20-M2 (`chain::verify` synchronous-iterator API incompatible with async paginator; no bridge specified): CLOSED — line 45 now specifies `struct ChainVerifyState { last_computed_hash: Option<String>, sentinel_count: u64 }`, `ChainVerifyState::new()`, `fn verify_batch(batch: &[AuditRow], state: &mut ChainVerifyState) -> Result<(), ChainError>`, and `fn verify_finish(state: &ChainVerifyState) -> Result<(), ChainError>`; startup paginator calls `verify_batch` per 500-row page then `verify_finish`.
- R20-L1 (Cargo.toml feature list still offered `strum::Display` as an alternative, potentially re-triggering E0119): CLOSED — line 15 now reads "`strum = { version = '…', features = ['derive'] }` to `core/Cargo.toml` — the `derive` feature enables all strum macros including `EnumString`, `EnumIter`, and `EnumCount`. **Do NOT add `#[derive(strum::Display)]`**".

No items are carried forward from prior rounds.

---

## Findings

### MEDIUM — `verify_finish` semantics are underspecified: the function receives `&ChainVerifyState` but the design does not say what it is required to check, making two valid implementations possible that produce opposite results on the same state

**Category:** Logic flaw

**Trigger:** The `chain::verify` task (line 45) specifies: "`fn verify_finish(state: &ChainVerifyState) -> Result<(), ChainError>` — validates the terminal state (returns `Ok(())` for a chain that has not yet started)." No other requirement on `verify_finish` is given. `ChainVerifyState` carries two fields: `last_computed_hash: Option<String>` and `sentinel_count: u64`.

There are at least two reasonable interpretations of what `verify_finish` is supposed to check:

Interpretation A — `verify_finish` is a no-op / informational summary function. It checks whether any non-sentinel chain entries were processed (`last_computed_hash.is_some()`) and returns `Ok(())` unconditionally; it exists only to provide a place for a potential future invariant and to make the calling convention symmetric (batch + finish). Under this interpretation, `verify_finish` always returns `Ok(())`.

Interpretation B — `verify_finish` validates that the last row's hash was properly chained. Under this interpretation, the function checks some terminal condition — but the design gives no terminal condition to check. The design specifies that `verify_batch` already returns early on the first broken link. There is no "pending hash to validate" concept left for `verify_finish` to handle; all validation happens inside `verify_batch`.

Because the design says `verify_finish` "validates the terminal state" but gives only one specific case ("returns `Ok(())` for a chain that has not yet started"), an implementer reading the design does not know:
- Whether `verify_finish` has any non-trivial return path to `Err`.
- If so, which `ChainError` variant it may return and under what condition.
- Whether `verify_finish` performs any work beyond inspecting the final `last_computed_hash`.

Concrete failure scenario: Implementer A treats `verify_finish` as the place to check whether the chain concluded with a valid sentinel count (e.g., returns `Err(ChainBroken { … })` if `sentinel_count > 0` and `last_computed_hash.is_none()`, which would mean all rows were ZERO_SENTINEL and no chained row ever appeared). Implementer B, seeing the spec says "`Ok(())` for a chain that has not yet started," treats `verify_finish` as always returning `Ok(())`. These two implementations pass the existing specified tests (the tests do not exercise a case that distinguishes them) but produce different runtime behavior when called on a log containing only ZERO_SENTINEL rows followed by no chained rows.

The composition test for `verify_batch` + `verify_finish` in the "Done when" criteria (line 46) tests: (a) a tampered row, (b) a chain intact after ten appended rows, (c) ZERO_SENTINEL rows skipped (but hashed), (d) `""` returns `EmptyHash`, (e) startup log, (f) all-ZERO_SENTINEL → `Ok(())`, (g) tamper-sentinel, (h) stability test, (i) 3-row chain with break at row 2. None of these tests observe a non-`Ok(())` return from `verify_finish` specifically — they test the overall sequence and observe the aggregate result. An implementer cannot infer the expected `verify_finish` behavior from these tests alone.

**Consequence:** Two compliant implementations of `verify_finish` produce different return values for some inputs. If `verify_finish` is supposed to detect a condition (e.g., a chain that started but was never finished cleanly), a no-op implementation silently suppresses that detection. If `verify_finish` is supposed to be a no-op, an implementation that returns `Err` for some state causes spurious startup failures. Because the startup paginator calls `verify_finish(&state)?` and propagates any `Err`, a `verify_finish` that erroneously returns `Err` prevents chain verification from completing — resulting in a logged chain-integrity alarm on every startup.

**Design assumption violated:** The design assumes that "validates the terminal state" and the single specified case ("returns `Ok(())` for a chain that has not yet started") are sufficient to uniquely determine `verify_finish`'s behavior. They are not: an implementer who treats the "chain has not yet started" case as the only non-trivial case will write a trivially `Ok(())` function, while an implementer who reads "validates the terminal state" as requiring substantive checks will write additional logic without guidance on what to check.

**Suggested mitigation:** Amend the `chain::verify` task to specify `verify_finish` exhaustively: "**`verify_finish(state: &ChainVerifyState) -> Result<(), ChainError>`** checks terminal state. Current contract: always returns `Ok(())`. `verify_batch` performs all link validation incrementally and returns early on any break; there is no deferred validation left for `verify_finish`. The function exists as an explicit termination point for the calling convention and as a future extension point (e.g., for a sentinel-count floor check). Implementations MUST NOT add additional error conditions until the design is updated." Add to the "Done when" criteria: "A test calls `verify_batch` on a single valid 3-row chain (no sentinels), then calls `verify_finish` and asserts `Ok(())`; a separate test calls `verify_batch` on all-ZERO_SENTINEL rows, then calls `verify_finish` and asserts `Ok(())`; both tests confirm that `verify_finish` does not return `Err` on these inputs."

---

### MEDIUM — `verify_batch` ZERO_SENTINEL handling for a mixed batch is underspecified: when a batch contains both ZERO_SENTINEL rows and chained rows, the transition semantics are not stated

**Category:** Logic flaw

**Trigger:** The `verify_batch` specification (line 45) states: "For rows where `prev_hash == ZERO_SENTINEL` (pre-epoch): compute `sha256(canonical_json(row))` and overwrite `last_computed_hash` with it (do NOT assert that `row.prev_hash` matches any predecessor)." The specification also states: "`chain::verify` MUST return `Err(ChainBroken { … })` on the first broken link."

These two rules interact on batches that contain a transition from ZERO_SENTINEL rows to chained rows. The transition happens when `prev_hash` changes from `ZERO_SENTINEL` to a computed SHA-256 value. The specification is clear about how to handle pure-ZERO_SENTINEL batches and clear about how to handle pure-chained batches, but does not specify the behavior when both appear in the same 500-row page.

Concrete ambiguity: a batch arrives with rows [S1 (sentinel), S2 (sentinel), R1 (chained: `prev_hash = sha256(canonical_json(S2))`), R2 (chained: `prev_hash = sha256(canonical_json(R1))`)]. The state machine must:
1. Process S1: overwrite `last_computed_hash` with `sha256(canonical_json(S1))`.
2. Process S2: overwrite `last_computed_hash` with `sha256(canonical_json(S2))`.
3. Process R1: `R1.prev_hash` is `sha256(canonical_json(S2))`. Is R1 a "chained row" or a "sentinel row"? Its `prev_hash` is not `ZERO_SENTINEL` and not `""`. Verdict: R1 is a chained row. `verify_batch` should assert `R1.prev_hash == last_computed_hash` (which is `sha256(canonical_json(S2))`). This assertion passes. Update `last_computed_hash = sha256(canonical_json(R1))`.
4. Process R2: assert `R2.prev_hash == last_computed_hash` (which is `sha256(canonical_json(R1))`). This passes.

This is the correct behavior, but it requires that `verify_batch` applies the ZERO_SENTINEL branch only when `row.prev_hash == ZERO_SENTINEL`, and applies the chain-verification branch otherwise. The design does not state this dispatching rule. An implementer who reads "ZERO_SENTINEL rows are skipped" might interpret "skipped" as "any row up until the first non-sentinel row" — applying the sentinel branch conditionally based on an "have we seen a non-sentinel yet" flag rather than inspecting each row's `prev_hash`.

Under the "flag" interpretation: if sentinel rows appear in a batch before any chained rows, all are processed as sentinels. Once a chained row is seen, the flag is set and all subsequent rows (even if their `prev_hash` happens to be `ZERO_SENTINEL`) are processed as chained rows. Under the "per-row `prev_hash` inspection" interpretation: each row is classified individually based on whether its `prev_hash` is `ZERO_SENTINEL`, `""`, or a SHA-256 hex string. These two interpretations agree for ordered `audit_log` data (ZERO_SENTINEL rows always come before chained rows because the migration backfills existing rows and new rows get computed hashes). However, if an operator manually inserted a row with `prev_hash = ZERO_SENTINEL` after the migration (bypassing the immutability trigger, e.g., during a restore scenario), the two interpretations diverge.

**Consequence:** Two implementers may write `verify_batch` with different transition detection logic. In the ordered case (all ZERO_SENTINEL rows precede all chained rows), both implementations agree. In an out-of-order or tampered case (a ZERO_SENTINEL row appearing after chained rows), the flag-based implementation treats it as a chained row and may report a spurious `ChainBroken`; the per-row implementation treats it as a sentinel and silently skips the link assertion. The two different behaviors affect what `verify_batch` catches vs. ignores for edge-case data, and they produce different `last_computed_hash` values after the batch. If `last_computed_hash` diverges, the next batch's first chained row fails verification — a false positive chain break introduced by `verify_batch`'s own state inconsistency.

**Design assumption violated:** The design assumes that ZERO_SENTINEL rows always appear before chained rows in `ORDER BY rowid ASC` order, so the transition rule does not need to be made explicit. Migration 0006 ensures this for pre-existing rows, but a restore operation or a bug in a future phase could insert a row with `prev_hash = ZERO_SENTINEL` into a position after chained rows. Without an explicit per-row dispatching rule, two implementations diverge on these inputs.

**Suggested mitigation:** Add a dispatch rule to the `verify_batch` specification: "For each row in the batch, classify it by inspecting `row.prev_hash` directly: (1) if `row.prev_hash == ''` → return `Err(ChainError::EmptyHash { row_id })` immediately; (2) if `row.prev_hash == ZERO_SENTINEL` → compute `sha256(canonical_json(row))`, overwrite `last_computed_hash`, increment `sentinel_count`, continue; (3) otherwise → assert `row.prev_hash == last_computed_hash.as_deref().unwrap_or(ZERO_SENTINEL)`; if assertion fails, return `Err(ChainError::ChainBroken { row_id, expected: last_computed_hash.clone().unwrap_or(ZERO_SENTINEL.to_owned()), actual: row.prev_hash.clone() })`; update `last_computed_hash = sha256(canonical_json(row))`. This per-row dispatching rule is applied regardless of batch position or whether prior rows in the batch were sentinels." This eliminates the "flag-based" vs "per-row" ambiguity.

---

### LOW — `verify_batch` called with an empty slice is unspecified; the startup paginator may call it with an empty final page

**Category:** Logic flaw

**Trigger:** The design specifies: "`fn verify_batch(batch: &[AuditRow], state: &mut ChainVerifyState) -> Result<(), ChainError>` — processes one page of rows, updating `state`." The startup paginator calls `verify_batch(page, &mut state)?` per 500-row page.

In a paginated `SELECT ... ORDER BY rowid ASC LIMIT 500 OFFSET n` loop, the standard termination condition is: stop when the returned page has fewer than 500 rows (the last page). Most paginator implementations also call the page handler once with an empty result set when `OFFSET n` exactly equals the total row count. Whether the paginator should call `verify_batch` with a zero-length slice is not specified.

`verify_batch(&[], &mut state)` is a valid Rust call — `&[AuditRow]` accepts an empty slice. The current design does not say whether `verify_batch` should return `Ok(())` immediately on an empty batch (the obvious and correct behavior) or whether the caller is responsible for not calling it with an empty batch.

This is a minor concern: `verify_batch(&[])` with an immediate `Ok(())` return is the only sensible implementation. However, an implementer who adds a `debug_assert!(!batch.is_empty(), "verify_batch called with empty slice")` in their implementation (a not-unreasonable defensive assertion) will cause a debug-mode panic during startup on any database whose row count is an exact multiple of 500. This is unlikely but deterministic for specific database sizes.

**Consequence:** With the most defensive implementation of `verify_batch`, the startup guard panics during chain verification on databases with row counts that are exact multiples of 500. The panic is only in debug builds; release builds pass. This is a subtle differential behavior between debug and release modes that is not caught by any specified test (the test corpus does not specify a 500-row or 1000-row chain).

**Design assumption violated:** The design assumes that paginators are implemented to not call `verify_batch` with an empty slice, or that `verify_batch`'s behavior on an empty slice is self-evidently `Ok(())`. Neither is stated explicitly.

**Suggested mitigation:** Add one sentence to the `verify_batch` specification: "`verify_batch` called with an empty slice MUST return `Ok(())` immediately without modifying `state` — paginators that fetch an empty final page may safely call it." This is a one-line addition that removes the defensive-assertion hazard.

---

## No findings (categories with nothing concrete to raise)

- **Authentication & authorization** — `AuditWriter` is server-internal; no public bypass. No new vector.
- **Abuse cases** — 10 MB cap (BLOB-accurate, `COALESCE`-wrapped, both `redacted_diff_json` and `notes`), max 1000 rows, `AUDIT_QUERY_ROW_FIXED_OVERHEAD_BYTES` constant, `busy_timeout = 5000`, `occurred_at_ms > 0` guard before mutex lock. All paths fully specified. No new vector.
- **Data exposure** — `RedactedDiff` newtype with no public field and controlled constructors prevents plaintext exposure. `from_db_str` is `pub` with doc comment and companion grep recipe. No new data exposure vector.
- **Race conditions** — `tokio::sync::Mutex` (now explicitly mandated on line 64) + `BEGIN IMMEDIATE` serialises all writes. Concurrent-write chain linearisation tested (ten writes × ten repetitions, `multi_thread` flavor). No new race vector.
- **State manipulation** — ZERO_SENTINEL / `""` / computed-hash three-way classification is specified (with the caveat in R21-M2 about per-row dispatching, which is a specification gap not a missing invariant). `SecretsRevealed` guard is step 1, `InvalidTimestamp` guard is step 1b, both before mutex lock.
- **Resource exhaustion** — Stateful batch API (500 rows/page) bounded memory; no full-log preload. 10 MB query cap. `busy_timeout`. No new exhaustion vector.
- **Single points of failure** — Connection recovery (R5-F402, R6-F505), `busy_timeout`, `PRAGMA foreign_keys = ON` on initial and recovery opens, `ConnectionRecoveryFailed` surfacing both errors. No new SPOF.
- **Timeouts & retries** — `busy_timeout = 5000` + `BusyTimeout` return; test (h) verifies ~6 s bound. No retry loop. No new hazard.
- **Rollbacks** — Audit writes are out-of-band from business transactions by design. No rollback semantics for audit rows.
- **Orphaned data** — Immutability triggers prevent cleanup by design. No orphan accumulation path.
- **`tokio::sync::Mutex` closure (R20-M1)** — Line 64 now explicitly mandates `tokio::sync::Mutex<Option<SqliteConnection>>` and explicitly prohibits `std::sync::Mutex`, explaining why (`!Send` guard, `E0277`, latent deadlock). The concurrent-writes test (`multi_thread` flavor) is structurally impossible to compile with `std::sync::Mutex`. CLOSED.
- **Stateful batch API closure (R20-M2)** — `ChainVerifyState`, `verify_batch`, and `verify_finish` are now specified on line 45. The calling convention (per 500-row page, then `verify_finish` after last page) is specified for the startup paginator. The R21-M1 and R21-M2 findings identify remaining specification gaps within the batch API itself. CLOSED for the structural sync/async bridge problem; residual specification gaps raised as R21-M1 and R21-M2.
- **strum Cargo spec (R20-L1)** — Line 15 now reads "Do NOT add `#[derive(strum::Display)]`" with no ambiguous parenthetical offering it as an option. The feature `"derive"` covers `EnumString`, `EnumIter`, and `EnumCount` without listing `Display`. CLOSED.
- **`canonical_json` key names and encoding** — All 17 keys specified (including `"redacted_diff_json"` not `"redacted_diff"`), `occurred_at` computed from `occurred_at_ms / 1000`, JSON null for None, sorted keys, no whitespace. Stability test (criterion h) mandated with specific field-level assertions. No new gap.
- **`AuditEvent::Unknown` Display** — Manual `impl fmt::Display for AuditEvent` mandated on line 14; `strum::Display` derive explicitly prohibited. Targeted test for `Unknown("tool.future-op").to_string() == "tool.future-op"` mandated on line 15. No new gap.
- **`Actor::from_kind_id` fallback sentinel** — `"__unknown:{kind}"` sentinel specified identically for `record` step 5 (line 64) and startup paginator (line 45, and line 217 design decision). Both call sites confirmed consistent. No new gap.
- **`AuditOutcome` serde encoding** — `#[serde(rename_all = "lowercase")]` on both `Serialize` and `Deserialize`; stability test asserts lowercase output. No new gap.
- **Migration 0006 sequencing** — ALTER TABLE → backfill UPDATE → CREATE TRIGGER × 2; backfill before immutability triggers; atomic via sqlx transaction. No new gap.
- **`record` step 5 named column projection** — `SELECT *` now explicitly prohibited; named projection excluding `occurred_at` mandated (line 64). Consistent with startup paginator fix (line 45). No new gap.
- **`prev_hash` write-time binding** — Step 7 binds `new_prev_hash`, not `row.prev_hash`. Line 197 records this decision. No new gap.
- **Eventual consistency** — Single-process SQLite; no multi-store gap.

---

## Summary

**Critical:** 0  **High:** 0  **Medium:** 2  **Low:** 1

**Top concern:** R21-M1 — `verify_finish` is specified only as "validates the terminal state (returns `Ok(())` for a chain that has not yet started)" with no other stated invariant. Two implementers can write equally-compliant functions that return different values for the same state. Because the startup paginator calls `verify_finish(&state)?`, a `verify_finish` that adds an undocumented non-trivial check causes spurious startup chain alarms or blocks. This is a one-sentence fix.

**Recommended action before proceeding:** Address R21-M1 (specify `verify_finish`'s complete contract — currently "always `Ok(())`" since all link validation happens in `verify_batch` — and add two targeted tests to confirm this) and R21-M2 (specify the per-row `prev_hash` dispatching rule for `verify_batch` to eliminate the "flag-based" vs "per-row" implementation ambiguity in mixed batches). R21-L1 (empty-slice behavior of `verify_batch`) is a one-line addition that removes a defensive-assertion hazard. All three are textual design changes with no implementation cost.

---

Not yet ready — 2 blocker(s) remain (R21-M1, R21-M2).
