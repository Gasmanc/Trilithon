# Phase 5 — Code Adversarial Review Findings

**Reviewer:** code_adversarial
**Date:** 2026-05-05
**Diff range:** d4a320a..HEAD
**Phase:** 5

---

[HIGH] HARDCODED caddy_instance_id BREAKS MULTI-INSTANCE MONOTONICITY CHECK
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 388-392
Description: The config_version monotonicity guard is scoped to `caddy_instance_id = 'local'` — a string literal baked into both the MAX query and the INSERT. The Snapshot struct carries no caddy_instance_id field. If a second Caddy instance is onboarded, every new snapshot passes the monotonicity check against zero rows, silently permitting config_version = 1 to be reused.
Technique: Assumption Violation
Suggestion: Add `caddy_instance_id` back to `Snapshot` (or derive it from context) and use it as the scope for the MAX query rather than the literal 'local'.

[HIGH] InMemoryStorage DIVERGES FROM SqliteStorage ON DUPLICATE SEMANTICS
File: core/crates/core/src/storage/in_memory.rs
Lines: 71-75
Description: `SqliteStorage::insert_snapshot` returns `Ok(existing_id)` for a byte-equal duplicate (idempotent dedup). `InMemoryStorage::insert_snapshot` returns `Err(StorageError::SnapshotDuplicate)` for the same call. Any caller tested against InMemoryStorage and deployed against SqliteStorage will behave differently on the idempotent-retry path.
Technique: Composition Failure
Suggestion: Align `InMemoryStorage::insert_snapshot` to return `Ok(id)` when the existing body is byte-equal, and `Err(SnapshotHashCollision)` when the body differs.

[HIGH] DEDUPLICATION PATH RETURNS EARLY INSIDE AN OPEN TRANSACTION
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 360-365
Description: When a byte-equal duplicate is detected, the code executes `return Ok(SnapshotId(id))` inside an open `let mut tx = self.pool.begin()...` scope. The transaction is dropped without explicit commit or rollback. This relies on sqlx's Drop implementation rolling back correctly and creates a pattern that is one future edit away from a real leak.
Technique: Assumption Violation
Suggestion: Explicitly call `tx.rollback().await` on the early-return paths before returning.

[WARNING] MONOTONICITY GUARD DEDUP BYPASS MASKS STALE VERSION
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 353-402
Description: The dedup check exits early for byte-equal duplicates before the monotonicity check runs. A caller can re-submit a genuine deduplicated snapshot with a stale config_version and receive a silent success — even when the current max is 5 and config_version = 1.
Technique: Abuse Case
Suggestion: On the dedup-exit path, verify that the config_version in the incoming snapshot matches the stored row's config_version. Return SnapshotVersionMismatch if they differ.

[WARNING] fetch_by_date_range WITH EMPTY RANGE PERFORMS FULL TABLE SCAN
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 166-205
Description: When `SnapshotDateRange { since: None, until: None }` is passed, the query omits the WHERE clause entirely and returns all rows with no LIMIT. The snapshots table grows unboundedly over the daemon lifetime.
Technique: Abuse Case
Suggestion: Add a mandatory `limit: u32` parameter or enforce a maximum result count.

[WARNING] IMMUTABILITY TRIGGERS DO NOT EXIST UNTIL MIGRATION 0004 RUNS
File: core/crates/adapters/migrations/0004_snapshots_immutable.sql
Lines: general
Description: The snapshots_no_update and snapshots_no_delete triggers are added by migration 0004. Any code path that inserts snapshots between `open()` returning and `apply_migrations()` completing operates on a database without the immutability guarantee.
Technique: Assumption Violation
Suggestion: Make `SqliteStorage::open` unconditionally apply migrations before returning, or add an assertion in `insert_snapshot` that the migration version is at least 4.

[WARNING] canonicalise_value SORTS KEYS WITH sort_unstable_by — DUPLICATE KEYS UNDEFINED ORDER
File: core/crates/core/src/canonical_json.rs
Lines: 69-71
Description: `sort_unstable_by` applied to flattened (key, value) pairs does not guarantee stable relative order between equal keys. If a custom Serialize impl emits duplicate JSON object keys, two logically identical maps could produce different canonical bytes and different content addresses.
Technique: Assumption Violation
Suggestion: Add duplicate-key detection in `canonicalise_value`, or use `sort_by` (stable) and document that duplicate keys produce undefined behaviour.
