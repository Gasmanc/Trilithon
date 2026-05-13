# Phase 8 — Scope Guardian Review Findings

**Reviewer:** scope_guardian
**Date:** 2026-05-11
**Diff range:** 4402d00..HEAD
**Phase:** 8

---

[CRITICAL] MISSING_CLI_INTEGRATION_TEST
File: core/crates/cli/tests/
Lines: general
Description: Slice 8.5 requires drift_task_registered_at_startup test. TODO says this is a hard exit condition. No such test exists.
Suggestion: Add core/crates/cli/tests/drift_task_registered_at_startup.rs per spec.

[HIGH] APPLY_MUTEX_NOT_SHARED_WITH_APPLIER
File: core/crates/cli/src/run.rs
Lines: 239-271
Description: apply_mutex created inside build_drift_detector, never passed to CaddyApplier. SkippedApplyInFlight is dead code.
Suggestion: Create apply_mutex at run_with_shutdown level and share with both applier and detector.

[WARNING] BOX_LEAK_FOR_STATIC_REFS
File: core/crates/cli/src/run.rs
Lines: 249-252
Description: Box::leak creates static references instead of sharing existing instances.
Suggestion: Pass existing SchemaRegistry from run_with_shutdown.

[WARNING] MISSING_8_6_TESTS_IN_DIFF
File: core/crates/adapters/tests/
Lines: general
Description: Slice 8.6 tests may not all be in diff range. Verify all 8.6 acceptance tests exist and pass.
Suggestion: Confirm all 8.6 tests compile and pass.

[WARNING] INTERVAL_NOT_WIRED_FROM_SETTINGS
File: core/crates/cli/src/run.rs
Lines: 258-261
Description: Interval hardcoded to 60s despite spec requiring configuration-overridable.
Suggestion: Wire drift interval from DaemonConfig/settings.

[SUGGESTION] CADDY_DIFF_ENGINE_TRAIT_SPLIT
File: core/crates/core/src/diff.rs
Lines: 139, 613
Description: CaddyDiffEngine trait split alongside DiffEngine not in TODO but reasonable.
Suggestion: Document rationale.
