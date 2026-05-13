# Phase 8 — GLM Review Findings

**Reviewer:** glm
**Date:** 2026-05-11
**Diff range:** 4402d00..HEAD
**Phase:** 8

---

[CRITICAL] DETECTED_AT_BYPASSES_CLOCK
File: core/crates/adapters/src/drift.rs
Lines: 260
Description: DriftEvent.detected_at uses time::OffsetDateTime::now_utc() instead of self.clock.
Suggestion: Replace with self.clock.now_unix_ms() / 1_000.

[HIGH] INIT_FROM_STORAGE_NEVER_CALLED
File: core/crates/cli/src/run.rs
Lines: 173-178
Description: DriftDetector::init_from_storage() is never called at startup.
Suggestion: Add detector.init_from_storage().await after construction.

[WARNING] INSTANCE_ID_UNUSED
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 977-1001
Description: _instance_id parameter unused in SQL query.
Suggestion: Add WHERE clause or remove parameter.

[WARNING] RESOLVE_NO_UNIQUE_CONSTRAINT
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 1004-1026
Description: UPDATE by correlation_id without unique constraint, silently updates zero or multiple rows.
Suggestion: Add unique index on correlation_id and check rows_affected().

[WARNING] FRAGILE_RESOLUTION_DESER
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 971, 998
Description: Resolution deserialization uses fragile format!("\"{r}\"") pattern with .ok().
Suggestion: Use serde_json::from_value(Value::String(r)) or explicit match.

[SUGGESTION] DUPLICATE_ROW_MAPPING
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 963-974, 990-1001
Description: Identical row-mapping closures in two methods.
Suggestion: Extract shared helper function.

[SUGGESTION] BOX_LEAK
File: core/crates/cli/src/run.rs
Lines: 249-252
Description: Box::leak for registry and hasher creates intentional memory leaks.
Suggestion: Use Arc instead.
