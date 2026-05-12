## Slice 8.1
- **Pre-existing gate failure: `caddy_sentinel_e2e`** — `trilithon-adapters` test `caddy_sentinel_e2e` fails to compile with `can't find crate for trilithon_core` / `can't find crate for tokio`. Added in Phase 3 (commit e55bb18); not caused by Slice 8.1. The test in `crates/adapters/Cargo.toml` lacks a `required-features` guard. Deferred to a cleanup slice.

## End-of-Phase Review Findings (10 reviewers, deduplicated)

### CRITICAL
1. **DETECTED_AT_BYPASSES_CLOCK** (glm, kimi, qwen, code_adversarial) — `tick_once()` uses `time::OffsetDateTime::now_utc()` instead of `self.clock` for `detected_at`. Breaks test determinism.
2. **MISSING_CLI_INTEGRATION_TEST** (scope_guardian) — `drift_task_registered_at_startup` test required by TODO does not exist.
3. **DRIFT_DETECTOR_DESERIALIZES_WRONG_SCHEMA** (codex) — `get_running_config()` returns raw Caddy JSON but code deserializes into `DesiredState`.

### HIGH
4. **INIT_FROM_STORAGE_NEVER_CALLED** (codex, glm, kimi, code_adversarial — 4 reviewers) — `init_from_storage()` implemented but never invoked at startup.
5. **APPLY_MUTEX_NOT_SHARED** (scope_guardian, code_adversarial) — `apply_mutex` created inside `build_drift_detector`, never shared with actual apply path.
6. **NON_ATOMIC_DUAL_WRITE** (code_adversarial, security) — `record()` writes audit row then drift row as separate operations.
7. **DEFER_MAPS_TO_ROLLEDBACK** (codex, qwen, code_adversarial, security) — `ResolutionKind::Defer` mapped to `DriftResolution::RolledBack`.
8. **BOX_LEAK** (code_adversarial, security, scope_guardian, glm) — `Box::leak` for `SchemaRegistry`/`Sha256AuditHasher` in `build_drift_detector`.
9. **ADOPT_MUTATION_OCC_GUARD** (codex) — `adopt_running_state` sets `expected_version` from `running_state.version` instead of current desired-state version.
10. **FRAGILE_RESOLUTION_DESERIALIZATION** (kimi, qwen, glm) — `format!("\"{r}\"")` pattern with `.ok()` for resolution deser.

### WARNING
11. **INSTANCE_ID_UNUSED** (glm, qwen, kimi, code_adversarial, security, minimax, scope_guardian — 7 reviewers) — `latest_unresolved_drift_event` ignores `instance_id` parameter.
12. **OBJECTKIND_DEAD_VARIANTS** (codex, kimi) — `ObjectKind::Upstream`/`Policy` never returned by `classify`.
13. **IGNORED_COUNT_DOUBLE_COUNTS** (kimi) — `ignored_count` incremented in both before/after loops.
14. **DIFF_JSON_STORED_WITHOUT_REDACTION** (security) — Diff JSON stored without passing through `SecretsRedactor`.
15. **RESOLVE_NO_UNIQUE_CONSTRAINT** (glm) — UPDATE by `correlation_id` without unique constraint.
16. **INTERVAL_NOT_WIRED_FROM_SETTINGS** (scope_guardian) — Interval hardcoded to 60s.
17. **MISLEADING_ATOMICITY_COMMENT** (qwen) — Comment claims atomic writes but they are independent async calls.
18. **POINTER_REMOVE_IGNORES_OOB** (kimi) — `pointer_remove` silently ignores out-of-bounds array indices.

### SUGGESTION
19. **RESOLVE_SILENTLY_SUCCEEDS** (code_adversarial) — `resolve_drift_event` returns `Ok(())` when zero rows updated.
20. **PUBLIC_TEST_MODULE** (qwen) — Test module in `resolve.rs` declared `pub`.
21. **DUPLICATE_ROW_MAPPING** (glm) — Identical row-mapping closures in two methods.
22. **TICKERROR_ERASES_TYPES** (kimi) — `TickError` variants use `String`, discarding original error types.
23. **INITIAL_TICK_ON_SHUTDOWN** (kimi) — `run()` executes one tick even if shutdown already signaled.
