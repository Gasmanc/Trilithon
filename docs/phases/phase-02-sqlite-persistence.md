# Phase 2 — SQLite persistence and migration framework

Source of truth: [`../phases/phased-plan.md#phase-2--sqlite-persistence-and-migration-framework`](../phases/phased-plan.md#phase-2--sqlite-persistence-and-migration-framework).

## Pre-flight checklist

- [ ] Phase 1 complete; the daemon starts, reads configuration, and exits cleanly.
- [ ] The data directory path from configuration is resolvable to a writable location.

## Tasks

### Backend / core crate

- [ ] **Define the `Storage` trait surface.**
  - Acceptance: `crates/core/src/storage.rs` MUST expose async methods for snapshot insert, snapshot fetch by content identifier, audit row insert, audit row range query, mutation queue enqueue and dequeue, session create and revoke, secrets metadata insert and fetch, and user create and authenticate.
  - Done when: the trait compiles, contains no SQLite types, and `cargo test -p trilithon-core storage::tests::trait_is_pure` passes.
  - Feature: foundational (cross-cuts T1.2, T1.7, T1.14, T1.15).
- [ ] **Provide an in-memory `Storage` test double.**
  - Acceptance: An `InMemoryStorage` test double MUST live under `#[cfg(test)]` only and MUST satisfy the trait contract for unit tests.
  - Done when: `cargo test -p trilithon-core storage::tests::in_memory_round_trip` passes.
  - Feature: foundational.
- [ ] **Encode typed `StorageError`.**
  - Acceptance: A `thiserror`-backed `StorageError` enum MUST cover not-found, conflict, busy, integrity-failure, and underlying-IO classes.
  - Done when: every `Storage` method's `Result` is `Result<_, StorageError>` and a unit test exercises every variant via the in-memory double.
  - Feature: foundational.

### Backend / adapters crate

- [ ] **Implement `SqliteStorage` over `sqlx`.**
  - Acceptance: `crates/adapters/src/sqlite_storage.rs` MUST implement `Storage` using `sqlx` with the `sqlite` feature and a connection pool sized from configuration.
  - Done when: `cargo test -p trilithon-adapters sqlite_storage::tests` passes against a temporary database.
  - Feature: foundational.
- [ ] **Configure WAL and pragmas at pool initialisation.**
  - Acceptance: Pool initialisation MUST execute `PRAGMA journal_mode = WAL`, `PRAGMA synchronous = NORMAL`, `PRAGMA foreign_keys = ON`, and `PRAGMA busy_timeout = 5000`.
  - Done when: an integration test queries `PRAGMA journal_mode` and the other pragmas after pool start and asserts the values.
  - Feature: foundational (mitigates H14).
- [ ] **Embed migrations and run them at startup.**
  - Acceptance: Migrations MUST be embedded under `crates/adapters/migrations/` and applied via `sqlx::migrate!` at daemon startup.
  - Done when: the daemon emits `storage.migrations.applied` with the resulting schema version on first run.
  - Feature: foundational.
- [ ] **Author migration `0001_init.sql`.**
  - Acceptance: Migration `0001_init.sql` MUST create `snapshots`, `audit_log`, `sessions`, `users`, `mutations`, `secrets_metadata`, and `caddy_instances`, every table carrying a `caddy_instance_id` column hard-coded to `local`. The table is named `mutations` (singular `mutations`, not `mutations_queue`); architecture §6.7 is the canonical name.
  - Done when: the seven tables exist after first run and `cargo test -p trilithon-adapters migrations::initial_schema` passes.
  - Feature: T1.2, T1.7, T1.15 (substrate).
- [ ] **Refuse downgrade migrations.**
  - Acceptance: A migration runner MUST refuse to start the daemon if the database schema version is newer than the embedded migration set.
  - Done when: a fixture database tagged with a future version triggers exit code `3` and a typed error.
  - Feature: foundational.
- [ ] **Periodic `PRAGMA integrity_check` task.**
  - Acceptance: A task scheduled every six hours MUST run `PRAGMA integrity_check` and emit a tracing event `storage.integrity_check.failed` on any non-`ok` result, satisfying H14.
  - Done when: a unit test simulating a non-`ok` result asserts the tracing event and an integration test exercises the schedule.
  - Feature: foundational.
- [ ] **Advisory single-instance lock on the database file.**
  - Acceptance: Pool initialisation MUST acquire an advisory lock; a second daemon process pointed at the same database MUST be rejected before any write occurs.
  - Done when: an integration test launching a second daemon against the same `data_dir` observes the structured "another Trilithon may be running" error.
  - Feature: foundational.

### Database migrations

- [ ] **Document the up-only migration policy.**
  - Acceptance: `crates/adapters/migrations/README.md` MUST state that migrations are up-only, never edited after application, and that each schema change is a new migration.
  - Done when: the README exists and is referenced from the project's documentation index.
  - Feature: foundational.
- [ ] **Add a `caddy_instance_id` default of `local` everywhere.**
  - Acceptance: Every table created in `0001_init.sql` MUST carry `caddy_instance_id TEXT NOT NULL DEFAULT 'local'`.
  - Done when: a schema-introspection test asserts the column on every table.
  - Feature: keeps T3.1 reachable.

### Tests

- [ ] **Trait-contract unit tests via in-memory double.**
  - Acceptance: Unit tests MUST cover every `Storage` method through the in-memory double.
  - Done when: `cargo test -p trilithon-core storage::tests::contract` passes.
  - Feature: foundational.
- [ ] **Integration tests against a temporary SQLite database.**
  - Acceptance: Integration tests in `crates/adapters/tests/` MUST cover the SQLite implementation, exercising every method.
  - Done when: `cargo test -p trilithon-adapters` passes.
  - Feature: foundational.
- [ ] **Startup-failure exit code for storage failure.**
  - Acceptance: The daemon MUST exit `3` if SQLite cannot acquire the database file or if migrations fail.
  - Done when: an integration test with an unwritable database directory observes exit `3`.
  - Feature: foundational.

### Documentation

- [ ] **Document the persistence layer in `core/README.md`.**
  - Acceptance: A "Persistence" section MUST describe WAL, the migration directory, and the integrity-check schedule.
  - Done when: the section is present and references migration `0001_init.sql`.
  - Feature: foundational.

## Cross-references

- ADR-0006 (SQLite as V1 persistence layer).
- ADR-0009 (immutable content-addressed snapshots and audit log — substrate is created here).
- PRD T1.2 (snapshot history), T1.7 (audit log), T1.15 (secrets metadata).
- Architecture: "Persistence — SQLite," "Data model — Tier 1 tables," "Failure modes — SQLite corruption."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] The daemon runs migrations on startup and emits `storage.migrations.applied` with the resulting schema version.
- [ ] The daemon exits `3` if SQLite cannot acquire the database file or if migrations fail.
- [ ] All seven Tier 1 tables exist after first run, store time as UTC Unix seconds where time is stored, and satisfy H6.
- [ ] A second daemon process pointed at the same database file is rejected by an advisory lock check before any write occurs.
