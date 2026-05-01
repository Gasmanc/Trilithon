# Phase 02 — SQLite Persistence — Unfixed Items

Items identified during the multi-review fix pass that could not be fixed
within this slice, documented per the review process.

---

## UNFIXED 1 — `schema_migrations` table is redundant dead code

**File:** `core/crates/adapters/migrations/0001_init.sql`

The manually-created `schema_migrations` table in `0001_init.sql` is never
updated by sqlx (which uses `_sqlx_migrations`). It was included to mirror a
common Rails-style convention but serves no functional purpose.

**Reason not fixed:** Up-only migration policy prohibits editing
`0001_init.sql`. Removing the table requires a dedicated
`0002_drop_schema_migrations.sql` migration. Deferred to the next
schema-change window.

---

## UNFIXED 2 — Pool size hardcoded at 10

**File:** `core/crates/adapters/src/sqlite_storage.rs`

`SqliteStorage::open` uses `.max_connections(10)`. The phase spec mentions
"pool sized from configuration" but `StorageConfig` in `core` does not have a
`max_connections` field and no slice in Phase 2 adds one.

**Reason not fixed:** Adding `max_connections` to `StorageConfig` is a
config-surface change outside this slice's scope. Tracked for a follow-up
task when storage configuration is expanded.

---

## UNFIXED 3 — Storage trait missing `session_*`, `user_*`, `secrets_*` methods

**Files:** `core/crates/core/src/storage/trait_def.rs`,
`core/crates/adapters/migrations/0001_init.sql`

The `Storage` trait has no methods for sessions, users, or secrets metadata,
despite `0001_init.sql` creating those tables.

**Reason not fixed:** Per the Phase 2 TODO slice plan
(`docs/todo/phase-02-sqlite-persistence.md`), these APIs were not assigned to
any slice in Phase 2. They are deferred to Phase 10 (secrets vault) for
secrets, and future phases for session/user management. The TODO file drives
scope.

---

## UNFIXED 4 — `From<StorageError>` and `From<MigrationError>` for `ExitCode` are unreachable

**File:** `core/crates/cli/src/exit.rs`

`cli/src/exit.rs` defines `From<StorageError>` and `From<MigrationError>` for
`ExitCode`, but `run.rs` wraps errors in `anyhow` before they reach the exit
mapping, so these impls are dead code. Exit code 3 is produced by the
anyhow catch-all.

**Reason not fixed:** Changing the error-handling flow in `run.rs` to
pattern-match before wrapping is a broader refactor of the startup error path.
Deferred.

---

## UNFIXED 5 — Missing `caddy_instance_id` index on `audit_log`

**File:** `core/crates/adapters/migrations/0001_init.sql`

The `audit_log` table has a `caddy_instance_id` column but no index on it.
All other multi-row tables have their instance column indexed.

**Reason not fixed:** Cannot add an index to `0001_init.sql` (up-only policy).
A `0002_` migration would add
`CREATE INDEX IF NOT EXISTS idx_audit_log_instance ON audit_log(caddy_instance_id)`.
