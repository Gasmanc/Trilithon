# Phase 8 — Unfixed Findings

**Run date:** 2026-05-13
**Total unfixed:** 6 (3 deferred · 2 won't fix · 1 conflict pending)

| ID | Severity | Consensus | Title | File | Status | Reason |
|----|----------|-----------|-------|------|--------|--------|
| F010 | HIGH | MAJORITY | NON_ATOMIC_DUAL_WRITE_IN_RECORD | `adapters/src/drift.rs` | deferred | audit_log.record() uses BEGIN IMMEDIATE; nested transaction deadlocks. Documented with zd:F010. Proper fix requires split record_audit_event API. |
| F013 | HIGH | SINGLE | ADOPT_MUTATION_OCC_GUARD_WRONG_VERSION | `core/src/diff/resolve.rs` | wont_fix | Already fixed in partial remediation 97d5f42 — desired_version param added |
| F018 | WARNING | SINGLE | INTERVAL_NOT_WIRED_FROM_SETTINGS | `cli/src/run.rs` | deferred | Requires DaemonConfig schema change; deferred to Phase 9 config layer |
| F020 | SUGGESTION | SINGLE | DUPLICATE_ROW_MAPPING | `adapters/src/sqlite_storage.rs` | deferred | Three-use threshold not met; defer to next refactor pass |
| F021 | SUGGESTION | SINGLE | TICKERROR_ERASES_TYPES | `adapters/src/drift.rs` | deferred | TickError wrapping refactor deferred to avoid scope creep |
| F024 | SUGGESTION | SINGLE | PUBLIC_TEST_MODULE | `core/src/diff/resolve.rs` | wont_fix | Finding incorrect — test module is already private |
