# Adversarial Review — Phase 6 — Round 22

**Design summary:** Phase 6 adds a tamper-evident, append-only audit log to Trilithon: an `AuditWriter` with a dedicated serialised `tokio::sync::Mutex<Option<SqliteConnection>>`, a SHA-256 `prev_hash` chain verified via a stateful batch API (`ChainVerifyState` / `verify_batch` / `verify_finish`), a `RedactedDiff` newtype for secrets-safe diffs, an `Actor` enum with `from_kind_id` reverse-mapping and unknown-kind fallback, startup integrity checks, and a query API with a 10 MB soft cap.

**Prior rounds:** Rounds 1–21 reviewed. All R21 findings were addressed before this round:

- R21-M1 (`verify_finish` semantics underspecified — two valid implementations possible that return different values): CLOSED — line 45 now reads "`verify_finish` — **always returns `Ok(())`**; all link validation is performed incrementally inside `verify_batch` which returns early on the first broken link. Implementations MUST NOT add additional error conditions to `verify_finish` until the design is updated." Tests (k) and (l) in the "Done when" criteria are added to confirm this for a completed chain and an empty log.
- R21-M2 (`verify_batch` mixed-batch transition semantics underspecified — "flag-based" vs "per-row" ambiguity): CLOSED — the design now contains an explicit per-row dispatching rule: "(1) `row.prev_hash == ""` → EmptyHash; (2) `row.prev_hash == ZERO_SENTINEL` → overwrite hash, increment `sentinel_count`, continue; (3) otherwise → assert against `last_computed_hash.as_deref().unwrap_or(ZERO_SENTINEL)`, update hash." Applied per-row regardless of batch position.
- R21-L1 (`verify_batch` empty-slice behavior unspecified — defensive `debug_assert!(!batch.is_empty())` hazard): CLOSED — design now reads "`verify_batch` called with an empty slice MUST return `Ok(())` immediately without modifying `state`" and explicitly prohibits `debug_assert!(!batch.is_empty())`. Test (j) mandates this.

No items are carried forward from prior rounds.

---

## Findings

### MEDIUM — `sentinel_count` in `ChainVerifyState` is specified to be incremented but never read, never tested, and no caller ever observes its value — the field is unverifiable dead state that will silently corrupt a future `verify_finish` extension

**Category:** Logic flaw

**Trigger:** The `chain::verify` task (line 45) specifies `struct ChainVerifyState { last_computed_hash: Option<String>, sentinel_count: u64 }`. The per-row dispatching rule says: for ZERO_SENTINEL rows, "compute `sha256(canonical_json(row))`, overwrite `last_computed_hash`, **increment `sentinel_count`**, continue to next row." `verify_finish` always returns `Ok(())` and takes `state: &ChainVerifyState` but does not read any of its fields. No other code path in the design reads `state.sentinel_count`.

The design also contains this note on `verify_finish`: it "exists as a future extension point (e.g., for a future sentinel-count floor check)." This explicitly anticipates a future reader of `sentinel_count`. But no test in the "Done when" criteria ever asserts `state.sentinel_count == N` for any N.

Concrete failure sequence: Implementer A reads the spec and implements the ZERO_SENTINEL branch correctly — `sentinel_count += 1` per sentinel row, `last_computed_hash` updated. Implementer B, noticing `sentinel_count` is never read, never tested, and never logged, concludes it is vestigial boilerplate and initialises it but never increments it (or increments a local variable instead of the struct field). Both implementations pass all specified tests because no test checks `state.sentinel_count`. When a future phase adds `verify_finish` logic that reads `sentinel_count` (the stated extension point), Implementer B's code silently returns wrong results — `sentinel_count` is 0 for a DB with 50,000 pre-migration rows, and any floor-check that should catch "fewer than expected sentinel rows" becomes vacuous.

The connection to `tracing::info!` adds to the ambiguity: line 45 says ZERO_SENTINEL rows should emit "`tracing::info!` for count of skipped rows." The design does not say whether this log fires once per sentinel row (inside the loop) or once at the end of the batch (using `sentinel_count` from state). If the intent is one log per row, `sentinel_count` is unnecessary for the log and its purpose is purely the future extension. If the intent is one summary log per batch, `sentinel_count` is needed — but neither the log timing nor the use of `sentinel_count` vs. a local counter is specified.

**Consequence:** `sentinel_count` accumulates silently without any observable effect on the current design. Any future extension that reads it will receive a value that was never verified to be correct. The two possible mis-implementations (never incrementing, or incrementing a local variable) pass all current tests and only fail when the field is actually used — at which point the root cause is invisible.

**Design assumption violated:** The design assumes that specifying `sentinel_count += 1` in the ZERO_SENTINEL branch is sufficient to ensure implementers track the field correctly. Without a test that asserts `state.sentinel_count == expected_N` for a batch containing N ZERO_SENTINEL rows, there is no signal that the increment is being executed.

**Suggested mitigation:** Add one assertion to the "Done when" criteria for the `verify_batch` test that already exercises ZERO_SENTINEL rows: "(c) ZERO_SENTINEL rows skipped (but hashed) — **this test MUST additionally assert `state.sentinel_count == N` where N is the number of ZERO_SENTINEL rows in the input slice**." This makes `sentinel_count` observable and forces implementers to actually increment the struct field rather than a local variable. Optionally, specify that the `tracing::info!` fires once at the end of each batch using `sentinel_count` (not per-row), which makes the field doubly purposeful.

---

### LOW — Test (j) "does not modify state" is unverifiable as written — the criterion does not specify which state fields must remain unchanged

**Category:** Logic flaw

**Trigger:** Test (j) in the "Done when" criteria for `verify_batch` (line 46) reads: "`verify_batch` called with an empty slice returns `Ok(())` and **does not modify state**." The test is structurally ambiguous: "does not modify state" has no definition in the design — an implementer does not know which fields of `ChainVerifyState` must remain equal to their initial values.

Concrete problem: the empty-slice test passes if and only if the test author asserts specific field invariants. An implementer who writes `fn verify_batch(batch: &[AuditRow], state: &mut ChainVerifyState) -> Result<(), ChainError> { if batch.is_empty() { state.last_computed_hash = None; return Ok(()); } … }` incorrectly resets `last_computed_hash` to `None` on empty-slice calls. This destroys accumulated `last_computed_hash` from prior batches. If the test only checks the return value (`assert_eq!(result, Ok(()))`) and not the state fields, the implementation passes test (j) while introducing a latent bug: any subsequent batch processed after an empty-page call would behave as if no prior rows existed.

The multi-batch paginator scenario: pages 1–N are full (500 rows), page N+1 is empty (exact multiple of 500). The paginator calls `verify_batch(page_N+1, &mut state)`. A buggy implementation resets `state.last_computed_hash = None`. The paginator then calls `verify_finish(&state)?` which always returns `Ok(())`. No error is detected. But if there were a page N+2 (which in this scenario there isn't), the next batch would compare against `None.as_deref().unwrap_or(ZERO_SENTINEL)` = ZERO_SENTINEL, causing a spurious `ChainBroken`.

In the exact-multiple-of-500 scenario there is no N+2, so the bug is invisible in normal operation but the state corruption is real and could mask bugs in refactored or extended code.

**Consequence:** The empty-slice test can be vacuously satisfied by a test that only checks the `Ok(())` return value without asserting field invariants. An implementation that corrupts `last_computed_hash` on empty-slice calls passes all specified tests.

**Design assumption violated:** The design assumes "does not modify state" is a precise behavioral specification that implementers and test authors will interpret consistently. The phrase has no operative definition in the design.

**Suggested mitigation:** Replace "(j) `verify_batch` called with an empty slice returns `Ok(())` and does not modify state" with the operationally precise form: "(j) `verify_batch` called with an empty slice returns `Ok(())` without modifying state — tested by: calling `verify_batch` on a 3-row chain (setting up `last_computed_hash` to a known value), then calling `verify_batch` with an empty slice, then asserting that `state.last_computed_hash` still holds the same value from after the 3-row call." This forces the test to actually check state persistence.

---

### LOW — `RedactedDiff` reconstruction in `record` step 5 uses sqlx `FromRow` on a custom newtype; no mechanism is specified for mapping `Option<String>` columns to `Option<RedactedDiff>` — implementers face an unguided compile error

**Category:** Logic flaw

**Trigger:** `record` step 5 queries the predecessor row using a named column projection that includes `redacted_diff_json`. `AuditRow.redacted_diff` is typed `Option<RedactedDiff>`. `RedactedDiff` is a newtype `pub struct RedactedDiff(String)` with no `From<String>`, no public field, and only two constructors: `RedactedDiff::new(...)` (for write path) and `RedactedDiff::from_db_str(s: String)` (for read path).

If an implementer uses `#[derive(sqlx::FromRow)]` on `AuditRow` and runs the named-projection query, the sqlx macro will attempt to map the `TEXT` column `redacted_diff_json` to `Option<RedactedDiff>`. `sqlx::FromRow` can map `TEXT` to `Option<String>` or any type that implements `sqlx::Type<Sqlite>`. `RedactedDiff` does not implement `sqlx::Type<Sqlite>` (the design never specifies this impl). The result is a compile error in offline mode or a runtime decode error in online mode.

The design acknowledges `from_db_str` is needed "by `record` step 5 and the startup paginator when reconstructing a predecessor `AuditRow` from an already-stored DB string" but does not specify the mechanism: whether the implementer should (a) use an intermediate struct `PredecessorRow` with `redacted_diff_json: Option<String>` and convert manually, (b) implement `sqlx::Type<Sqlite>` for `RedactedDiff`, or (c) use a manual `FromRow` impl on `AuditRow`. Each approach has different tradeoffs and divergent implementations across two callers (step 5 and startup paginator) could produce structurally different code paths.

The startup paginator is in the same position: it must reconstruct `AuditRow` from rows with `redacted_diff_json: TEXT`, facing the same sqlx mapping problem.

**Consequence:** Two implementers write different reconstruction mechanisms. If one uses `sqlx::Type<Sqlite>` for `RedactedDiff` (which requires exposing the inner `String` indirectly) and another uses an intermediate struct, the intermediate-struct approach is more auditable (only one named call to `from_db_str`) but adds boilerplate. Neither is wrong, but the absence of guidance means code review cannot catch a deviation from the intended approach. More concretely: an implementer who adds `impl From<String> for RedactedDiff` (to satisfy `sqlx::Type`) violates the "No `From<String>`" invariant stated in the design.

**Design assumption violated:** The design assumes that specifying `from_db_str` as the reconstruction constructor is sufficient to guide implementers through the sqlx type-mapping problem. It is not: the gap between "call `from_db_str`" and "make sqlx actually compile with `Option<RedactedDiff>`" requires a design-level mechanism choice.

**Suggested mitigation:** Add one sentence to the `AuditWriter::record` step 5 and to the startup paginator task: "Use an intermediate `struct PredecessorRow` with `redacted_diff_json: Option<String>` (and all other columns as their primitive DB types) when running the sqlx query; then convert to `AuditRow` by calling `RedactedDiff::from_db_str` on the non-null value. Do NOT add `impl sqlx::Type<Sqlite> for RedactedDiff` or `impl From<String> for RedactedDiff` — both violate the opacity invariant." This specifies the mechanism, prevents the invariant-violating workaround, and gives both call sites identical guidance.

---

## No findings (categories with nothing concrete to raise)

- **Authentication & authorization** — `AuditWriter` is server-internal; all public surface is `record`. No new bypass vector.
- **Abuse cases** — 10 MB query cap (BLOB-accurate, COALESCE-wrapped, both columns), max 1000 rows, `AUDIT_QUERY_ROW_FIXED_OVERHEAD_BYTES` constant, `busy_timeout = 5000`, `occurred_at_ms > 0` guard before mutex lock. No new abuse vector.
- **Data exposure** — `RedactedDiff` newtype with controlled constructors, `from_db_str` is `pub` with doc comment and companion grep recipe, no `From<String>`. No new exposure vector.
- **Race conditions** — `tokio::sync::Mutex` + `BEGIN IMMEDIATE` serialises all writes. Concurrent chain test specified. No new race vector.
- **State manipulation** — ZERO_SENTINEL / `""` / computed-hash three-way per-row dispatch is fully specified. `SecretsRevealed` and `InvalidTimestamp` guards are pre-mutex. No new vector.
- **Resource exhaustion** — 500-row batch API bounds memory; no full-log preload; busy_timeout; 10 MB query cap. No new exhaustion vector.
- **Single points of failure** — Connection recovery (close + reopen), `ConnectionRecoveryFailed` surfacing both errors, `PRAGMA foreign_keys = ON` on recovery opens. No new SPOF.
- **Timeouts & retries** — `busy_timeout = 5000` + `BusyTimeout` return; test (h) verifies ~6 s bound. No retry amplification.
- **Eventual consistency** — Single-process SQLite; no multi-store gap.
- **Rollbacks** — Audit writes are out-of-band from business transactions by design; immutability is by DB trigger. No rollback semantics for audit rows.
- **Orphaned data** — Immutability triggers prevent cleanup by design. No accumulation path during normal operation.
- **Rate limits** — `busy_timeout` + bounded query page sizes cover the query path. No new gap.
- **R21 findings verification** — All three R21 findings (M1, M2, L1) are genuinely closed. The `verify_finish` contract is now explicit ("always returns `Ok(())`"; "Implementations MUST NOT add additional error conditions"); per-row dispatch is explicit and eliminates the flag-based interpretation; empty-slice behavior is mandated and the `debug_assert!` prohibition is present. Tests (j), (k), (l) are added to the "Done when" criteria.
- **`unwrap_or(ZERO_SENTINEL)` composition** — For case (3) in the per-row dispatch rule, `last_computed_hash.as_deref().unwrap_or(ZERO_SENTINEL)` is sound under the design's constraints. The first row written by `AuditWriter::record` on an empty table always has `prev_hash = ZERO_SENTINEL` (step 5: "if the table is empty, `new_prev_hash = ZERO_SENTINEL"`), so it is classified by case (2), not case (3). No legitimate first-chained-row has `prev_hash` that is not `ZERO_SENTINEL` — and if it did (via a bypass of the private write path), case (3)'s comparison against `ZERO_SENTINEL` would correctly produce `ChainBroken`. The fallback produces a true positive detection, not a false pass.
- **`verify_batch`/`verify_finish` tests (k) and (l) for `verify_finish` coverage** — Tests (k) and (l) are not vacuous for `verify_finish` specifically: (k) exercises `verify_finish` after a completed non-empty chain, (l) exercises it after zero rows. Both require `Ok(())`. Since `verify_finish` is now specified to always return `Ok(())` with no error conditions, any implementation that adds an error condition fails both tests. Genuinely closed.

---

## Summary

**Critical:** 0  **High:** 0  **Medium:** 1  **Low:** 2

**Top concern:** R22-M1 — `sentinel_count` in `ChainVerifyState` is specified to be incremented in the ZERO_SENTINEL branch but is never read by any code path, never checked by any test, and never logged with an explicit reference to the struct field. An implementer can omit the increment entirely and pass all specified tests. When a future phase adds the anticipated "sentinel-count floor check" to `verify_finish`, they will silently receive incorrect results from any implementation that skipped the increment. A one-line addition to an existing test criterion — asserting `state.sentinel_count == N` — closes this gap.

**Recommended action before proceeding:** Address R22-M1 (add a `sentinel_count` assertion to an existing ZERO_SENTINEL test criterion) and R22-L2 (specify the intermediate-struct pattern for sqlx reconstruction of `RedactedDiff` in step 5 and the startup paginator). R22-L1 (imprecise "does not modify state" test criterion for empty-slice) is a one-sentence clarification. All three are textual design changes with no implementation cost.

---

Ready — 0 blockers.
