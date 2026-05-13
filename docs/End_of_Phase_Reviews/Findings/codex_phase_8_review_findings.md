# Phase 8 — Codex Review Findings

**Reviewer:** codex
**Date:** 2026-05-11
**Diff range:** 4402d00..HEAD
**Phase:** 8

---

[CRITICAL] DRIFT_DETECTOR_DESERIALIZES_WRONG_SCHEMA
File: core/crates/adapters/src/drift.rs
Lines: 205-214
Description: get_running_config() returns rendered Caddy JSON, but the code deserializes it into DesiredState. Real Caddy payloads do not match DesiredState fields, so ticks fail with serialization errors and drift detection never executes in production.
Suggestion: Compare like-for-like Caddy JSON structures or introduce a dedicated Caddy-config diff path.

[HIGH] RESTART_DEDUP_INITIALIZATION_IS_NEVER_CALLED
File: core/crates/cli/src/run.rs
Lines: 172-178
Description: The detector's restart dedup bootstrap (init_from_storage) is implemented but never invoked before run(). After every daemon restart, last_running_hash resets to None.
Suggestion: Call detector.init_from_storage().await before spawning the drift loop.

[HIGH] ADOPT_MUTATION_USES_RUNNING_VERSION_AS_OCC_GUARD
File: core/crates/core/src/diff/resolve.rs
Lines: 79-81
Description: adopt_running_state sets expected_version from running_state.version, but optimistic concurrency should be guarded by the current desired-state version.
Suggestion: Pass the current desired-state version into adopt_running_state and use that for expected_version.

[WARNING] DEFER_RESOLUTION_IS_PERSISTED_AS_ROLLBACK
File: core/crates/adapters/src/drift.rs
Lines: 349-353
Description: ResolutionKind::Defer is mapped to DriftResolution::RolledBack, collapsing two distinct outcomes.
Suggestion: Add a dedicated persisted resolution variant for defer.

[WARNING] INSTANCE_SCOPING_FOR_UNRESOLVED_DRIFT_IS_NOT_IMPLEMENTED
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 977-985
Description: latest_unresolved_drift_event ignores instance_id; the query is global.
Suggestion: Add caddy_instance_id to drift_events and filter queries by instance.

[WARNING] DIFF_SUMMARY_CLASSIFIER_HAS_UNREACHABLE_BUCKETS
File: core/crates/core/src/diff.rs
Lines: 640-673
Description: ObjectKind defines Upstream and Policy but classify never returns either variant.
Suggestion: Add classification patterns for upstream and policy paths.
