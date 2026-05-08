---
id: contract-drift:area::phase-2-scope-guardian-review-findings:legacy-uncategorized
category: contract-drift
kind: process
location:
  area: phase-2-scope-guardian-review-findings
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

# Phase 2 — Scope Guardian Review Findings

**Reviewer:** scope_guardian
**Date:** 2026-05-07
**Diff range:** be773df..cfba489
**Phase:** 2

---

[CRITICAL] `AuditEventRow` missing `prev_hash` field
File: `core/crates/core/src/storage/types.rs`
Lines: 134–169
Description: `AuditEventRow` is defined without a `prev_hash` field. Slice 2.1 specifies the field `pub prev_hash: String` as part of the canonical struct shape from trait-signatures.md §1 ("`SHA-256 of previous row's canonical JSON (or all-zero for first row); ADR-0009`"). The audit_log DDL also lacks the `prev_hash` column. Without this field, the ADR-0009 hash chain cannot exist in the type system at all.
Suggestion: Add `pub prev_hash: String` to `AuditEventRow` in `types.rs`. Add `prev_hash TEXT NOT NULL DEFAULT '0000...0'` column to `audit_log` in `0001_init.sql`.

[CRITICAL] `core/crates/core/src/storage/helpers.rs` (`canonical_json_for_hash`) entirely absent
File: `core/crates/core/src/storage/`
Lines: general
Description: Slice 2.1 mandates creating `core/crates/core/src/storage/helpers.rs` with the `canonical_json_for_hash()` helper that both `InMemoryStorage` and `SqliteStorage` must call to compute `prev_hash`. The file does not exist. Without it the hash-chain tests required by slices 2.2 and 2.4 cannot compile or run.
Suggestion: Create `core/crates/core/src/storage/helpers.rs` with `pub fn canonical_json_for_hash(row: &AuditEventRow) -> String` using `BTreeMap` for deterministic key order.

[CRITICAL] `proposals` table absent from `0001_init.sql`
File: `core/crates/adapters/migrations/0001_init.sql`
Lines: 1–122
Description: Slice 2.3 requires eight tables: `caddy_instances`, `users`, `sessions`, `snapshots`, `audit_log`, `mutations`, `proposals`, `secrets_metadata`. The diff's `0001_init.sql` contains only seven — the `proposals` table is not present. `enqueue_proposal`, `dequeue_proposal`, and `expire_proposals` on `SqliteStorage` cannot function without this table.
Suggestion: Add the `proposals` DDL (with all columns from architecture §6.8, including `wildcard_callout`, `decided_*`, `wildcard_ack_*`, `resulting_mutation`) plus its indexes to `0001_init.sql`.

[CRITICAL] `schema_migrations` DDL present in migration file (forbidden by spec)
File: `core/crates/adapters/migrations/0001_init.sql`
Lines: 4–9
Description: The TODO states explicitly: "The `_sqlx_migrations` table is created by sqlx at runtime and MUST NOT be in the DDL." The diff introduces a `CREATE TABLE schema_migrations` in the migration file. This is a custom table sqlx will not recognise and produces an extra table with no writer.
Suggestion: Remove the `CREATE TABLE schema_migrations` block from `0001_init.sql`.

[CRITICAL] `snapshots` DDL missing required columns (`created_at_monotonic_ns`, `canonical_json_version`, self-cycle `CHECK`)
File: `core/crates/adapters/migrations/0001_init.sql`
Lines: 48–66
Description: Three items mandated by the phase checklist and architecture §6.5 are absent: (1) `created_at_monotonic_ns INTEGER NOT NULL`; (2) `canonical_json_version INTEGER NOT NULL DEFAULT 1`; (3) `CHECK (parent_id != id)` on `parent_id` to prevent self-cycles.
Suggestion: Add `created_at_monotonic_ns INTEGER NOT NULL`, `canonical_json_version INTEGER NOT NULL DEFAULT 1`, and `CHECK (parent_id != id)` to the `snapshots` DDL.

[CRITICAL] `audit_log` missing `prev_hash` column in DDL
File: `core/crates/adapters/migrations/0001_init.sql`
Lines: 68–89
Description: The spec requires `prev_hash TEXT NOT NULL DEFAULT '0000...0'` (64 zeros) in `audit_log`. The diff's `audit_log` DDL has no such column. This is the schema-level complement of the missing `prev_hash` in `AuditEventRow`.
Suggestion: Add `prev_hash TEXT NOT NULL DEFAULT '0000000000000000000000000000000000000000000000000000000000000000'` to the `audit_log` DDL.

[HIGH] `InMemoryStorage` uses `std::sync::Mutex` instead of `tokio::sync::Mutex`
File: `core/crates/core/src/storage/in_memory.rs`
Lines: 19, 38–44
Description: Slice 2.2 specifies `use tokio::sync::Mutex`. The diff uses `std::sync::Mutex` throughout. Holding a `std::sync::MutexGuard` across an `.await` point deadlocks the executor; the spec chose `tokio::sync::Mutex` to avoid poisoning and allow safe `.await` inside lock guards.
Suggestion: Replace `use std::sync::Mutex` with `use tokio::sync::Mutex` and change all `lock().expect(...)` calls to `.lock().await`.

[HIGH] `PRAGMA application_id` validation and post-migration set step absent
File: `core/crates/adapters/src/sqlite_storage.rs`, `core/crates/cli/src/run.rs`
Lines: general
Description: Slice 2.4 and 2.7 mandate: (a) in `SqliteStorage::open`, reading `PRAGMA application_id` and rejecting values other than `0` or `0x54525754`; (b) in `run.rs` after successful migration, executing `PRAGMA application_id = 0x54525754`. Neither step appears in the diff. The phase checklist requires this.
Suggestion: Add the `PRAGMA application_id` read-and-validate in `SqliteStorage::open`. Add the write step in `run.rs` after `apply_migrations` succeeds.

[HIGH] Startup synchronous integrity check not wired (`integrity_check_once` before `daemon.started`)
File: `core/crates/cli/src/run.rs`
Lines: 75–105
Description: Slice 2.6 algorithm and the phase checklist require a synchronous call to `integrity_check_once` before `daemon.started` is emitted. The diff's `run.rs` spawns the periodic task but does not call `integrity_check_once` at startup. `daemon.started` is emitted immediately after migration without the startup integrity gate.
Suggestion: After `apply_migrations` and before `daemon.started`, call `integrity_check_once(storage.pool()).await`; if `Failed { detail }`, log and return `ExitCode::StartupPreconditionFailure`.

[WARNING] `docs/phase-02-review.md` and `docs/phase-02-unfixed.md` added to the repo
File: `docs/phase-02-review.md`, `docs/phase-02-unfixed.md`
Lines: general
Description: These files are not referenced by any TODO work unit or the phase exit checklist. These appear to be reviewer artefacts (in-flight review outputs) that belong under `docs/In_Flight_Reviews/` or `docs/End_of_Phase_Reviews/`, not at the repo root.
Suggestion: Move these files to the established review artefacts directories or exclude them from the phase commit.

[WARNING] `ShutdownObserver` trait method renamed from `async fn wait` to `fn changed` without spec update
File: `core/crates/core/src/lifecycle.rs`
Lines: 10–19
Description: The TODO specifies `async fn wait(&mut self)` as the single method on `ShutdownObserver`. The diff defines `fn changed(&mut self) -> Pin<Box<dyn Future<...>>>` instead, and adds a second method `fn is_shutting_down(&self) -> bool`. The rename and the extra method are not mandated.
Suggestion: Either align the trait with the spec (`async fn wait(&mut self)` only) or document the divergence as an explicit architectural decision.
