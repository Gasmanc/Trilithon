# Phase 2 — Qwen Review Findings

**Reviewer:** qwen
**Date:** 2026-05-07
**Diff range:** be773df..cfba489
**Phase:** 2

---

[CRITICAL] SQL-INJECTION-LIKE-INJECTION-IN-AUDIT-GLOB
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 691-694
Description: When the audit selector `kind_glob` is an exact match (no trailing `*`), the code passes the literal string directly into a `LIKE ?` predicate without escaping metacharacters. The dot in `"config.applied"` matches any character in LIKE semantics, so `"configXapplied"` would erroneously match. This is both a correctness bug (wrong rows returned) and a mild information-disclosure vector — a crafted actor or event kind could leak data across correlation boundaries. The `InMemoryStorage` double uses `String::eq` for exact matches (correct), creating an inconsistency where tests pass but production queries return spurious rows.
Suggestion: Escape LIKE metacharacters (`%`, `_`) in the literal string before binding, or use a separate exact-match SQL branch (`kind = ?`) for non-glob selectors.

[HIGH] APPLIED-COUNT-CALCULATION-ERROR
File: core/crates/adapters/src/migrate.rs
Lines: 117-118
Description: `applied_count` is computed as `current_version - db_version` using version numbers, not row counts. If migration versions are non-sequential (e.g., 1, 2, 5), then `applied_count = 5 - 2 = 3` is correct by coincidence, but with gaps like versions 1, 10, it would report `9` applied when only 1 migration ran. sqlx does not mandate sequential versions. The value is used in the `tracing::info!` log line and exposed in `MigrationOutcome`, which downstream consumers may rely on for audit trails.
Suggestion: Query `SELECT COUNT(*) FROM _sqlx_migrations WHERE version > ?` to count actual rows applied, or document that versions must be sequential and enforce it at build time.

[HIGH] TAIL-AUDIT-LOG-PERFORMANCE-NO-OFFSET-LIMIT
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 673-782
Description: The `tail_audit_log` query has no index hint optimization for the `ORDER BY occurred_at DESC` clause when no `since`/`until` filter is present. With the existing `audit_log_occurred_at` index, SQLite can perform an index-only reverse scan, but the query as written may trigger a filesort for wide range queries. More critically, there is no pagination mechanism — callers requesting large limits will materialize the entire result set in memory via `fetch_all`. For an audit log that grows unbounded, this will degrade over time.
Suggestion: Use cursor-based pagination (pass the last seen `occurred_at` and id) rather than limit-only queries, or at minimum document that `limit` should be bounded by callers.

[HIGH] MISSING-DIRECTORY-IS-UNCLEAR-ERROR
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 65-69
Description: When `data_dir` does not exist, `LockHandle::acquire` fails with a generic I/O error like "No such file or directory". The operator sees a confusing message about acquiring a lock rather than "data directory does not exist". The CLI integration test `missing_data_dir_exits_3` accepts exit code 2 OR 3, indicating the error path is not deterministic (config loader may intercept first).
Suggestion: Check `data_dir.exists()` and return a clear `StorageError::Io` with a descriptive message, or validate in the config loader before reaching `SqliteStorage::open`.

[WARNING] SHUTDOWN-OBSERVER-SIGNUAL-RACE-IN-INTEGRITY-LOOP
File: core/crates/adapters/src/integrity_check.rs
Lines: 52-68
Description: In the `tokio::select!` loop, the `ticker.tick()` and `shutdown.changed()` branches have equal priority. If both are ready in the same poll iteration, `tokio::select!` picks randomly. This means a shutdown signal could be delayed by one full interval (6 hours by default) if the ticker wins the race. More concerning: if the ticker fires and the integrity check itself takes a long time (corrupt DB), shutdown will wait for it to complete before checking the signal again.
Suggestion: Add a timeout around `integrity_check_once` (e.g., 30 seconds) to bound the worst-case shutdown latency, or use `tokio::select!` with biased polling towards the shutdown branch.

[WARNING] INSERT-SNAPSHOT-REDUNDANT-HASH-COMPUTATION
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 525-530
Description: `insert_snapshot` calls `validate_snapshot_invariants` which computes `content_address_bytes(desired_state_json)` (SHA-256). Then `insert_snapshot_inner` performs multiple database queries. The SHA-256 computation is ~O(n) where n is the JSON payload size. For large configs, this adds measurable latency before the transaction even starts. While the validation-before-transaction ordering is correct (fail fast), the hash is computed unconditionally even for idempotent duplicates that could be detected by a quick DB lookup first.
Suggestion: Consider checking for existing ID first (fast index lookup) before computing SHA-256, to short-circuit idempotent re-inserts.

[SUGGESTION] LOCK-HANDLE-DROP-IGNORES-UNLOCK-ERROR
File: core/crates/adapters/src/lock.rs
Lines: 59-64
Description: The `Drop` implementation silently ignores unlock failures (`let _ = FileExt::unlock(&self.file)`). If the unlock fails (e.g., fd closed prematurely, filesystem error), the lock file persists on disk. This is benign for the advisory lock semantics (the lock is process-scoped and released on fd close anyway), but could confuse operators inspecting leftover lock files.
Suggestion: Add a `tracing::warn!` on unlock failure for observability, or document the behavior explicitly.

[SUGGESTION] DB-ERRORS-MASKING-LOST-CONTEXT
File: core/crates/adapters/src/db_errors.rs
Lines: 12-36
Description: The `sqlx_err` function maps all non-specific database errors to `SqliteErrorKind::Other(e.to_string())`, which includes the full sqlx error message. These error strings often contain internal details (connection URLs, file paths) that leak into logs. Since `StorageError::Sqlite { kind }` displays as `"sqlite error: Other(\"...\")"`, the full string appears in operator-visible output.
Suggestion: Either sanitize the error string before storing in `Other`, or add a separate variant for "disk IO" errors that commonly occur in SQLite and don't need the full context string.

[SUGGESTION] TAIL-AUDIT-LOG-DYNAMIC-SQL-FORMAT-STRING
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 713-730
Description: The WHERE clause is assembled via `format!()` using a vector of `'static str` literals. While the author correctly notes that all user input goes through bind parameters (safe from injection), the dynamic format-string approach is harder to audit than a static query with optional subqueries. Any future addition to the condition-building logic could inadvertently introduce user-controlled SQL text.
Suggestion: Consider using a query builder library or at minimum add a compile-time assertion that `conditions` only contains static strings, to make the security invariant self-documenting.

[SUGGESTION] ROW-TO-SNAPSHOT-VERBOSE-ERROR-PROPAGATION
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 448-484
Description: Every field extraction uses `.map_err(sqlx_err)?` individually. If `try_get` fails partway through (e.g., column type mismatch), the function returns early with a `StorageError` but the partially-constructed data is lost. This is correct behavior (never return partial rows), but the error messages from `sqlx::Error::ColumnIndex` are cryptic (e.g., "column index 7 not found").
Suggestion: Map column errors to a more descriptive `StorageError::Integrity` with the column name included.
