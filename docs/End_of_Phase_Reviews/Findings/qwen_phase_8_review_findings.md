# Phase 8 — Qwen Review Findings

**Reviewer:** qwen
**Date:** 2026-05-11
**Diff range:** 4402d00..HEAD
**Phase:** 8

---

[HIGH] DEFER_RESOLUTION_SEMANTIC_MISMATCH
File: core/crates/adapters/src/drift.rs
Lines: 352
Description: ResolutionKind::Defer maps to DriftResolution::RolledBack. Semantically distinct operations conflated.
Suggestion: Add Deferred variant to DriftResolution enum.

[HIGH] DETECTED_AT_BYPASSES_INJECTED_CLOCK
File: core/crates/adapters/src/drift.rs
Lines: 260
Description: tick_once uses time::OffsetDateTime::now_utc() instead of self.clock, breaking test determinism.
Suggestion: Replace with self.clock.now_unix_ms() / 1_000.

[HIGH] CADDY_JSON_TO_DESIRED_STATE_ROUNDTRIP_RISK
File: core/crates/adapters/src/drift.rs
Lines: 19
Description: DiffEngine operates on DesiredState pairs but tick_once parses raw Caddy JSON into DesiredState. Runtime-populated fields may not round-trip.
Suggestion: Document limitation or add round-trip test.

[WARNING] INSTANCE_ID_UNUSED_IN_QUERY
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 977-978
Description: latest_unresolved_drift_event takes instance_id but SQL doesn't filter by it.
Suggestion: Add instance_id column or document single-instance limitation.

[WARNING] FRAGILE_RESOLUTION_DESERIALIZATION
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 971, 998
Description: Resolution deserialization uses fragile format!("\"{r}\"") round-trip pattern.
Suggestion: Store variant as plain string with FromStr impl.

[WARNING] RESOLVE_DRIFT_EVENT_MULTI_ROW
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 1017
Description: resolve_drift_event updates by correlation_id which may match multiple rows.
Suggestion: Resolve by id (primary key) or document multi-row semantics.

[WARNING] MISLEADING_ATOMICITY_COMMENT
File: core/crates/adapters/src/drift.rs
Lines: 277-279
Description: Comment claims guard ensures atomic writes but audit and storage writes are independent async calls.
Suggestion: Remove misleading atomicity claim or wire through transactional boundary.

[SUGGESTION] DIFF_IS_EMPTY_DOC
File: core/crates/core/src/diff.rs
Lines: 117
Description: Diff::is_empty() ignores ignored_count. Doc should clarify.
Suggestion: Add doc comment clarifying ignored paths excluded.

[SUGGESTION] PUBLIC_TEST_MODULE
File: core/crates/core/src/diff/resolve.rs
Lines: 136
Description: Test module declared pub. Should not be public.
Suggestion: Change to mod tests.
