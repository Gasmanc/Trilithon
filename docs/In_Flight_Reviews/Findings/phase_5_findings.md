---
id: duplicate:area::phase-5-findings:legacy-uncategorized
category: duplicate
kind: process
location:
  area: phase-5-findings
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

## Tests
**Status:** complete
**Date:** 2026-05-05
**Summary:** Added three tests per the Phase 5 spec: a 50-entry canonicalisation corpus (insertion-order and float-normalisation variants all hash identically), a loop-based strict-monotonicity property test for `config_version` across 30 sequential writes including rejection of equal/lower versions, and a root-snapshot NULL parent assertion that verifies the first snapshot has `parent_id IS NULL` and the second has a non-null parent.
### Simplify Findings
Fixed inline: removed redundant `.clone()` call in `addr_via_float_json`; replaced redundant closure `|v| v.as_i64()` with `Value::as_i64` method reference; fixed `format!` argument inlining; added backticks to doc identifiers (`config_version`, `caddy_instance_id`, `SqliteStorage`).
### Items Fixed Inline
- Redundant `.clone()` on `canonical_value` removed (clippy::redundant_clone)
- Redundant closure `|v| v.as_i64()` → `Value::as_i64` (clippy::redundant_closure_for_method_calls)
- `format!("{:0>64x}", i)` → `format!("{i:0>64x}")` (clippy::uninlined_format_args)
- Doc comments: added backticks to `config_version`, `caddy_instance_id`, `SqliteStorage`
### Items Left Unfixed
none

## Backend / adapters crate
**Status:** complete
**Date:** 2026-05-05
**Summary:** Implemented SnapshotWriter functionality integrated into SqliteStorage — enforces parent existence, strict-monotonic config_version, and byte-equal body verification on hash match, all inside a single SQLite transaction. Added three new StorageError variants (SnapshotParentNotFound, SnapshotVersionNotMonotonic, SnapshotHashCollision) to core. Added fetch_by_config_version, fetch_by_parent_id, and fetch_by_date_range fetch operations as inherent methods on SqliteStorage. Integration tests in tests/snapshot.rs cover all acceptance criteria (deduplication, collision detection, parent enforcement, monotonicity, and all four fetch shapes).
### Simplify Findings
Fixed inline: moved use crate::lock::LockHandle import above SnapshotDateRange struct definition (import ordering); fixed multiple doc comments missing backticks around config_version, parent_id identifiers (clippy::doc_markdown).
### Items Fixed Inline
- Import ordering: LockHandle use statement moved above SnapshotDateRange struct
- Doc comments: backticks added to `config_version` and `parent_id` references in error.rs and snapshot.rs
- Updated existing sqlite_storage.rs test `insert_duplicate_different_body_returns_duplicate_error` to expect `SnapshotHashCollision` instead of `SnapshotDuplicate` (the new error is more semantically precise)
### Items Left Unfixed
none

## Backend / core crate
**Status:** complete
**Date:** 2026-05-05
**Summary:** Implemented canonical JSON serialiser with lexicographic key sorting and numeric normalisation, versioned via CANONICAL_JSON_VERSION=1. Defined the Snapshot record type with all T1.2-spec fields including 4 KiB intent bound. Added content_address helper (SHA-256 hex) and MutationId integration. Adapter layer updated to map legacy DB columns to new field names.
### Simplify Findings
Fixed inline: removed unused imports (GlobalConfig, TlsConfig), made validate_intent const, fixed cast allow-list to cover cast_precision_loss and cast_possible_wrap in adapters, changed pub mod tests to mod tests, added missing_docs allow, added backtick to doc comment.
### Items Fixed Inline
- Unused imports GlobalConfig and TlsConfig in canonical_json tests
- pub mod tests → mod tests (not public) with missing_docs allow
- validate_intent promoted to const fn
- cast_precision_loss suppression added alongside cast_possible_truncation in canonicalise_value
- cast_possible_wrap suppression added in sqlite_storage adapter
- Backtick added to "DesiredState" in fixture_state doc comment
### Items Left Unfixed
none

## Database migrations
**Status:** complete
**Date:** 2026-05-05
**Summary:** Authored migration 0004_snapshots_immutable.sql adding BEFORE UPDATE and BEFORE DELETE triggers on the snapshots table, each calling RAISE(ABORT, ...) to enforce immutability at the database layer per ADR-0009. Integration tests in tests/snapshot.rs verify that both UPDATE and DELETE on snapshot rows return a database-level error.
### Simplify Findings
nothing flagged
### Items Fixed Inline
none
### Items Left Unfixed
none

## Documentation
**Status:** complete
**Date:** 2026-05-05
**Summary:** Added a "Snapshots" section to `core/README.md` covering canonical JSON (lexicographic key sort, numeric normalisation, `CANONICAL_JSON_VERSION`), content addressing (SHA-256 hex, 64-char `snapshot_id`), parent linkage (`parent_id` chain, `NULL` root), and the immutability guarantee (`BEFORE UPDATE`/`BEFORE DELETE` triggers from migration 0004). Section references ADR-0009.
### Simplify Findings
nothing flagged
### Items Fixed Inline
none
### Items Left Unfixed
none
