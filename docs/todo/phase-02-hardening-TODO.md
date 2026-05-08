# Phase 02 Hardening — Implementation Slices

> Source: [`../adversarial/phase-2-decision.md`](../adversarial/phase-2-decision.md) — retro adversarial triage of phase 2.
> Architecture: [`../architecture/architecture.md`](../architecture/architecture.md), [`../architecture/trait-signatures.md`](../architecture/trait-signatures.md)
> ADRs: ADR-0006 (startup integrity check), ADR-0009 (audit hash chain).

This phase closes the 8 real risks identified in the phase-2 retro adversarial triage. Each slice is self-contained and corresponds 1:1 to one hardening item (H-1 … H-8) in the decision doc.

## Inputs the implementer must have in context

- This file.
- [`../adversarial/phase-2-decision.md`](../adversarial/phase-2-decision.md) (the triage and rationale for each item).
- The current phase 2 code: `core/crates/core/src/storage/`, `core/crates/adapters/src/`, `core/crates/cli/src/run.rs`, and `core/crates/adapters/migrations/`.
- ADR-0006 (`PRAGMA integrity_check` on startup), ADR-0009 (audit log hash chain).

## Slice plan summary

| Slice | Title | Severity | Primary files | Effort (h) | Depends on |
|-------|-------|----------|---------------|------------|------------|
| H-1 | Audit hash chain writers | CRITICAL | `core/crates/core/src/storage/{helpers,canonical_json}.rs`, `core/crates/adapters/src/sqlite_storage.rs`, `core/crates/core/src/storage/in_memory.rs` | 4 | — |
| H-2 | `tokio::sync::Mutex` in `InMemoryStorage` | HIGH | `core/crates/core/src/storage/in_memory.rs` | 2 | — |
| H-3 | `application_id` pragma + post-migration check | HIGH | `core/crates/adapters/migrations/0005_application_id.sql` (new), `core/crates/adapters/src/sqlite_storage.rs`, `core/crates/cli/src/run.rs` | 2 | — |
| H-4 | Fix sqlite URL construction | HIGH | `core/crates/adapters/src/sqlite_storage.rs` | 1 | — |
| H-5 | Synchronous startup integrity check | MEDIUM | `core/crates/cli/src/run.rs` | 1 | — |
| H-6 | Cap `parent_chain` `max_depth` | MEDIUM | `core/crates/core/src/storage/{mod,helpers}.rs`, `core/crates/adapters/src/sqlite_storage.rs`, `core/crates/core/src/storage/in_memory.rs` | 1 | — |
| H-7 | `StorageError::NotYetAvailable` variant | MEDIUM | `core/crates/core/src/storage/error.rs`, `core/crates/adapters/src/sqlite_storage.rs` | 1 | — |
| H-8 | Drop or document `SqliteBusy.retries` | LOW | `core/crates/core/src/storage/error.rs` (and any callers) | 1 | — |

Total: 8 slices, ~13h.

All slices are independent (no dependency edges). They may be implemented in any order. `just check` must pass after each slice.

---

## Slice H-1 [cross-cutting] — Audit log hash chain writers

### Goal

Every row written to `audit_log` must carry a real `prev_hash`. Both `SqliteStorage` and `InMemoryStorage` must use the same canonical-JSON serialization so chains computed against either backend are identical and verifiable.

### Why

`record_audit_event` in `SqliteStorage` currently omits `prev_hash` from its INSERT statement (the column gets the all-zero default). `InMemoryStorage::record_audit_event` hard-codes the all-zero string. ADR-0009 requires every row to chain to its predecessor; the gap is structural and cannot be retrofitted without breaking continuity. The chain must work before phase 6's verifier is implemented.

### Entry conditions

- `core::canonical_json` already exists (used by `validate_snapshot_invariants`).
- `audit_log.prev_hash` column is in `0001_init.sql`.
- `AuditEventRow` already has a `prev_hash: String` field.

### Algorithm

1. Add a new module `core/crates/core/src/storage/helpers.rs` (or extend the existing one) with:
   ```rust
   pub fn canonical_json_for_audit_hash(row: &AuditEventRow) -> String { ... }
   pub fn audit_prev_hash_seed() -> &'static str { "00000000...0000" /* 64 zero hex chars */ }
   pub fn compute_audit_chain_hash(prev_canonical_json: &str) -> String { /* sha256 hex */ }
   ```
   The canonical serialization MUST emit all `AuditEventRow` fields **except** `prev_hash` itself, with object keys sorted lexicographically. Use `serde_json` with a sorted map. Produce stable output across both adapters.
2. In `SqliteStorage::record_audit_event`:
   - Wrap the existing INSERT in a `BEGIN IMMEDIATE` transaction.
   - Inside the tx, `SELECT id, caddy_instance_id, correlation_id, occurred_at, occurred_at_ms, actor_kind, actor_id, kind, target_kind, target_id, snapshot_id, redacted_diff_json, redaction_sites, outcome, error_kind, notes FROM audit_log ORDER BY occurred_at DESC, id DESC LIMIT 1`. (Tiebreak by `id` to keep the chain deterministic across same-millisecond rows.)
   - If a row exists: rebuild its `AuditEventRow`, run `canonical_json_for_audit_hash`, sha256 it, and use that as the new row's `prev_hash`. If no row exists: use `audit_prev_hash_seed()`.
   - Bind `prev_hash` in the INSERT (it is currently absent from the column list).
   - `COMMIT`.
3. In `InMemoryStorage::record_audit_event`: same logic — read the last entry from the `Vec`, compute its canonical-JSON hash, set the new entry's `prev_hash`. Use the same helper.
4. Update the existing seven contract tests if they assert on `prev_hash` values; they may need to be relaxed or strengthened to assert the chain links correctly.

### Files to create or modify

- `core/crates/core/src/storage/helpers.rs` — add `canonical_json_for_audit_hash`, `audit_prev_hash_seed`, `compute_audit_chain_hash`. If the file does not exist yet, create it; if it does, extend it.
- `core/crates/core/src/storage/mod.rs` (or wherever `pub mod helpers` lives) — re-export the new helpers.
- `core/crates/adapters/src/sqlite_storage.rs:634` (`record_audit_event`).
- `core/crates/core/src/storage/in_memory.rs:326` (the constant `prev_hash` line).
- `core/crates/adapters/tests/` (or wherever the contract tests live) — add `audit_chain_prev_hash_links_rows`.

### Tests

- New contract test: insert two events through `SqliteStorage`; assert `events[1].prev_hash == sha256(canonical_json_for_audit_hash(events[0]))`.
- Same test against `InMemoryStorage`; assert both adapters produce identical hashes for identical event sequences.
- New test: first row's `prev_hash` equals the all-zero seed.

### Exit conditions

- `record_audit_event` writes a real `prev_hash` value derived from the previous row, or the seed if first.
- `SqliteStorage` and `InMemoryStorage` produce byte-identical canonical-JSON for any given `AuditEventRow`.
- All three new tests pass.
- `just check` is green.

---

## Slice H-2 [standard] — `tokio::sync::Mutex` in `InMemoryStorage`

### Goal

Replace `std::sync::Mutex` with `tokio::sync::Mutex` throughout `InMemoryStorage` so a panic inside a contract test does not poison the lock and cascade-fail subsequent tests.

### Entry conditions

- `tokio` is already in `core/Cargo.toml`'s `dev-dependencies` (verify with `grep tokio core/crates/core/Cargo.toml`).

### Algorithm

1. In `core/crates/core/src/storage/in_memory.rs`:
   - Replace `use std::sync::Mutex;` with `use tokio::sync::Mutex;`.
   - All `.lock().unwrap()` call sites become `.lock().await`.
   - The `async_trait` impl methods are already `async`, so `.await`-ing the lock is fine.
2. If any non-async helper currently holds a `MutexGuard` across an `await`, audit and fix.

### Files to create or modify

- `core/crates/core/src/storage/in_memory.rs` — single import swap + 10–20 call-site changes.
- `core/crates/core/Cargo.toml` — verify `tokio` is in dev-dependencies; add it if not (with `features = ["sync"]`).

### Tests

- New regression test `panic_in_one_method_does_not_poison_subsequent_calls`:
  ```rust
  let storage = InMemoryStorage::new();
  let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
      // panic inside a method that holds the lock
  }));
  // subsequent normal call must succeed
  storage.insert_snapshot(...).await.unwrap();
  ```

### Exit conditions

- `std::sync::Mutex` no longer appears in `in_memory.rs` (`grep "std::sync::Mutex" core/crates/core/src/storage/in_memory.rs` returns nothing).
- The new regression test passes.
- All existing seven contract tests still pass.
- `just check` is green.

---

## Slice H-3 [standard] — `application_id` pragma + post-migration check

### Goal

Set a fixed `PRAGMA application_id` on the database and verify it after migrations. Operators who accidentally point the daemon at a wrong-but-valid SQLite file (via misconfigured `data_dir`) get a clear error instead of silently migrating the wrong database.

### Entry conditions

- The migration runner already reads `_sqlx_migrations` (slice H-3 does not depend on H-7 or others).

### Algorithm

1. Choose the application_id constant. Use `0x54525754` (ASCII `"TRWT"` — Trilithon). Define it as a `pub const` in `core::storage` (e.g. `pub const APPLICATION_ID: u32 = 0x5452_5754;`).
2. Create new migration `core/crates/adapters/migrations/0005_application_id.sql`:
   ```sql
   PRAGMA application_id = 1414681940;  -- 0x54525754 ('TRWT')
   ```
   *(SQLite stores `application_id` as a signed 32-bit integer in the file header; the literal must be the decimal form. Compute and verify before committing.)*
3. In `SqliteStorage::open`, **after** the caller has run `apply_migrations` (i.e. inside `cli/src/run.rs:run_with_shutdown`, immediately after the migration call), read `PRAGMA application_id` and verify it matches the constant. If it does not, return `StorageError::Sqlite { kind: SqliteErrorKind::Other("application_id mismatch — wrong database file?".into()) }` (or a new dedicated kind if simpler).
4. Place the check post-migration so a brand-new SQLite file (default `application_id = 0`) is correctly initialised by the migration before being checked.

### Files to create or modify

- `core/crates/adapters/migrations/0005_application_id.sql` (new).
- `core/crates/core/src/storage/mod.rs` (or `core/crates/core/src/storage.rs`) — add `pub const APPLICATION_ID: u32 = 0x5452_5754;`.
- `core/crates/cli/src/run.rs:91` — after `apply_migrations`, add the check. (Or expose a method `SqliteStorage::verify_application_id(&self) -> Result<(), StorageError>` and call it from `run.rs`.)

### Tests

- Test: open against a freshly-migrated database; `application_id` matches.
- Test: manually open a SQLite file, set `PRAGMA application_id = 999`, run the daemon's open path; expect `StorageError::Sqlite { kind: Other(...) }` mentioning the mismatch.

### Exit conditions

- New migration is in the embedded set and runs cleanly on both fresh and existing databases.
- `application_id` is verified post-migration.
- A wrong-id database is rejected with a clear error.
- `just check` is green.

---

## Slice H-4 [standard] — Fix sqlite URL construction

### Goal

Replace string-formatted SQLite URL with `SqliteConnectOptions::filename()` so paths containing spaces or `#`/`?` characters are handled correctly.

### Algorithm

1. In `core/crates/adapters/src/sqlite_storage.rs:72`, replace:
   ```rust
   let db_url = format!("sqlite://{}/trilithon.db", data_dir.display());
   let opts = SqliteConnectOptions::from_str(&db_url)
       .map_err(...)?
       .create_if_missing(true)
       ...
   ```
   with:
   ```rust
   let opts = SqliteConnectOptions::new()
       .filename(data_dir.join("trilithon.db"))
       .create_if_missing(true)
       ...
   ```
2. Drop the `from_str` import if no longer used.

### Files to create or modify

- `core/crates/adapters/src/sqlite_storage.rs:8-10` (imports) and `:72-90` (open path).

### Tests

- New test: open a database under a path containing a space (e.g. `tmp_path.join("My Files").join("trilithon")`); creation and migration succeed.
- New test: open under a path containing `#`; succeeds.

### Exit conditions

- `format!("sqlite://...")` no longer appears in `sqlite_storage.rs`.
- Both new tests pass.
- `just check` is green.

---

## Slice H-5 [standard] — Synchronous startup integrity check

### Goal

Run `PRAGMA integrity_check` synchronously at daemon startup, before `daemon.started` fires, per ADR-0006.

### Algorithm

1. In `core/crates/cli/src/run.rs`, between the `apply_migrations` call (`:86`) and the existing `tasks.spawn(run_integrity_loop(...))` (`:101`), insert:
   ```rust
   match trilithon_adapters::integrity_check::integrity_check_once(&pool).await {
       Ok(IntegrityResult::Ok) => {
           tracing::info!("storage.integrity_check.startup.ok");
       }
       Ok(IntegrityResult::Failed { detail }) => {
           tracing::error!(detail = %detail, "storage.integrity_check.startup.failed");
           return Err(anyhow::anyhow!("startup integrity check failed: {detail}"));
       }
       Err(e) => {
           tracing::error!(error = %e, "storage.integrity_check.startup.error");
           return Err(anyhow::anyhow!("startup integrity check error: {e}"));
       }
   }
   ```
   Adjust the match arms to whatever `IntegrityResult` actually exposes (see `core/crates/adapters/src/integrity_check.rs:12`).
2. Ensure the failure path maps to `ExitCode::StartupPreconditionFailure` (exit 3). The existing `anyhow::Result<ExitCode>` plumbing already does this if the function returns `Err`.

### Files to create or modify

- `core/crates/cli/src/run.rs:91` — insert the synchronous check.

### Tests

- Update or add an integration test in `core/crates/cli/tests/storage_startup.rs` that opens a deliberately-corrupted database and asserts the daemon exits 3 with a startup-integrity message. *(If corrupting a SQLite file in a test is too much hassle, accept this as covered by manual verification and document the gap inline.)*

### Exit conditions

- Startup runs `integrity_check_once` before spawning the periodic loop.
- A failed startup integrity check returns an error from `run_with_shutdown` (which maps to exit 3).
- `just check` is green.

---

## Slice H-6 [standard] — Cap `parent_chain` `max_depth`

### Goal

Reject `parent_chain` callers passing an unbounded `max_depth`, preventing pool saturation if the chain has a cycle (currently only self-cycles are blocked by the schema CHECK constraint) or if a malicious caller sets `max_depth` to `usize::MAX`.

### Algorithm

1. Define `pub const MAX_PARENT_CHAIN_DEPTH: usize = 256;` in `core/crates/core/src/storage/mod.rs` (or wherever the storage module's public constants live).
2. In `SqliteStorage::parent_chain` and `InMemoryStorage::parent_chain` (if it implements one): if `max_depth > MAX_PARENT_CHAIN_DEPTH`, return `StorageError::Integrity { detail: format!("max_depth {max_depth} exceeds ceiling {MAX_PARENT_CHAIN_DEPTH}") }` immediately, before any DB access.

### Files to create or modify

- `core/crates/core/src/storage/mod.rs` (or `storage.rs`) — add the constant.
- `core/crates/adapters/src/sqlite_storage.rs:556` (`parent_chain`).
- `core/crates/core/src/storage/in_memory.rs` — equivalent guard.

### Tests

- Test: `parent_chain(leaf, MAX_PARENT_CHAIN_DEPTH + 1)` returns `StorageError::Integrity`.
- Test: `parent_chain(leaf, MAX_PARENT_CHAIN_DEPTH)` succeeds against a normal chain.

### Exit conditions

- Both adapters reject excessive depths.
- Both new tests pass.
- `just check` is green.

---

## Slice H-7 [standard] — `StorageError::NotYetAvailable` variant

### Goal

Stop overloading `StorageError::Migration` to mean "feature not yet implemented". Callers matching on `Migration` will currently treat a not-yet-implemented call as a schema migration failure (exit 3).

### Algorithm

1. In `core/crates/core/src/storage/error.rs:88` (after the `Migration` variant), add:
   ```rust
   /// A method is structurally present on the trait but its backing schema or
   /// implementation has not yet been added in this phase.
   #[error("storage feature not yet available: {reason}")]
   NotYetAvailable {
       /// Human-readable description of which feature is missing and where it lands.
       reason: String,
   },
   ```
2. Update the five stubbed methods in `core/crates/adapters/src/sqlite_storage.rs:796-829` to return `StorageError::NotYetAvailable { reason: ... }` with the existing message text.
3. Audit any `match` on `StorageError` callers (likely none yet, but check) to handle the new variant.
4. If `From<StorageError> for ExitCode` exists, decide whether `NotYetAvailable` should map to a developer-error exit code (e.g. exit 64 / `ExitCode::Internal`) or leave it as the default. Recommend `Internal` since reaching this path indicates a wiring bug.

### Files to create or modify

- `core/crates/core/src/storage/error.rs:88` (new variant).
- `core/crates/adapters/src/sqlite_storage.rs:796, 803, 810, 817, 824` (five stubs).
- Any `From<StorageError> for ExitCode` impl.

### Tests

- Existing tests should still pass; if any test asserted on `StorageError::Migration { version: 0, ... }` for a stubbed method, update it to `NotYetAvailable`.

### Exit conditions

- `StorageError::NotYetAvailable` exists.
- All five stubs use it.
- `just check` is green.

---

## Slice H-8 [trivial] — Drop or document `SqliteBusy.retries`

### Goal

The `retries: u32` field on `StorageError::SqliteBusy` is meaningless until phase 4's mutation queue retry loop exists. Either remove it or document the limitation so log readers aren't confused.

### Algorithm

Pick one (recommend option A):

**Option A — Remove the field:**
1. Change `SqliteBusy { retries: u32 }` to `SqliteBusy` (unit variant) in `core/crates/core/src/storage/error.rs:69`.
2. Update `Display`/`thiserror` annotation: `#[error("sqlite busy")]`.
3. Update all construction sites (search `SqliteBusy {` across the workspace).
4. Phase 4 will re-add the field if/when it implements retries.

**Option B — Document:**
1. Add a doc-comment on `retries`: `/// Always 0 in phase 2. Becomes meaningful when the mutation queue's retry loop lands in phase 4.`
2. Leave construction sites unchanged.

### Files to create or modify

- `core/crates/core/src/storage/error.rs:67-72`.
- (Option A only) any construction sites: `grep -rn "SqliteBusy {" core/crates/`.

### Exit conditions

- Either the field is gone, or the field has a doc-comment explaining its phase-2 limitation.
- `just check` is green.

---

## Phase exit conditions

- All 8 slices marked complete.
- `just check` passes from a clean tree.
- All new and existing tests pass.
- A single commit (or one commit per slice) on a `phase-02-hardening` branch, ready for review.

## Out of scope (carry forward — do not implement here)

- Vocabulary drift CI test referencing `AuditEvent::all_kinds()` — belongs to phase 6 (R2-F4, R3-F6).
- Proposal `Claimed` state vs. destructive DELETE on dequeue — belongs to phase 4 (R3-F5).
- In-memory `dequeue_proposal` ordering by `submitted_at` — belongs to phase 4 (R1-F11).
