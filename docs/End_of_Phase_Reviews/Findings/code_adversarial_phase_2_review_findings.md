---
id: duplicate:area::phase-2-code-adversarial-review-findings:legacy-uncategorized
category: duplicate
kind: process
location:
  area: phase-2-code-adversarial-review-findings
  multi: false
finding_kind: legacy-uncategorized
phase_introduced: unknown
status: open
created_at: migration
created_by: legacy-migration
last_verified_at: 0a795583ea9c4266e7d9b0ae0f56fd47d2ecf574
severity: medium
do_not_autofix: false
---

# Phase 2 — Code Adversarial Review Findings

**Reviewer:** code_adversarial
**Date:** 2026-05-07
**Diff range:** be773df..cfba489
**Phase:** 2

---

[HIGH] TRANSACTION LEAKED ON SPAWN-BLOCKING CANCELLATION
File: `core/crates/adapters/src/sqlite_storage.rs`
Lines: 280–393
Description: `insert_snapshot_inner` manually issues `BEGIN IMMEDIATE`, then relies on sequential `ROLLBACK` or `COMMIT` calls to resolve the transaction. The connection is borrowed from the pool (`pool.acquire()`), not from a sqlx `Transaction` guard. If the task that owns this future is cancelled between `BEGIN IMMEDIATE` and the next `ROLLBACK`/`COMMIT` call — which can happen when the `drain_tasks` timeout in `run.rs` fires and `JoinSet::abort_all()` is called — the connection is returned to the pool while the SQLite transaction is still open. The next caller to borrow that connection will inherit an in-progress write transaction, causing `SQLITE_BUSY` or silent data corruption.
Suggestion: Replace the raw `BEGIN IMMEDIATE` / `COMMIT` / `ROLLBACK` pattern with sqlx `pool.begin()` / `.begin_immediate()` and let the `Transaction` drop guard issue the rollback automatically; this is resilient to cancellation.

[HIGH] DOWNGRADE CHECK IS BYPASSED WHEN MIGRATION TABLE VERSION OVERFLOWS u32
File: `core/crates/adapters/src/migrate.rs`
Lines: 78, 96
Description: The version stored in `_sqlx_migrations` is an `i64` in the database. When it is converted to `u32` via `u32::try_from(v).unwrap_or(0)`, a negative or very large value silently becomes `0`. If a corrupted or attacker-controlled database contains a version of, say, `-1` or `> u32::MAX`, `db_version` is mapped to `0`, which is ≤ `embedded_max`. The downgrade guard passes, and sqlx then attempts to run migrations against a database it should have refused.
Suggestion: Return `MigrationError::Read` (or a new `MigrationError::VersionOverflow`) instead of silently mapping to 0; use `u32::try_from(v).map_err(|_| MigrationError::Read { source: ... })` and propagate the error.

[HIGH] STORAGE OPEN/MIGRATE ERRORS EXIT WITH WRONG CODE (1 NOT 3)
File: `core/crates/cli/src/run.rs`
Lines: 77–91
Description: Both `SqliteStorage::open` and `apply_migrations` errors are converted to `anyhow::Error` via `map_err(|e| anyhow::anyhow!(...))` and returned with `?`. The error is erased into `anyhow::Error` first, so if a caller ever constructs the error path differently (e.g. in a future function that returns `MigrationError` directly), the two conversion paths diverge. More immediately: the `LockError::AlreadyHeld` case in `lock.rs` is wrapped via `std::io::Error::other(e.to_string())`, which discards the structured error type; monitoring code that inspects `StorageError` variants cannot distinguish "lock held" from any other I/O failure.
Suggestion: Add a `StorageError::LockHeld` variant so that the lock-held condition is not erased into a generic `Io` error.

[WARNING] INTEGRITY_CHECK USES `fetch_one` — PANICS ON EMPTY RESULT SET
File: `core/crates/adapters/src/integrity_check.rs`
Lines: 30–32
Description: `PRAGMA integrity_check` returns one or more rows — one row with `"ok"` when healthy, or multiple rows describing individual problems. `fetch_one` returns an error if the result set is empty. More importantly, when the database reports multiple problems it returns only the first row and silently discards the rest. The `IntegrityResult::Failed { detail }` variant therefore only surfaces one line of a multi-line corruption report.
Suggestion: Use `fetch_all` and join the rows with newlines into `detail`, or document the behavior in the function's doc comment.

[WARNING] DUPLICATE `schema_migrations` TABLE IN 0001_init.sql NEVER READ
File: `core/crates/adapters/migrations/0001_init.sql`
Lines: 4–9
Description: `0001_init.sql` creates a `schema_migrations` table with `version`, `applied_at`, `description`, and `checksum` columns. This table is never queried by the migration runner, which queries only `_sqlx_migrations` (the sqlx-managed table). Both tables will exist in the database after migration. Any future migration that also creates a `schema_migrations` table would fail with a `table already exists` error.
Suggestion: Remove the hand-rolled `schema_migrations` DDL from `0001_init.sql` and rely solely on `_sqlx_migrations`.

[SUGGESTION] ADVISORY LOCK SURVIVES ACROSS FORK — LOCK HANDLE DOES NOT MARK CLOSE-ON-EXEC
File: `core/crates/adapters/src/lock.rs`
Lines: 35–50
Description: The `File` created for the lock is opened without setting `O_CLOEXEC` / `FD_CLOEXEC`. On a `fork`+`exec` (e.g. if the daemon ever spawns a child process), the child process inherits the open file descriptor and the OS-level lock. The child will hold the lock without owning a `LockHandle`, so the parent's `LockHandle::drop` will release the lock while the child still holds the file descriptor.
Suggestion: After opening the file, call `nix::fcntl::fcntl(fd, F_SETFD, FD_CLOEXEC)` (or use `std::os::unix::fs::OpenOptionsExt::custom_flags(libc::O_CLOEXEC)` on the `OpenOptions`) to prevent the descriptor from being inherited across exec.
