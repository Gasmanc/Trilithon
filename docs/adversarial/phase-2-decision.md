# Design Decision — Phase 2

**Date:** 2026-05-08
**Rounds:** 3 (retro — phase 2 was already coded when the rounds were run)
**Final approach:** Phase 2 ships the SQLite persistence substrate (`Storage` trait, `InMemoryStorage` double, `0001_init.sql`, `SqliteStorage` adapter with WAL pragmas, advisory file lock, sqlx-based migration runner, periodic `PRAGMA integrity_check`, and daemon startup wiring). Phase 2 was implemented before adversarial review; this decision doc records a retro triage of the three round files against the merged code.

---

## Status

This doc closes out three round files (`phase-2-round-1.md`, `-round-2.md`, `-round-3.md`) that were generated *after* phase 2 had already been coded. Each finding has been graded against the actual code on `main` and classified as **Resolved**, **Real risk still in code**, **Obsolete**, or **Carry forward** to a later phase.

The round files are removed alongside this decision doc (they were never committed per the `/plan-adversarial` workflow; they ended up tracked by accident on this branch).

---

## Rejected Approaches

| Approach | Rejected because |
|----------|-----------------|
| Custom `schema_migrations` table | sqlx writes to `_sqlx_migrations` automatically; the custom table was always empty and never read after the downgrade check moved (R1-F12 / R2-F5). Removed from DDL. |
| `std::sync::Mutex` for `ShutdownSignal` plumbing into `core` | Would have either (a) required `tokio` as a `core` dependency (violates "No Tokio in core") or (b) required `adapters` to import from `cli` (layer violation). Resolved with `core::lifecycle::ShutdownObserver` trait + concrete impl in `cli` (R1-F2). |
| Reading `MAX(config_version)` outside a transaction | TOCTOU race: two concurrent inserters read the same max and both compute `max+1`; the unique index would mask one as a deduplication success. Resolved by wrapping read-modify-write in `BEGIN IMMEDIATE` (R1-F3). |

---

## Key Constraints Surfaced

The adversarial process surfaced these constraints that any future phase must respect:

1. **Audit hash chain is structural, not retrofittable.** Every `audit_log` row written without a real `prev_hash` permanently breaks the chain at that point — phase 6's verifier cannot bridge a gap. The DDL column is in place, but the writers in `SqliteStorage` and `InMemoryStorage` currently emit the all-zero default. This must be fixed before any audit rows in production matter.
2. **`core` must not depend on `tokio`.** The `ShutdownObserver` trait pattern is the canonical way for `adapters` to participate in shutdown without violating the layer rule. New adapters that need shutdown awareness use the same trait, not a direct import.
3. **All write-then-read sequences against `snapshots` must run inside `BEGIN IMMEDIATE`.** SQLite's serialized writer plus a 10-connection pool does not give atomicity by itself; explicit transactions are required wherever `MAX(...)` informs a subsequent INSERT.
4. **`AUDIT_KINDS` is a closed vocabulary.** Any new audit `kind` value introduced by a future phase must be added to `core::storage::audit_vocab::AUDIT_KINDS` and covered by a vocabulary-drift test, otherwise `record_audit_event` will reject it at runtime.
5. **Application-level retry counts do not yet exist.** `StorageError::SqliteBusy { retries }` will report `retries: 0` until the mutation queue's retry loop lands in phase 4 — readers of error logs should know.

---

## Triage Summary

**Resolved in code: 17**
- Schema completeness: R1-F1, R1-F5, R1-F6, R1-F7
- Concurrency / transactions: R1-F3, R2-F6
- Layer rules / shutdown: R1-F2, R2-F3
- Migration tracking: R1-F12, R2-F5, R3-F3, R3-F7
- Pragmas / pool: R1-F14
- Query safety: R1-F10
- Implementation correctness: R3-F8

**Real risks still in code: 8** *(see Unaddressed Findings table)*

**Obsolete (no longer applicable to phase 2): 3**
- R1-F11 (in-memory dequeue ordering) — `dequeue_proposal` is a stub; carry to phase 4
- R3-F2 (invalid hash-chain SQL) — subsumed by R2-F1
- R3-F5 (proposal "claimed" state) — `dequeue_proposal` is a stub; carry to phase 4

**Carry forward to other phases: 2**
- R2-F4 + R3-F6 → phase 6: when new audit kinds are introduced, update `AUDIT_KINDS` and add a vocabulary-drift CI test
- R1-F11 + R3-F5 → phase 4: pick a state model for proposal dequeue (add `Claimed` variant + CHECK update, OR destructive DELETE on dequeue)

---

## Unaddressed Findings (Real Risks Still in Code)

These findings represent real risks against the merged phase 2 code. Each is recorded here as known-and-tracked rather than accepted-as-fine. Remediation guidance follows in the next section.

| ID | Severity | Finding | Status |
|----|----------|---------|--------|
| R2-F1 + R3-F4 | CRITICAL | Audit log `prev_hash` never written — both `SqliteStorage` and `InMemoryStorage` emit the all-zero default. Hash chain is structurally broken at the phase 2 / phase 6 seam. | OPEN — blocks phase 6 |
| R1-F4 + R2-F2 | HIGH | `InMemoryStorage` uses `std::sync::Mutex`; test panic poisons the lock and cascades failures across the seven contract tests. | OPEN |
| R1-F8 | HIGH | No `PRAGMA application_id` in DDL; daemon will happily migrate and write to a wrong-but-valid SQLite file pointed at by a misconfigured `data_dir`. | OPEN |
| R1-F9 | HIGH | SQLite connection URL built via `format!("sqlite://{}/...", data_dir.display())`; paths with spaces or `#`/`?` characters fail or silently truncate. Data-loss risk on macOS/Windows dev paths. | OPEN |
| R1-F13 + R2-F8 | MEDIUM | Startup `integrity_check_once` is not called synchronously; `run_integrity_loop` is spawned with a 6-hour interval. ADR-0006 requires startup check. | OPEN |
| R2-F10 | MEDIUM | `parent_chain.max_depth` is unbounded; a caller passing `usize::MAX` (or a cycle in a corrupted chain) will saturate the connection pool. Schema's `CHECK (parent_id != id)` prevents self-cycles only. | OPEN |
| R2-F7 | MEDIUM | Stub methods (`record_drift_event`, `enqueue_proposal`, `dequeue_proposal`, `expire_proposals`, `latest_drift_event`) return `StorageError::Migration { version: 0, ... }`. Semantically overloaded — callers matching on `Migration` will treat "feature not yet implemented" as "schema migration failed". | OPEN |
| R1-F15 | LOW | `StorageError::SqliteBusy { retries: u32 }` carries a field that is meaningless until phase 4's mutation-queue retry loop exists. Will report `retries: 0` if it ever fires. | OPEN |

---

## Recommended Remediation

A small "phase 2 hardening" task list. Items ordered by criticality. The first two should land before phase 6 / phase 4 begin; the rest are independently mergeable.

### CRITICAL — must land before phase 6 audit work

**H-1: Implement audit hash chain writers (R2-F1, R3-F4, R3-F2)**
- Add `core::storage::canonical_json_for_hash(&AuditEventRow) -> String` helper that produces a deterministic serialization (lexicographically sorted keys, all fields except `prev_hash`).
- Update `SqliteStorage::record_audit_event` to:
  - Inside a `BEGIN IMMEDIATE` transaction, `SELECT canonical-json-of-latest-row + sha256(...)` to compute `prev_hash`. If the table is empty, use the all-zero sentinel.
  - Bind `prev_hash` in the INSERT statement (it is currently omitted entirely).
- Update `InMemoryStorage::record_audit_event` to use the same helper so both adapters produce identical chains.
- Add contract test `hash_chain_prev_hash_links_rows` that inserts two events and asserts `row[1].prev_hash == sha256(canonical_json(row[0]))`.
- Files: `core/crates/core/src/storage/{helpers,canonical_json}.rs`, `core/crates/adapters/src/sqlite_storage.rs:634`, `core/crates/core/src/storage/in_memory.rs:326`.

### HIGH

**H-2: Use `tokio::sync::Mutex` in `InMemoryStorage` (R1-F4, R2-F2)**
- Swap `std::sync::Mutex` for `tokio::sync::Mutex` throughout `core/crates/core/src/storage/in_memory.rs`. Update all `.lock().unwrap()` → `.lock().await`.
- Add a regression test that panics inside one method and asserts subsequent calls still succeed.
- File: `core/crates/core/src/storage/in_memory.rs:19` and downstream call sites.

**H-3: Add `PRAGMA application_id` and post-migration check (R1-F8, R3-F1)**
- Add `PRAGMA application_id = 0x54525754;` (or chosen fixed integer) to a new migration `0005_application_id.sql` so it lands on existing databases too.
- In `SqliteStorage::open`, **after** `apply_migrations` returns successfully, read `PRAGMA application_id`. If non-zero and not the expected value, return `StorageError::Sqlite { kind: Other("application_id mismatch — wrong database file?") }`.
- Place the check post-migration to avoid R3-F1's fresh-install-fail trap.
- File: `core/crates/adapters/migrations/0005_application_id.sql` (new), `core/crates/adapters/src/sqlite_storage.rs:65`, `core/crates/cli/src/run.rs:75`.

**H-4: Fix sqlite URL construction (R1-F9)**
- Replace `format!("sqlite://{}/trilithon.db", data_dir.display())` + `SqliteConnectOptions::from_str(...)` with `SqliteConnectOptions::new().filename(data_dir.join("trilithon.db"))`.
- Add a test that opens a database under a path containing a space and `#`.
- File: `core/crates/adapters/src/sqlite_storage.rs:72`.

### MEDIUM

**H-5: Synchronous startup integrity check (R1-F13, R2-F8)**
- In `run_with_shutdown`, after `apply_migrations` and before `tasks.spawn(run_integrity_loop(...))`, add `trilithon_adapters::integrity_check::integrity_check_once(&pool).await?` and convert a non-Ok result to exit code 3.
- File: `core/crates/cli/src/run.rs:91-105`.

**H-6: Cap `parent_chain` max_depth (R2-F10)**
- Define `pub const MAX_PARENT_CHAIN_DEPTH: usize = 256;` in `core::storage`.
- In `SqliteStorage::parent_chain` (and `InMemoryStorage`'s equivalent), if `max_depth > MAX_PARENT_CHAIN_DEPTH` return `StorageError::Integrity { detail }`.
- File: `core/crates/adapters/src/sqlite_storage.rs:556`, `core/crates/core/src/storage/in_memory.rs`.

**H-7: Add `StorageError::NotYetAvailable` (R2-F7)**
- Add variant `NotYetAvailable { reason: String }` to `StorageError`.
- Update the five stubbed methods in `SqliteStorage` to return `NotYetAvailable` instead of `Migration { version: 0, ... }`.
- File: `core/crates/core/src/storage/error.rs:88` (insert), `core/crates/adapters/src/sqlite_storage.rs:796-829`.

### LOW

**H-8: Drop or document `SqliteBusy.retries` (R1-F15)**
- Either: remove the field (simpler) and re-add it in phase 4 when the retry loop exists.
- Or: document on the variant that the value is always 0 in phase 2, and only meaningful once the mutation queue lands.
- File: `core/crates/core/src/storage/error.rs:69`.

### Carry-forward items (open as TODOs in the named phases, do not fix here)

- **Phase 4** — proposal state model decision (R3-F5): pick `Claimed` variant + CHECK update, OR destructive DELETE on dequeue. Update `dequeue_proposal` algorithm in `phase-04` TODO.
- **Phase 4** — proposal dequeue ordering (R1-F11): in-memory implementation must use `min_by_key(|p| p.submitted_at)`, matching the adapter's `ORDER BY submitted_at ASC` to keep contract tests honest.
- **Phase 6** — audit kind drift CI (R2-F4, R3-F6): when new audit kinds are added, update `AUDIT_KINDS` and add a test asserting every `AuditEvent` enum variant maps to a kind in the list. Define `AuditEvent::all_kinds()` to make the test implementable.

---

## Round Summary

| Round | Critical | High | Medium | Low | Outcome |
|-------|----------|------|--------|-----|---------|
| 1 | 3 | 6 | 5 | 1 | 9 resolved in code; 5 real risks remain; 1 obsolete (proposals are stubbed) |
| 2 | 2 | 4 | 4 | 0 | 4 resolved; 4 real risks remain (R2-F1 audit chain is the most consequential); 1 obsolete; 1 carry-forward |
| 3 | 2 | 3 | 3 | 0 | 3 resolved; 1 carry-forward; 2 obsolete (R3-F1 because R1-F8 was never fixed; R3-F2 subsumed by R2-F1); 2 already represented above |

**Net unaddressed: 8 findings → 8 hardening items above.**
