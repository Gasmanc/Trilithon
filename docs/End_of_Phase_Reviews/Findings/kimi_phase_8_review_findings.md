# Phase 8 — Kimi Review Findings

**Reviewer:** kimi
**Date:** 2026-05-11
**Diff range:** 4402d00..HEAD
**Phase:** 8

---

[HIGH] DETECTED_AT_BYPASSES_CLOCK
File: core/crates/adapters/src/drift.rs
Lines: 260
Description: tick_once calls time::OffsetDateTime::now_utc() instead of self.clock for detected_at.
Suggestion: Use self.clock.now_unix_ms() / 1_000.

[HIGH] FRAGILE_RESOLUTION_DESERIALIZATION
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 971, 998
Description: Resolution deserialization uses format!("\"{r}\"") pattern that silently fails with .ok().
Suggestion: Store as plain string with direct match or FromStr impl.

[HIGH] INIT_FROM_STORAGE_NEVER_CALLED
File: core/crates/cli/src/run.rs
Lines: 172-178
Description: init_from_storage() never called at startup, last_running_hash always starts as None.
Suggestion: Await detector.init_from_storage().await before spawning.

[WARNING] INSTANCE_ID_UNUSED
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 979
Description: latest_unresolved_drift_event ignores instance_id parameter.
Suggestion: Add WHERE clause or remove parameter.

[WARNING] POINTER_REMOVE_IGNORES_OOB
File: core/crates/core/src/diff.rs
Lines: 493-495
Description: pointer_remove silently ignores out-of-bounds array indices.
Suggestion: Return DiffError::MissingParentPath for OOB indices.

[WARNING] IGNORED_COUNT_DOUBLE_COUNTS
File: core/crates/core/src/diff.rs
Lines: 213-214, 231-232
Description: ignored_count incremented in both before and after loops, double-counting paths present in both.
Suggestion: Only count unique ignored paths.

[WARNING] OBJECTKIND_DEAD_VARIANTS
File: core/crates/core/src/diff.rs
Lines: 662-674
Description: ObjectKind::Upstream and Policy variants never produced by classify.
Suggestion: Add corresponding patterns or remove unused variants.

[SUGGESTION] TICKERROR_ERASES_TYPES
File: core/crates/adapters/src/drift.rs
Lines: 84-98
Description: TickError variants use String, discarding original error types.
Suggestion: Wrap original error types instead of .to_string().

[SUGGESTION] INITIAL_TICK_ON_SHUTDOWN
File: core/crates/adapters/src/drift.rs
Lines: 141-158
Description: run() executes one tick even if shutdown already signaled.
Suggestion: Check shutdown.borrow() before entering loop.
