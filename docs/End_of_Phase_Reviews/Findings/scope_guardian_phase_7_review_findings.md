# Phase 7 — Scope Guardian Review Findings

**Reviewer:** scope_guardian
**Date:** 2026-05-10
**Diff range:** ddda146..HEAD
**Phase:** 7

---

[WARNING] MIGRATION FILENAME MISMATCH IN TODO SPEC
File: `core/crates/adapters/migrations/0007_apply_locks.sql`
Lines: general
Description: Slice 7.6 in the TODO specifies the migration file as `core/crates/adapters/migrations/0004_apply_locks.sql`. The actual file created is `0007_apply_locks.sql`. Migration `0004_snapshots_immutable.sql` already existed, making `0004` an invalid name. The implementation chose the correct sequential name (`0007`), but the TODO spec contains a stale filename.
Suggestion: No code change needed. Update the TODO spec filename from `0004_apply_locks.sql` to `0007_apply_locks.sql` to match reality.

[WARNING] `validate()` PLACEHOLDER USES WRONG RETURN PATH VS TODO SPEC
File: `core/crates/adapters/src/applier_caddy.rs`
Lines: 564–567
Description: The TODO spec explicitly states the `validate()` placeholder should return `ApplyError::PreflightFailed { failures: vec![] }` as an `Err`. The implementation instead returns `Ok(ValidationReport::default())`. Returning `Ok` signals "valid" to callers, whereas returning `Err(PreflightFailed)` would signal "not implemented, treat as failure."
Suggestion: Either add `PreflightFailed { failures: Vec<ValidationFailure> }` to `ApplyError` and return it from `validate()`, or document in a code comment why `Ok(ValidationReport::default())` is the correct placeholder behavior for Phase 9 callers.

[WARNING] `InMemoryStorage::current_config_version` READS MAX(config_version) INSTEAD OF APPLIED VERSION
File: `core/crates/core/src/storage/in_memory.rs`
Lines: 307–318
Description: The SQLite production implementation reads `caddy_instances.applied_config_version` — a separately tracked column that only advances on a successful apply. The `InMemoryStorage` implementation reads `MAX(snapshots.config_version)`, which is always >= the applied version because snapshots are inserted before `apply()` is called. Tests using `InMemoryStorage` will observe different version baseline than production.
Suggestion: Add an explicit `applied_config_version: i64` field (per-instance) to `InMemoryStorage` and advance it only in `cas_advance_config_version` on success, mirroring the SQLite semantic exactly.

[WARNING] `notes_to_string` AND `sort_keys` DUPLICATED BETWEEN TWO MODULES
File: `core/crates/adapters/src/tls_observer.rs` and `core/crates/adapters/src/applier_caddy.rs`
Lines: `tls_observer.rs:61–80`, `applier_caddy.rs:93–113`
Description: Identical `notes_to_string` and `sort_keys` helper functions appear in both modules. Two identical copies — the second is scope creep relative to what the TODO requires.
Suggestion: Extract `notes_to_string` and `sort_keys` into a private `audit_notes` helper module within the `adapters` crate and import from both callers.

[SUGGESTION] `NoOpDiffEngine` PLACED IN `core` RATHER THAN `adapters`
File: `core/crates/core/src/diff.rs`
Lines: 64–103
Description: `NoOpDiffEngine` — a stub that always reports no differences — is also placed in `core`. Stubs and no-op test helpers conventionally belong in the layer that needs them (adapters tests), not in pure `core`. This sets a precedent of stub types in the production core library.
Suggestion: Move `NoOpDiffEngine` to a `#[cfg(test)]` module within `core/src/diff.rs` or to a test-utilities file in `adapters`.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-13 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Seams not written to seams-proposed.md | ✅ Fixed | `af38262` | — | 2026-05-13 | |
| 2 | InMemoryStorage CAS reads MAX(snapshots) | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 3 | Duplicate sort_keys + notes_to_string | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 4 | validate() returns Ok instead of PreflightFailed | ✅ Fixed | `569b149` | — | 2026-05-13 | Doc clarification added |
| 5 | Migration filename mismatch | 🔕 Superseded | — | — | — | Doc-only fix, out of aggregate scope |
| 6 | NoOpDiffEngine in core (move to #[cfg(test)]) | 🔕 Superseded | — | — | — | Design preference, deferred as low priority |
