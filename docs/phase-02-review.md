# Phase 02 — SQLite Persistence — Review Log

## Slice 2.2
**Status:** complete
**Summary:** Created `InMemoryStorage` as a `#![cfg(test)]`-gated struct in `core/crates/core/src/storage/in_memory.rs` implementing all `Storage` trait methods using `std::sync::Mutex`-backed collections. Added the §6.6 audit kind vocabulary in `audit_vocab.rs` (not cfg-gated, so adapters can import it in future slices). All seven contract tests pass.

### Simplify Findings
- `tail_audit_log` chains `.rev()` directly onto the filter iterator, avoiding an intermediate `Vec` allocation.
- `dequeue_proposal` collapses to `pos.map(|idx| proposals.remove(idx))` — a single-expression return.
- `Default` impl delegates to `new()` per Rust convention; `is_none_or` used instead of `map_or(true, …)` as suggested by Clippy.
- No redundant code found; vocabulary lives exactly once in `audit_vocab.rs`.

### Fixes Applied
- Clippy: added `significant_drop_tightening` to module-level `#[allow]` (mutex guards are intentionally broad in a test double).
- Clippy: replaced `map_or(true, …)` with `is_none_or(…)` per `unnecessary_map_or` lint.
- Clippy: removed unused `ProposalSource` and `DriftEventRow`/`DriftRowId` imports from the test module.
- Clippy: added `Default` impl for `InMemoryStorage`.
- Formatter: applied `cargo fmt` formatting pass (two minor whitespace diffs).

## Slice 2.3
**Status:** complete
**Summary:** Created `core/crates/adapters/migrations/0001_init.sql` with nine tables (`schema_migrations`, `caddy_instances`, `users`, `sessions`, `snapshots`, `audit_log`, `mutations`, `secrets_metadata`) — every row-data table carries `caddy_instance_id TEXT NOT NULL DEFAULT 'local'`. Added `sqlx` with `sqlite`, `migrate`, and `runtime-tokio` features to the adapters crate. The `migrations_parse` integration test verifies the file parses cleanly via `sqlx::migrate::Migrator::new`.

### Simplify Findings
Nothing flagged — SQL matches spec exactly, test uses `?` propagation, one justified `#[allow(clippy::disallowed_methods)]` for a clippy MIR-level false positive where `Migrator::new`'s internal `.expect()` is attributed to the test call site.

### Fixes Applied
- Formatter: `rustfmt` required joining a two-line `let` binding onto one line.
- Clippy: `assert_eq!` macro expands to internal `expect` calls; replaced with explicit `if count != 1 { return Err(...) }`.
- Clippy: added `#[allow(clippy::disallowed_methods)]` to the test function for the residual MIR-level span attribution from `Migrator::new` internals.

## Slice 2.4
**Status:** complete
**Summary:** Implemented `SqliteStorage` in `core/crates/adapters/src/sqlite_storage.rs` backed by sqlx, with all four required pragmas (WAL, NORMAL sync, foreign keys, 5s busy timeout) set via `SqliteConnectOptions`. An advisory file-lock helper in `lock.rs` uses `fs2::FileExt::try_lock_exclusive` to fail fast if a peer process holds the lock. All `Storage` methods are implemented; drift, proposals stubs return `StorageError::Migration` with a versioned message. All eight named integration tests pass.

### Simplify Findings
- `row_to_snapshot` extracted as a shared helper used across `get_snapshot`, `parent_chain`, and `latest_desired_state` (avoids repeating 14-field struct construction).
- `sqlx_err` helper keeps all error mappings consistent in a single place.
- `tail_audit_log` builds the WHERE clause as SQL text (safe because only predicate structure is injected; all values are bound parameters) — same pattern as the in-memory double.
- `actor_kind_str` and `outcome_str` made `const fn` per clippy `missing_const_for_fn` lint.

### Fixes Applied
- Gate failure 1: `async-trait` dependency was missing from `adapters/Cargo.toml` — added it.
- Gate failure 1: switched from `sqlx::query!()` macros (require DATABASE_URL) to dynamic `sqlx::query()` throughout.
- Gate failure 2: `LockHandle` missing `Debug` derive — added `#[derive(Debug)]`.
- Gate failure 3 (clippy): `FileExt::unlock()` call in `Drop` was ambiguous with `std::fs::File::unlock` (stable 1.89); qualified as `FileExt::unlock(&self.file)` to select the `fs2` method.
- Gate failure 3 (clippy): `std::io::Error::new(ErrorKind::Other, …)` rewritten to `std::io::Error::other(…)`.
- Gate failure 3 (clippy): `pool()` made `const fn`; `actor_kind_str`/`outcome_str` made `const fn`.
- Gate failure 3 (clippy): `sqlx_err` annotated with `#[allow(clippy::needless_pass_by_value)]` to preserve the ergonomic `.map_err(sqlx_err)` pattern.

## Slice 2.5
**Status:** complete
**Summary:** Implemented `core/crates/adapters/src/migrate.rs` — a migration runner wrapping `sqlx::migrate!` with a downgrade refusal gate. The runner queries `_sqlx_migrations` (sqlx's internal tracking table) before and after `MIGRATOR.run()` to compute `applied_count`, and refuses to start with `MigrationError::Downgrade` if the database's max version exceeds the embedded set max. All three integration tests (`fresh_db_applies_all`, `idempotent_second_run`, `refuses_downgrade`) use in-memory SQLite for isolation and pass cleanly.

### Simplify Findings
- `MIGRATOR.iter().map(...).max().map_or(0, ...)` avoids a `.map().unwrap_or()` chain (caught by Clippy `map_unwrap_or` on first gate run).
- `Ok(None) | Err(_) => 0` collapses two arms into one (caught by Clippy `match_wildcard_for_single_variants` / `single_match_else` on first gate run).
- The test wildcard `other =>` arm was replaced with the explicit `MigrationError::Sqlx { source }` variant — required by the strict `match_wildcard_for_single_variants` lint.

### Fixes Applied
- Gate failure 1 (fmt): `rustfmt` reordered imports and collapsed a 4-line `connect_with` chain into one line in the test helper.
- Gate failure 1 (clippy): added doc comments to all `MigrationError` struct fields (`db_version`, `embedded_max`, `source`).
- Gate failure 1 (clippy): replaced `.map(...).unwrap_or(0)` chains with `.map_or(0, ...)` in two places.
- Gate failure 2 (clippy): merged `Ok(None) => 0, Err(_) => 0` into `Ok(None) | Err(_) => 0`.
- Gate failure 2 (clippy): replaced wildcard `other =>` arm in test match with explicit `MigrationError::Sqlx { source }` variant.
- Gate failure 2 (clippy): added `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::disallowed_methods)]` to test file — same pattern as `sqlite_storage.rs` tests.

## Slice 2.6
**Status:** complete
**Summary:** Added `ShutdownObserver` trait in `core/crates/core/src/lifecycle.rs` to break the layering dependency; implemented it on `ShutdownSignal` in cli. Created `integrity_check.rs` in adapters with `run_integrity_loop` (tokio::select! over a 6-hour ticker and shutdown signal) and `integrity_check_once` (PRAGMA integrity_check). All three tests pass: inline unit test confirms `Ok` on healthy DB, integration test confirms the loop exits on shutdown within 500 ms.

### Simplify Findings
- `MissedTickBehavior::Skip` correctly handles delayed ticks without catching up.
- `() = shutdown.wait_for_shutdown()` is more explicit than `_` per clippy `ignored_unit_patterns`.
- `SQLite` backtick-wrapping in doc comments required throughout to satisfy `doc_markdown` lint.

### Fixes Applied
- Gate failure 1 (fmt): reformatted `sqlx::query_scalar` chain and import list in test file.
- Gate failure 2 (clippy): fixed `doc_markdown` lint — `SQLite` → `` `SQLite` `` in four doc strings and module-level comment.
- Gate failure 2 (clippy): fixed `ignored_unit_patterns` lint — `_ =` → `() =` in `tokio::select!` shutdown arm.
