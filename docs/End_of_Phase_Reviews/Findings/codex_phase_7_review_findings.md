# Phase 7 — Codex Review Findings

**Reviewer:** codex
**Date:** 2026-05-10
**Diff range:** ddda146..HEAD
**Phase:** 7

---

[CRITICAL] APPLIED_VERSION_ADVANCES_ON_FAILED_APPLY
File: core/crates/adapters/src/applier_caddy.rs
Lines: 448-507
Description: `cas_advance_config_version` is executed before `POST /load`, and failures/`ApplyOutcome::Failed` return without reverting. This advances `applied_config_version` even when Caddy rejects/unreachable, so storage no longer reflects the last successfully applied config.
Suggestion: Treat CAS as a check-only gate before load, then advance `applied_config_version` only after successful load + equivalence check (or run check+apply+advance in one transaction with rollback on failure).

[HIGH] CAS_DOES_NOT_VERIFY_TARGET_SNAPSHOT_VERSION
File: core/crates/adapters/src/storage_sqlite/snapshots.rs
Lines: 81-108
Description: `advance_config_version_if_eq` only checks snapshot existence, not that `new_snapshot_id` has `config_version == expected_version + 1`. This can advance the pointer to a version unrelated to the snapshot being applied.
Suggestion: Query `snapshots.config_version` for `new_snapshot_id` and return `StorageError::Integrity` unless it exactly matches `expected_version + 1`.

[HIGH] CADDY_5XX_PATH_IS_MISCLASSIFIED_AND_UNAUDITED
File: core/crates/adapters/src/applier_caddy.rs
Lines: 258-302
Description: Non-4xx `BadStatus` (including 5xx) falls into `Err(ApplyError::Storage(...))` with no `config.apply-failed` row and no `ApplyFailureKind::CaddyServerError` outcome, breaking failure taxonomy and terminal-audit behavior.
Suggestion: Add explicit 5xx handling that writes `config.apply-failed` with `error_kind = CaddyServerError` and returns `Ok(ApplyOutcome::Failed { kind: CaddyServerError, ... })`.

[HIGH] TLS_OBSERVER_IS_NEVER_ACTUALLY_TRIGGERED_FROM_APPLY
File: core/crates/adapters/src/applier_caddy.rs
Lines: 520-525
Description: `apply()` always calls `observer.observe(..., vec![], ...)`; observer immediately returns on empty hostnames, so TLS follow-up audit rows are never emitted in production apply flow.
Suggestion: Derive the managed hostname set from `desired_state` and pass it to `observe`; skip spawning only when that derived set is empty.

[WARNING] ROLLBACK_EXPECTED_VERSION_IS_INCORRECT
File: core/crates/adapters/src/applier_caddy.rs
Lines: 569-580
Description: `rollback()` uses `expected = snapshot.config_version` for CAS. Rolling back to an older snapshot will usually conflict because DB observed version is newer.
Suggestion: Read current applied version from storage for `expected_version`, then apply rollback target (or create a new rollback snapshot at `current+1` and apply that).

[WARNING] LOCK_CONTESTION_CAN_REPORT_THE_WRONG_PID
File: core/crates/adapters/src/storage_sqlite/locks.rs
Lines: 215-227
Description: In the `existing_pid == None` race branch, if retry still fails, it returns `LockError::AlreadyHeld { pid: holder_pid }` (the caller's PID), which is incorrect and misleading.
Suggestion: Re-read `holder_pid` from `apply_locks` after failed retry and return that PID (or a clear unknown sentinel if absent).

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-13 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | CAS advance fires before Caddy load | ✅ Fixed | `36af1e7` | — | 2026-05-13 | |
| 2 | COMMIT result silently discarded | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 3 | rollback() CAS uses wrong expected | ✅ Fixed | `36af1e7` | — | 2026-05-13 | |
| 4 | LockError::AlreadyHeld reports own PID | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 5 | 5xx response mapped to Storage error | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
