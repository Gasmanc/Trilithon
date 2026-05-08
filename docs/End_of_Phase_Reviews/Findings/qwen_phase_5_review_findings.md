---
id: duplicate:area::phase-5-qwen-review-findings:legacy-uncategorized
category: duplicate
kind: process
location:
  area: phase-5-qwen-review-findings
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

# Phase 5 — Qwen Review Findings

**Reviewer:** qwen
**Date:** 2026-05-05
**Diff range:** d4a320a..HEAD
**Phase:** 5

---

[WARNING] created_at_monotonic_nanos semantic mismatch
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 321-325
Description: The DB column `created_at_ms` stores wall-clock epoch milliseconds. The code converts it to nanoseconds (* 1_000_000) and stores in `created_at_monotonic_nanos`, a field documented as "a monotonic nanosecond counter". Converting ms to ns does not make a wall-clock timestamp monotonic.
Suggestion: Either rename the field to something like `created_at_nanos` (drop "monotonic"), or store a genuine monotonic timestamp in a separate column.

[WARNING] caddy_instance_id hardcoded to 'local' in both write and read paths
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 389, 411
Description: `insert_snapshot` hardcodes `caddy_instance_id = 'local'` in the INSERT and the monotonicity check. The `Snapshot` type has no `caddy_instance_id` field, so there is no way to pass a different instance. When multi-instance support arrives, the monotonicity check will falsely reject or accept snapshots from other instances.
Suggestion: Add `caddy_instance_id: String` to `Snapshot`, bind it in the INSERT, and parameterise the monotonicity query.

[SUGGESTION] `let _ = parse_actor_kind(...)` discards parsed value awkwardly
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 305-306
Description: `let _ = parse_actor_kind(&actor_kind_str)?` parses for validation only but the intent is unclear. The `?` applies to the Result and `let _` discards the `ActorKind`.
Suggestion: Use `parse_actor_kind(&actor_kind_str)?;` (without `let _`) to make it clear the parsed value is intentionally unused.

[SUGGESTION] canonical_json_version defaults to current constant on every read
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 327
Description: `row_to_snapshot` always sets `canonical_json_version` to `CANONICAL_JSON_VERSION` because there is no DB column. When the format changes and the constant is incremented, all historical rows will be misreported as using the new format.
Suggestion: Add a migration to store `canonical_json_version` in a DB column before any format change.

[SUGGESTION] InMemoryStorage diverges from SqliteStorage on duplicate handling semantics
File: core/crates/core/src/storage/in_memory.rs
Lines: 67-93
Description: `InMemoryStorage::insert_snapshot` always returns `StorageError::SnapshotDuplicate` when the id already exists. `SqliteStorage::insert_snapshot` returns the existing id when bodies are equal (idempotent). Tests against `InMemoryStorage` will observe different behavior.
Suggestion: Align `InMemoryStorage` with `SqliteStorage` by checking body equality on duplicate id.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-05 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | created_at_monotonic_nanos semantic mismatch | ✅ Fixed | 9c9fa93 | — | 2026-05-05 | F008: doc comment corrected to reflect wall-clock ms basis |
| 2 | caddy_instance_id hardcoded to 'local' in both write and read paths | 🚫 Won't Fix | — | — | — | V1 single-instance design; documented inline with ADR-0009 references |
| 3 | let _ = parse_actor_kind(...) discards parsed value awkwardly | ✅ Fixed | pre-review | — | 2026-05-05 | Already uses statement form parse_actor_kind(&actor_kind_str)? |
| 4 | canonical_json_version defaults to current constant on every read | ✅ Fixed | 9c9fa93 | — | 2026-05-05 | F006: migration 0005 adds column; row_to_snapshot reads it |
| 5 | InMemoryStorage diverges from SqliteStorage on duplicate handling | ✅ Fixed | pre-review | — | 2026-05-05 | in_memory.rs already checks body equality on duplicate id |
