# Phase 8 — Fixed Findings

**Run date:** 2026-05-13
**Total fixed:** 15

| ID | Severity | Title | File | Commit | Date |
|----|----------|-------|------|--------|------|
| F001 | CRITICAL | DETECTED_AT_BYPASSES_CLOCK | `adapters/src/drift.rs` | 97d5f42 | 2026-05-12 |
| F002 | CRITICAL | INIT_FROM_STORAGE_NEVER_CALLED | `cli/src/run.rs` | 97d5f42 | 2026-05-12 |
| F003 | HIGH | DEFER_MAPS_TO_ROLLEDBACK | `adapters/src/drift.rs` | 97d5f42 | 2026-05-12 |
| F004 | HIGH | FRAGILE_RESOLUTION_DESERIALIZATION | `adapters/src/sqlite_storage.rs` | 97d5f42 | 2026-05-12 |
| F005 | SUGGESTION | RESOLVE_SILENTLY_SUCCEEDS | `adapters/src/sqlite_storage.rs` | 97d5f42 | 2026-05-12 |
| F006 | HIGH | IGNORED_COUNT_DOUBLE_COUNTS | `core/src/diff.rs` | 97d5f42 | 2026-05-12 |
| F007 | WARNING | MISLEADING_ATOMICITY_COMMENT | `adapters/src/drift.rs` | 97d5f42 | 2026-05-12 |
| F008 | CRITICAL | DRIFT_DETECTOR_DESERIALIZES_WRONG_SCHEMA | `adapters/src/drift.rs` | 7adeb31 | 2026-05-12 |
| F009 | HIGH | MISSING_CLI_INTEGRATION_TEST | `cli/tests/drift_task_registered_at_startup.rs` | a81d952 | 2026-05-13 |
| F011 | HIGH | APPLY_MUTEX_NOT_SHARED | `cli/src/run.rs` | 583877b | 2026-05-13 |
| F012 | HIGH | BOX_LEAK_STATIC_REFS | `adapters/src/audit_writer.rs`, `cli/src/run.rs` | 583877b | 2026-05-13 |
| F014 | WARNING | INSTANCE_ID_UNUSED_IN_QUERY | `adapters/src/sqlite_storage.rs` | e582e7e | 2026-05-13 |
| F015 | WARNING | RESOLVE_NO_UNIQUE_CONSTRAINT | `adapters/migrations/0010_drift_correlation_unique.sql` | e582e7e | 2026-05-13 |
| F016 | WARNING | POINTER_REMOVE_IGNORES_OOB | `core/src/diff.rs` | e582e7e | 2026-05-13 |
| F017 | WARNING | OBJECTKIND_DEAD_VARIANTS | `core/src/diff.rs` | e582e7e | 2026-05-13 |
| F019 | WARNING | DIFF_JSON_STORED_WITHOUT_REDACTION | `adapters/src/drift.rs`, `adapters/src/audit_writer.rs` | e582e7e | 2026-05-13 |
| F022 | SUGGESTION | INITIAL_TICK_ON_SHUTDOWN | `adapters/src/drift.rs` | 957e551 | 2026-05-13 |
| F023 | SUGGESTION | DIFF_IS_EMPTY_DOC | `core/src/diff.rs` | 957e551 | 2026-05-13 |
