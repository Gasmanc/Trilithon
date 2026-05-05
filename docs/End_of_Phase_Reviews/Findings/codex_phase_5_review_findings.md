# Phase 5 — Codex Review Findings

**Reviewer:** codex
**Date:** 2026-05-05
**Diff range:** d4a320a..HEAD
**Phase:** 5

---

[CRITICAL] MONOTONICITY_CHECK_IS_RACEABLE
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 349-401
Description: `insert_snapshot` uses a default deferred transaction, reads `MAX(config_version)`, then inserts. Two concurrent writers can observe the same max and both pass the check, allowing a later commit with a lower `config_version` than an already-committed row, violating strict monotonicity.
Suggestion: Acquire a write lock before the read-check-insert sequence (BEGIN IMMEDIATE / equivalent in sqlx) or move monotonicity enforcement into a DB-level constraint/trigger.

[HIGH] CANONICALIZER_CAN_MUTATE_LARGE_INTEGER_VALUES
File: core/crates/core/src/canonical_json.rs
Lines: 77-86
Description: Number normalization goes through `as_f64` and then casts back to `i64`. Large integers above IEEE-754 exact range (2^53) can be rounded, so canonicalization can change numeric values instead of only changing representation.
Suggestion: Avoid float round-tripping. Keep integer `serde_json::Number` values as integers via `is_i64`/`is_u64`, and only normalize decimal forms using a lossless strategy.

[WARNING] BROKEN_ADR_LINK_IN_CORE_README
File: core/README.md
Lines: 111
Description: The link target `docs/adr/0009-...` is relative to `core/README.md`, so it resolves to `core/docs/adr/...` (nonexistent) instead of the repository-level `docs/adr/...`.
Suggestion: Update the link to `../docs/adr/0009-immutable-content-addressed-snapshots-and-audit-log.md`.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-05 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | MONOTONICITY_CHECK_IS_RACEABLE | ✅ Fixed | pre-review | — | 2026-05-05 | BEGIN IMMEDIATE already in place in insert_snapshot_inner |
| 2 | CANONICALIZER_CAN_MUTATE_LARGE_INTEGER_VALUES | ✅ Fixed | pre-review | — | 2026-05-05 | is_f64() guard already in canonical_json.rs |
| 3 | BROKEN_ADR_LINK_IN_CORE_README | ✅ Fixed | pre-review | — | 2026-05-05 | Link already uses ../docs/adr/0009-... |
