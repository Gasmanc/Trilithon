# Phase 8 — Code Adversarial Review Findings

**Reviewer:** code_adversarial
**Date:** 2026-05-11
**Diff range:** 4402d00..HEAD
**Phase:** 8

---

[HIGH] MEMORY_LEAK_VIA_BOX_LEAK
File: core/crates/cli/src/run.rs
Lines: 243-249
Description: build_drift_detector uses Box::leak twice for static references that are never reclaimed.
Suggestion: Use Arc wrappers instead of &'static.

[HIGH] DEDUP_HASH_NOT_INITIALISED_AT_STARTUP
File: core/crates/adapters/src/drift.rs, core/crates/cli/src/run.rs
Lines: 398-417, 255-262
Description: init_from_storage() exists but is never called. last_running_hash always starts as None.
Suggestion: Call detector.init_from_storage().await before spawning run loop.

[HIGH] NON_ATOMIC_DUAL_WRITE_IN_RECORD
File: core/crates/adapters/src/drift.rs
Lines: 280-335
Description: record() writes audit row then drift row as two separate operations. Partial failure leaves orphan audit rows.
Suggestion: Wrap both writes in a SQLite transaction or make record() idempotent.

[WARNING] DEFER_MAPS_TO_ROLLEDBACK
File: core/crates/adapters/src/drift.rs
Lines: 349-353
Description: ResolutionKind::Defer mapped to DriftResolution::RolledBack — semantic mismatch.
Suggestion: Add DriftResolution::Deferred variant.

[WARNING] TICK_ONCE_BYPASSES_CLOCK
File: core/crates/adapters/src/drift.rs
Lines: 260
Description: detected_at uses now_utc() instead of injected self.clock.
Suggestion: Replace with self.clock.now_unix_ms() / 1_000.

[WARNING] INSTANCE_ID_IGNORED
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 968-985
Description: latest_unresolved_drift_event ignores instance_id parameter.
Suggestion: Add instance_id column or document limitation.

[WARNING] APPLY_MUTEX_NOT_SHARED
File: core/crates/cli/src/run.rs
Lines: 237-239
Description: apply_mutex created new in build_drift_detector, not shared with actual apply path.
Suggestion: Wire same mutex into applier code path.

[SUGGESTION] RESOLVE_SILENTLY_SUCCEEDS_ON_NONEXISTENT_ID
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 998-1015
Description: resolve_drift_event returns Ok(()) even when zero rows updated.
Suggestion: Check rows_affected() and return error if zero.
