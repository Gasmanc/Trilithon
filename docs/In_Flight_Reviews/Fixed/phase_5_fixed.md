---
id: duplicate:area::phase-5-fixed-items:legacy-uncategorized
category: duplicate
kind: process
location:
  area: phase-5-fixed-items
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

# Phase 5 Fixed Items

| Unit | Issue | Fix |
|------|-------|-----|
| Tests | Redundant `.clone()` on `canonical_value` in `addr_via_float_json` | Removed (clippy::redundant_clone) |
| Tests | Redundant closure `\|v\| v.as_i64()` | Replaced with `Value::as_i64` method reference |
| Tests | `format!("{:0>64x}", i)` not inlined | Changed to `format!("{i:0>64x}")` |
| Tests | Doc comments missing backticks on `config_version`, `caddy_instance_id`, `SqliteStorage` | Added backticks |
|------|-------|-----|
| Backend / adapters crate | Import ordering: LockHandle use above SnapshotDateRange struct | Moved import above struct definition |
| Backend / adapters crate | Doc comments missing backticks around `config_version` and `parent_id` | Added backticks to all affected doc comments |
| Backend / adapters crate | Existing test expected `SnapshotDuplicate` for same-id-different-body collision | Updated test to expect more precise `SnapshotHashCollision` error |
| Backend / core crate | Unused imports GlobalConfig and TlsConfig in canonical_json tests | Removed |
| Backend / core crate | pub mod tests → mod tests (not public) with missing_docs allow | Changed |
| Backend / core crate | validate_intent promoted to const fn | Promoted |
| Backend / core crate | cast_precision_loss suppression missing in canonicalise_value | Added |
| Backend / core crate | cast_possible_wrap suppression missing in sqlite_storage adapter | Added |
| Backend / core crate | Backtick missing on "DesiredState" in fixture_state doc comment | Added |
|------|-------|-----|
| multi-review | Canonicalizer Corrupts Large Integers — core/crates/core/src/canonical_json.rs | Multi-review | 2026-05-05 | 78f5954 |
| multi-review | MONOTONICITY_CHECK_IS_RACEABLE — core/crates/adapters/src/sqlite_storage.rs | Multi-review | 2026-05-05 | 78f5954 |
| multi-review | Missing content-hash validation on snapshot insert — core/crates/adapters/src/sqlite_storage.rs | Multi-review | 2026-05-05 | 78f5954 |
| multi-review | InMemoryStorage DIVERGES FROM SqliteStorage ON DUPLICATE SEMANTICS — core/crates/core/src/storage/in_memory.rs | Multi-review | 2026-05-05 | 78f5954 |
| multi-review | DEDUPLICATION PATH RETURNS EARLY INSIDE AN OPEN TRANSACTION — core/crates/adapters/src/sqlite_storage.rs | Multi-review | 2026-05-05 | 78f5954 |
| multi-review | BROKEN_ADR_LINK_IN_CORE_README — core/README.md | Multi-review | 2026-05-05 | 78f5954 |
| multi-review | INTENT FIELD BOUND NOT ENFORCED AT WRITE PATH — core/crates/adapters/src/sqlite_storage.rs | Multi-review | 2026-05-05 | 78f5954 |
| multi-review | SUPPRESSION_MISSING_TRACKED_ID — core/crates/adapters/src/sqlite_storage.rs | Multi-review | 2026-05-05 | 78f5954 |
| multi-review | fetch_by_date_range SQL BUILT WITH format! — STRUCTURALLY FRAGILE — core/crates/adapters/src/sqlite_storage.rs | Multi-review | 2026-05-05 | 78f5954 |
| multi-review | INTENT_PRIVACY_DOC_MISMATCH — core/crates/core/src/storage/types.rs | Multi-review | 2026-05-05 | 78f5954 |
| multi-review | caddy_instance_id hardcoded without comment — core/crates/adapters/src/sqlite_storage.rs | Multi-review | 2026-05-05 | 78f5954 |
| multi-review | let _ = parse_actor_kind discards value awkwardly — core/crates/adapters/src/sqlite_storage.rs | Multi-review | 2026-05-05 | 78f5954 |
