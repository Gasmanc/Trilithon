# Phase 5 — Kimi Review Findings

**Reviewer:** kimi
**Date:** 2026-05-05
**Diff range:** d4a320a..HEAD
**Phase:** 5

---

[HIGH] Missing content-hash validation on snapshot insert
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 338-433
Description: `Storage::insert_snapshot` documents that it must "reject the row if the content hash does not match the canonical-JSON SHA-256", but `SqliteStorage::insert_snapshot` never verifies that `snapshot.snapshot_id` is actually the SHA-256 of `snapshot.desired_state_json`. A caller can persist an arbitrary id for a given body, breaking content-addressing.
Suggestion: Compute the SHA-256 of `snapshot.desired_state_json` and return `StorageError::Integrity` when it does not equal `snapshot.snapshot_id.0`.

[HIGH] canonical_json_version not persisted to database
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 296-330, 404-428
Description: `Snapshot.canonical_json_version` exists in the Rust model and core/README.md states snapshot rows store this constant for future format migration detection. However, there is no DB column; `row_to_snapshot` overwrites every loaded row with the current constant.
Suggestion: Add a migration introducing `canonical_json_version INTEGER NOT NULL DEFAULT 1`, bind it on insert, and read it back in `row_to_snapshot`.

[WARNING] created_at_monotonic_nanos misnamed and loses sub-millisecond precision
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 296-330, 338-433
Description: `Snapshot.created_at_monotonic_nanos` is documented as a "monotonic nanosecond counter", but `row_to_snapshot` populates it by multiplying `created_at_ms` by 1_000_000. A true monotonic counter is not wall-clock time.
Suggestion: Rename the field to `created_at_epoch_nanos` to match actual data, or introduce a separate `monotonic_nanos` DB column.

[WARNING] Snapshot fetches are not scoped to caddy_instance_id
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 100-205, 510-526
Description: `insert_snapshot` hardcodes `caddy_instance_id = 'local'` and monotonicity only considers 'local', but fetch_by_config_version, fetch_by_parent_id, fetch_by_date_range, latest_desired_state, and parent_chain do not filter by instance.
Suggestion: Add `caddy_instance_id` to the `Snapshot` model and filter all fetch queries by it.

[WARNING] Large integer precision loss in canonical JSON
File: core/crates/core/src/canonical_json.rs
Lines: 74-88
Description: `canonicalise_value` tests whole-valued numbers via `n.as_f64()`, which loses integer precision for values greater than 2^53. An i64 field with value 9_007_199_254_740_993 will be silently truncated.
Suggestion: Check `n.as_i64()` and `n.as_u64()` before the f64 path so that all integers fitting in 64-bit types are preserved exactly.

[SUGGESTION] Snapshot::intent documentation contradicts implementation
File: core/crates/core/src/storage/types.rs
Lines: 62-68
Description: The doc comment on `intent` states the field is "intentionally private to the serialiser" and references a `Snapshot::new` constructor, but the field is `pub` and no `new` method exists.
Suggestion: Either make `intent` non-public and add a `Snapshot::new` constructor, or update the doc comment.

[SUGGESTION] SnapshotId accepts arbitrary strings
File: core/crates/core/src/storage/types.rs
Lines: 31-32
Description: `SnapshotId` is an unvalidated String wrapper with no enforcement of the 64-character lowercase-hex invariant.
Suggestion: Add a constructor or `TryFrom<String>` implementation validating the input is exactly 64 ASCII hex digits.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-05 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Missing content-hash validation on snapshot insert | ✅ Fixed | pre-review | — | 2026-05-05 | validate_snapshot_invariants already recomputes SHA-256 and checks |
| 2 | canonical_json_version not persisted to database | ✅ Fixed | 9c9fa93 | — | 2026-05-05 | F006: migration 0005 adds canonical_json_version column |
| 3 | created_at_monotonic_nanos misnamed and loses sub-millisecond precision | ✅ Fixed | 9c9fa93 | — | 2026-05-05 | F008: doc comment corrected |
| 4 | Snapshot fetches are not scoped to caddy_instance_id | 🚫 Won't Fix | — | — | — | V1 single-instance design; documented inline with ADR-0009 references |
| 5 | Large integer precision loss in canonical JSON | ✅ Fixed | pre-review | — | 2026-05-05 | is_f64() guard already in canonical_json.rs |
| 6 | Snapshot::intent documentation contradicts implementation | ✅ Fixed | pre-review | — | 2026-05-05 | Doc updated; enforcement at write path via validate_snapshot_invariants |
| 7 | SnapshotId accepts arbitrary strings | ✅ Fixed | 9c9fa93 | — | 2026-05-05 | F022: added SnapshotId::try_from_hex |
