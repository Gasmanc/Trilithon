# Adversarial Review — Phase 6 — Round 20

**Design summary:** Phase 6 adds a tamper-evident, append-only audit log to Trilithon: an `AuditWriter` with a dedicated serialised SQLite connection, a SHA-256 `prev_hash` chain, a `RedactedDiff` newtype for secrets-safe diffs, an `Actor` enum with `from_kind_id` reverse-mapping and an unknown-kind fallback, startup integrity checks, and a query API with a 10 MB soft cap.

**Prior rounds:** Rounds 1–19 reviewed. All R19 findings are closed in the current design text:
- R19-H1 (`#[derive(strum::Display)]` and manual `impl fmt::Display` cannot coexist; vocabulary task mandated both): CLOSED — line 14 now reads "**Do NOT derive `strum::Display`; write a single manual `impl fmt::Display for AuditEvent`**" covering all 44 named variants and the `Unknown(s)` arm; line 223 confirms this with the R19-H1 tag; the impossible "hybrid" description is replaced by the clear unambiguous instruction.
- R19-M1 (`SELECT *` in `record` step 5 faces the same `occurred_at` sqlx mapping problem as the startup paginator, but only the paginator task mentioned the fix): CLOSED — line 64 (step 5) now reads "query with a **named column projection** listing all `AuditRow` fields... do NOT use `SELECT *`; the `occurred_at` column is intentionally excluded"; line 225 records the R19-M1 design decision.

No open items carried forward from prior rounds.

---

## Round 19 Closure Table

| ID | Severity | Title | Status |
|----|----------|-------|--------|
| R19-H1 | HIGH | Vocabulary task mandated both `#[derive(strum::Display)]` and a manual `impl fmt::Display`; compile error E0119 guaranteed | CLOSED — line 14 now mandates manual `Display` only; `strum::Display` derive explicitly prohibited |
| R19-M1 | MEDIUM | `record` step 5 used `SELECT *`; sqlx `FromRow` would fail on `occurred_at` column with no matching `AuditRow` field | CLOSED — line 64 now mandates named column projection excluding `occurred_at` |

---

## Findings

### MEDIUM — `Mutex<Option<SqliteConnection>>` type unspecified; `std::sync::Mutex` cannot be held across `.await` points in a Tokio runtime

**Category:** Logic flaw / composition failure

**Trigger:** The `AuditWriter` task (line 64) specifies that `AuditWriter` MUST hold a `Mutex<Option<SqliteConnection>>` and that `record` is `async fn record(&self, row: AuditRow) -> Result<(), adapters::Error>`. Inside `record`, the mutex is locked in step 2, and subsequent steps perform asynchronous database operations: `BEGIN IMMEDIATE` (step 4), `SELECT` (step 5), `INSERT` (step 7), and `COMMIT` (step 8) — all of which are `.await` points. The design never specifies which `Mutex` type to use: `std::sync::Mutex` or `tokio::sync::Mutex`.

Concrete failure sequence: an implementer writes `AuditWriter { conn: std::sync::Mutex<Option<SqliteConnection>> }`. Inside `record`, they call `let guard = self.conn.lock().unwrap()`. The `std::sync::MutexGuard` is `!Send` (it cannot be moved across threads). Tokio's multi-thread scheduler may move the future to a different thread between `.await` points. The Rust compiler rejects the async function with a compile error: `error[E0277]: 'MutexGuard<_>' cannot be sent between threads safely` — because `std::sync::MutexGuard` appears in the future's state machine across an `.await` point and the future must be `Send` to be spawned on a multi-thread Tokio executor.

An implementer encountering this error may work around it by wrapping the entire async section in `tokio::task::block_in_place` or `std::sync::Mutex::lock` in a non-async block — but these workarounds change the concurrency model in subtle ways. Alternatively, an implementer running a single-threaded Tokio runtime (`#[tokio::main(flavor = "current_thread")]`) will not see the compile error because the future does not need to be `Send`, but the `std::sync::Mutex` will then cause a deadlock if the future is cancelled while the lock is held and another `record` call attempts to acquire the lock on the same thread (Tokio does not preempt; the stuck lock holder never yields). A third path: an implementer who uses `std::sync::Mutex` and wraps `.await` calls inside `tokio::task::spawn_blocking` introduces a blocking call on a Tokio worker thread, which Tokio's documentation explicitly warns against.

The correct type for holding a lock across `.await` in async Rust is `tokio::sync::Mutex`, which is designed for this pattern. Without specifying this, implementers will arrive at `std::sync::Mutex` by default (it's the standard library type) and hit either a compile error (multi-thread executor, correct) or a latent deadlock (current-thread executor, not caught by tests).

**Consequence:** With a multi-thread Tokio executor (which is the default for `#[tokio::main]` and for `tokio::test` with `flavor = "multi_thread"`): the code does not compile, and the concurrent-writes test (ten `record` calls, `multi_thread` flavor) does not build. With a current-thread executor: the code compiles but is susceptible to deadlock if a future holding the `std::sync::Mutex` guard is dropped while yielding — the mutex is not released until the `Drop` on the guard runs, which cannot happen if the thread is not scheduled back. In a `current_thread` test, cancellation of a task while it holds the guard stalls all subsequent `record` calls permanently.

**Design assumption violated:** The design assumes that `Mutex<Option<SqliteConnection>>` is self-evidently the async-safe Tokio mutex. In Rust, `std::sync::Mutex` is the first result for any crate author who writes `Mutex` — and it is incompatible with `.await` across lock boundaries in a multi-thread Tokio runtime. The design must name the type explicitly.

**Suggested mitigation:** Amend the `AuditWriter` task (line 64) to read: "`AuditWriter` MUST hold a `tokio::sync::Mutex<Option<SqliteConnection>>` — **not** `std::sync::Mutex`, which cannot be held across `.await` points in a multi-thread Tokio executor and causes `error[E0277]` at compile time." Add `tokio` as an explicit `Mutex` source to the implementation note, and add to the "Done when" criteria: "the concurrent-writes test (`tokio::test(flavor = 'multi_thread')` × 10) compiles and passes — this test is structurally impossible to compile if `std::sync::Mutex` is used because its guard is `!Send`." The sign-off checklist should include: "`AuditWriter.conn` is `tokio::sync::Mutex<Option<SqliteConnection>>`; no `std::sync::Mutex` wraps the connection."

---

### MEDIUM — `chain::verify` is a synchronous `Iterator`-taking function but the startup paginator that feeds it must be async; no bridge is specified

**Category:** Composition failure / logic flaw

**Trigger:** The `chain::verify` specification (line 45) defines: `chain::verify(rows: impl Iterator<Item = &AuditRow>) -> Result<(), ChainError>`. The function is synchronous — it takes a synchronous iterator and is a non-async function. The implementation note explicitly says "walk the iterator incrementally — no pre-loading all rows in memory."

The startup guard (line 82) specifies that `chain::verify` is called in a paginated fashion: "paginated batch 500 `ORDER BY rowid ASC`." The paginator must fetch rows from SQLite — an I/O operation that, in the adapters crate using sqlx, is inherently async. The startup guard lives in the `adapters` crate, which uses an async runtime. The paginator makes async database calls and must convert the resulting rows into the `impl Iterator<Item = &AuditRow>` expected by `chain::verify`.

Concrete failure sequence: an implementer must call `chain::verify` from inside an async startup function. They have two structurally available paths:

Path A — collect all rows into a `Vec<AuditRow>` first, then pass `vec.iter()` to `chain::verify`: this preloads the entire audit log into memory. For a production system with millions of rows, this exhausts heap. The design says "no pre-loading all rows in memory" — but this requirement is stated for the `chain::verify` implementation, not the caller. An implementer who reads "no pre-loading all rows" as a constraint on `chain::verify`'s internal behavior (which it nominally is) will nonetheless be forced to preload at the call site to bridge the async/sync boundary. The spirit of the requirement is defeated.

Path B — construct a lazy iterator that makes blocking database calls when `next()` is called, then pass it to `chain::verify`: this requires either (a) running SQLite queries in a `spawn_blocking` context inside the iterator's `next()` implementation — deeply non-idiomatic and not expressible as a standard `Iterator`; or (b) using `tokio::task::block_in_place` to make the async paginator synchronous while `chain::verify` drives the iterator — which blocks the Tokio worker thread for the duration of the entire chain verification, during which no other tasks can run on that thread.

Path C — make `chain::verify` an async function that accepts an async stream: this violates the three-layer architecture rule ("no I/O in core") because the function would need to be async to accept a `Stream`. The design explicitly places `chain::verify` in `crates/core/src/audit.rs`, where async runtime use is forbidden.

None of the three paths satisfies both "no preloading" and "no I/O in core" and "async I/O in the paginator" simultaneously within a standard idiomatic Rust implementation. The design does not specify which bridge pattern to use.

**Consequence:** An implementer will independently choose Path A (preload all rows), which is the path of least resistance and produces correct behavior but defeats the memory-efficiency goal. For a deployment with 10 million audit rows averaging 500 bytes each in the in-memory `AuditRow` representation, Path A loads ~5 GB into RAM at startup — a silent operational hazard not caught by any specified test (the concurrent-writes test uses far fewer rows). The "paginated batch 500" language in the startup guard implies the intent is streaming, but without a specified bridge, every implementation will preload.

**Design assumption violated:** The design assumes that a synchronous `impl Iterator<Item = &AuditRow>` can be constructed lazily from async paginated SQL queries without either preloading all rows or blocking a Tokio worker thread. This is not achievable in idiomatic async Rust without explicit bridging infrastructure (e.g., a channel-fed iterator backed by a spawned async task). The design specifies the desired behavior at both ends (async paginator, synchronous iterator-driven `chain::verify`) but provides no specified bridge between them.

**Suggested mitigation:** Choose one of two explicit approaches and state it in the `chain::verify` and startup guard tasks:

Option A (recommended — bounded memory, correct architecture): Change the signature to `chain::verify(rows: &[AuditRow]) -> Result<(), ChainError>` and accept that each page is a `&[AuditRow]` slice. Add a stateful accumulator: `chain::verify_batch(batch: &[AuditRow], state: &mut ChainVerifyState) -> Result<(), ChainError>` where `ChainVerifyState` carries `last_computed_hash` and `row_count`. The startup paginator calls `verify_batch` once per 500-row page, accumulating state. This keeps per-batch allocations bounded at 500 rows, is async-safe because the paginator is async and the verify call is synchronous on each batch, and requires no cross-crate async infrastructure. Define `ChainVerifyState::new() -> Self` and `ChainVerifyState::finish() -> Result<(), ChainError>` (checks for any unresolved state after all batches). Document the calling convention in the startup guard task.

Option B (simpler, acceptable for small logs): State explicitly that the startup paginator MUST collect all rows into a `Vec<AuditRow>` (using paginated `SELECT ... LIMIT 500 OFFSET n` to avoid a single unbounded DB query) before calling `chain::verify(collected.iter())`. Add a documented limit: "For audit logs exceeding 1 million rows, startup verification may cause elevated RAM usage; operators with large logs should schedule chain verification as a background task rather than a startup gate." This makes the preloading explicit and intentional, removing the gap between the spec and the only practical implementation.

Either way, remove the phrase "no pre-loading all rows in memory" from the `chain::verify` specification if the batched-accumulator approach is chosen — the constraint applies to the implementation of the state machine traversal, not to the caller's page-feeding loop.

---

### LOW — `strum` feature list in the "Done when" criteria still includes `strum::Display` as an option despite line 14 now prohibiting `#[derive(strum::Display)]`

**Category:** Documentation trap

**Trigger:** After R19-H1 was closed by amending line 14 to read "**Do NOT derive `strum::Display`**", the "Done when" criteria on line 15 were not updated consistently. Line 15 still reads: "Add `strum::EnumString`, `strum::EnumIter`, `strum::EnumCount`, and `strum::Display` (**or a manual `Display` impl**) to the `strum` feature list in `core/Cargo.toml`."

The parenthetical "or a manual `Display` impl" now reads as an alternative — as if the implementer may choose either the strum `Display` feature or the manual impl. But line 14 has explicitly mandated the manual impl and prohibited the derive. An implementer reading line 15's Cargo.toml feature list instruction sees `strum::Display` listed as a feature to enable and the parenthetical as a choice — not as a note that the derive is forbidden. They may add `#[derive(strum::Display)]` to `AuditEvent` (it's a feature they just enabled, after all), producing the compile error E0119 that R19-H1 was raised to prevent, this time from reading line 15 rather than line 14's earlier state.

**Consequence:** An implementer who reads line 15 before line 14 (or who reads line 15's Cargo.toml feature spec in isolation when adding the Cargo.toml entry) may add the `Display` strum feature and the derive, then later encounter the manual `Display` impl requirement from line 14, and not realize the derive is the problem — because line 15 explicitly told them to add the `Display` feature. The resulting compile error E0119 is the same failure mode as R19-H1, now triggered from the Cargo.toml feature spec rather than the acceptance criterion wording.

**Design assumption violated:** The design assumes that line 14's prohibition on `#[derive(strum::Display)]` is authoritative and that line 15's feature list is merely a Cargo.toml implementation detail. An implementer following both instructions in order will see a contradiction when line 15 says to add the `Display` feature and the parenthetical offers it as an alternative to the manual impl.

**Suggested mitigation:** Amend the Cargo.toml feature list on line 15 to read: "Add `strum = { version = '…', features = ['derive'] }` to `core/Cargo.toml`. The `derive` feature enables all strum derive macros (`EnumCount`, `EnumString`, `EnumIter`). **Do NOT add `strum::Display` as a feature or derive** — the `Display` impl is manual (see above). Listing `strum::Display` in the feature list is harmless but may mislead an implementer into adding `#[derive(strum::Display)]`, which is prohibited." This replaces the misleading parenthetical and makes the Cargo.toml instruction unambiguous.

---

## No findings (categories with nothing concrete to raise)

- **Authentication & authorization** — `AuditWriter` is server-internal with no public bypass. No new vector.
- **Abuse cases** — 10 MB cap (BLOB-accurate, both `redacted_diff_json` and `notes`), max 1000 rows, named constant `AUDIT_QUERY_ROW_FIXED_OVERHEAD_BYTES`, `busy_timeout = 5000`, `occurred_at_ms > 0` guard before mutex lock. All paths specified. No new vector.
- **Race conditions** — `tokio::sync::Mutex` + `BEGIN IMMEDIATE` (once the Mutex type finding is addressed) serialises all writes. Concurrent-write chain linearisation tested (ten calls × ten repetitions). No new gap.
- **Resource exhaustion** — `chain::verify` paginated (though the bridge is unspecified — see R20-M2), 10 MB query cap, `busy_timeout`. No new exhaustion vector beyond the heap issue already described in R20-M2.
- **State machine violations** — Migration 0006 step order (ALTER TABLE → backfill UPDATE → CREATE TRIGGER × 2) is correctly sequenced: backfill before triggers. `SecretsRevealed` guard is step 1 (before mutex). `InvalidTimestamp` guard is step 1b (before mutex). No new violation.
- **Single points of failure** — Connection recovery (R5-F402, R6-F505), `busy_timeout`, `PRAGMA foreign_keys = ON` on initial and recovery opens, `ConnectionRecoveryFailed` surfacing both errors. No new SPOF.
- **Timeouts & retries** — `busy_timeout = 5000` + `BusyTimeout` error; test (h) verifies the ~6 s bound. No retry loop. No new hazard.
- **Rollbacks** — Audit writes are out-of-band from business transactions by design. No rollback semantics.
- **Orphaned data** — Immutability triggers prevent cleanup by design. No orphan accumulation path.
- **`AuditEvent::Unknown` Display correctness** — Line 14 now unambiguously mandates a single manual `Display` impl with an `Unknown(s) => write!(f, "{}", s)` arm. Line 15 mandates a targeted test (`Unknown("tool.future-op").to_string() == "tool.future-op"`). The R19-H1 compile contradiction is closed.
- **`canonical_json` stability** — All 17 key names specified (including `"redacted_diff_json"` not `"redacted_diff"`), `occurred_at` computed inside from `occurred_at_ms / 1000`, JSON null for None, sorted keys, no whitespace. Stability test (criterion h) specified with specific field assertions. No new gap.
- **`chain::verify` early-return** — R9-F903 specifies early return on first break; no re-scanning past the break; tested via the 3-row chain break test (criterion i). No new gap.
- **ZERO_SENTINEL tamper-detection bound** — R17-M1 documented that only the last sentinel row is cryptographically protected; `core/README.md` documentation requirement present. No new gap.
- **`AuditOutcome` serde encoding** — `#[serde(rename_all = "lowercase")]` on both `Serialize` and `Deserialize` ensures lowercase `"ok"`, `"error"`, `"denied"`. Stability test criterion (h) asserts this. No new gap.
- **`PRAGMA foreign_keys = ON`** — R17-L1 mandated on both initial open and recovery reopens. No new gap.
- **`Actor::Bootstrap` non-null actor_id** — Maps to `("system", "bootstrap")`; all variants produce non-null, non-empty `actor_id`. No new gap.
- **`prev_hash` write-time binding** — Step 7 binds `new_prev_hash`, not `row.prev_hash`; line 197 records the R12-H1 decision. No new gap.
- **Migration hazards** — `0006` is atomic (sqlx wraps each migration in a transaction); backfill before trigger creation; `0001` schema provides the base columns. No gap.
- **Eventual consistency** — Single-process SQLite; no multi-store gap.

---

## Summary

**Critical:** 0  **High:** 0  **Medium:** 2  **Low:** 1

**Top concern:** R20-M1 — The `Mutex<Option<SqliteConnection>>` type is unspecified. Using `std::sync::Mutex` with `.await` inside the lock causes a compile error (`error[E0277]`) on the multi-thread Tokio executor, which is both the default `#[tokio::main]` flavor and the executor used by the mandatory concurrent-writes test (`tokio::test(flavor = "multi_thread")`). An implementer who reaches this compile error and works around it (rather than switching to `tokio::sync::Mutex`) may introduce a latent deadlock or thread-blocking behavior. The fix is a one-line specification change.

**Recommended action before proceeding:** Address R20-M1 (specify `tokio::sync::Mutex` explicitly in the `AuditWriter` task) and R20-M2 (choose either the batched-accumulator approach or explicit preloading and state it unambiguously in the `chain::verify` and startup guard tasks; remove the contradictory "no pre-loading all rows in memory" language if preloading per batch is the chosen implementation). R20-L1 (Cargo.toml feature list still offers `strum::Display` as an option) is a documentation cleanup that avoids re-triggering the R19-H1 implementation confusion. All three are textual design changes with no implementation cost.
