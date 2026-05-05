# Phase 5 — GLM Review Findings

**Reviewer:** glm
**Date:** 2026-05-05
**Diff range:** d4a320a..HEAD
**Phase:** 5

---

[WARNING] IN_MEMORY_STORAGE_DEADLOCK_RISK
File: core/crates/core/src/storage/in_memory.rs
Lines: 67-69 vs 139-141
Description: `insert_snapshot` acquires locks in order snapshots → latest_ptr, but `latest_desired_state` acquires latest_ptr → snapshots. Two tokio tasks interleaving these methods can produce ABBA deadlock.
Suggestion: Pick one lock ordering and apply it consistently. Add a comment at each acquisition site documenting the ordering.

[WARNING] IN_MEMORY_DUPLICATE_DIVERGENCE
File: core/crates/core/src/storage/in_memory.rs
Lines: 70-75
Description: `InMemoryStorage::insert_snapshot` returns `Err(SnapshotDuplicate)` for any pre-existing id, but `SqliteStorage::insert_snapshot` returns `Ok(SnapshotId)` when the duplicate body is byte-equal. Tests using `InMemoryStorage` will see errors that production code would not.
Suggestion: Add a body-equality check in `InMemoryStorage` matching the SqliteStorage idempotency logic.

[WARNING] INTENT_PRIVACY_DOC_MISMATCH
File: core/crates/core/src/storage/types.rs
Lines: 62-68
Description: The doc comment on `Snapshot::intent` states "the field is intentionally private to the serialiser" and references a `Snapshot::new` constructor, but `intent` is declared `pub` and no `Snapshot::new` method exists.
Suggestion: Implement a Snapshot builder/constructor that validates intent, or update the doc comment.

[WARNING] SUPPRESSION_MISSING_TRACKED_ID
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 307, 345
Description: `#[allow(clippy::cast_sign_loss)]` and `#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]` use `// reason:` comments but lack the required `zd:<id> expires:<YYYY-MM-DD> reason:<short>` format mandated by the project constitution.
Suggestion: Add a tracked id and expiry date, e.g. `// zd:P5-001 expires:2027-01-01 reason: ...`.

[SUGGESTION] DUPLICATE_CONTENT_ADDRESS
File: core/crates/core/src/canonical_json.rs
Lines: general
Description: Two `content_address` functions exist with different signatures (`&DesiredState` vs `&[u8]`) performing the same SHA-256 hashing. If the algorithm changes, both must be updated independently.
Suggestion: Unify to a single canonical entry point.

[SUGGESTION] MONOTONIC_NANOS_SEMANTIC_CONFUSION
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 309-311, 344-347
Description: `created_at_monotonic_nanos` is read from `created_at_ms` (epoch milliseconds) by multiplying × 1_000_000. The field name implies a monotonic clock source but values actually encode wall-clock epoch milliseconds scaled to nanoseconds.
Suggestion: Rename the Rust field to reflect what's actually stored, or add a mapping comment explaining the legacy column.

[SUGGESTION] HARDCODED_LOCAL_INSTANCE_ID
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 389, 411
Description: The monotonicity check and INSERT both hardcode `caddy_instance_id = 'local'` rather than binding it from the Snapshot struct. The Snapshot struct no longer carries caddy_instance_id.
Suggestion: Add a brief comment on the struct noting caddy_instance_id is intentionally omitted for V1, or bind the value from a field if multi-instance is planned.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-05 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | IN_MEMORY_STORAGE_DEADLOCK_RISK | ✅ Fixed | 9c9fa93 | — | 2026-05-05 | F014: latest_desired_state now acquires snapshots → latest_ptr (same order as insert_snapshot) |
| 2 | IN_MEMORY_DUPLICATE_DIVERGENCE | ✅ Fixed | pre-review | — | 2026-05-05 | in_memory.rs already checks body equality on duplicate id |
| 3 | INTENT_PRIVACY_DOC_MISMATCH | ✅ Fixed | pre-review | — | 2026-05-05 | Doc comment updated; validate_snapshot_invariants enforces at write path |
| 4 | SUPPRESSION_MISSING_TRACKED_ID | ✅ Fixed | pre-review | — | 2026-05-05 | All suppressions already have zd:phase-05 expires:... format |
| 5 | DUPLICATE_CONTENT_ADDRESS | 🚫 Won't Fix | — | — | — | Functions serve different purposes (DesiredState vs bytes); not duplication |
| 6 | MONOTONIC_NANOS_SEMANTIC_CONFUSION | ✅ Fixed | 9c9fa93 | — | 2026-05-05 | F008: doc comment corrected to reflect wall-clock ms basis |
| 7 | HARDCODED_LOCAL_INSTANCE_ID | 🚫 Won't Fix | — | — | — | V1 single-instance design; documented inline with ADR-0009 references |
