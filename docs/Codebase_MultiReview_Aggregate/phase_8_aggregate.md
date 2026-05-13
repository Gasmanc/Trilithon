# Phase 8 — Aggregate Review Plan

**Generated:** 2026-05-12T00:00:00Z  
**Reviewers:** code_adversarial, codex, gemini (clean), glm, kimi, learnings_match, minimax, qwen, scope_guardian, security  
**Raw findings:** 45 across 9 reviewers (gemini: 0)  
**Unique findings:** 26 after clustering  
**Consensus:** 4 unanimous · 5 majority · 17 single-reviewer  
**Conflicts:** 0  
**Superseded (fixed in partial remediation 97d5f42):** 7

---

## How to use this document

Feed this file to `/review-remediate` to drive the fix cycle. Each finding has a
unique ID (F001, F002, …) that `review-remediate` uses to track state. Do not
renumber or delete findings — append `SUPERSEDED` status instead.

---

## CRITICAL Findings

### F001 · [CRITICAL] DETECTED_AT_BYPASSES_CLOCK
**Consensus:** UNANIMOUS · flagged by: glm (CRITICAL), kimi (HIGH), qwen (HIGH), code_adversarial (WARNING)  
**File:** `core/crates/adapters/src/drift.rs` · **Lines:** 260  
**Description:** `tick_once()` used `time::OffsetDateTime::now_utc()` instead of injected `self.clock` for `detected_at`, breaking test determinism.  
**Suggestion:** Replace with `self.clock.now_unix_ms() / 1_000`.  
**Claude's assessment:** Agree — injected clock is pointless if bypassed in the main hot path. Test determinism is critical.  
**Status:** ✅ SUPERSEDED — fixed in commit 97d5f42 (partial remediation)

---

### F008 · [CRITICAL] DRIFT_DETECTOR_DESERIALIZES_WRONG_SCHEMA
**Consensus:** MAJORITY · flagged by: codex (CRITICAL), qwen (HIGH as CADDY_JSON_TO_DESIRED_STATE_ROUNDTRIP_RISK)  
**File:** `core/crates/adapters/src/drift.rs` · **Lines:** 205–214  
**Description:** `get_running_config()` returns raw Caddy JSON, but the code deserializes it into `DesiredState`. Real Caddy payloads do not match `DesiredState` field layout, so drift ticks fail with deserialization errors in production — detection silently never executes.  
**Suggestion:** Compare like-for-like Caddy JSON structures, or introduce a dedicated `CaddyConfig` diff path. At minimum add a round-trip test that exercises real Caddy JSON.  
**Claude's assessment:** Agree at CRITICAL severity. If the deserialization silently fails with `.ok()`, we get a false "no drift" signal — which is worse than an error.

---

### F009 · [CRITICAL] MISSING_CLI_INTEGRATION_TEST
**Consensus:** SINGLE · flagged by: scope_guardian (CRITICAL)  
**File:** `core/crates/cli/tests/` · **Lines:** general  
**Description:** Slice 8.5 TODO required a `drift_task_registered_at_startup` CLI integration test as a hard exit condition. The test does not exist.  
**Suggestion:** Add `core/crates/cli/tests/drift_task_registered_at_startup.rs` per spec.  
**Claude's assessment:** Agree. The TODO's explicit "hard exit condition" language makes this a must-fix before phase is considered done. Confirmed: file is absent.

---

## HIGH Findings

### F002 · [HIGH] INIT_FROM_STORAGE_NEVER_CALLED
**Consensus:** UNANIMOUS · flagged by: code_adversarial (DEDUP_HASH_NOT_INITIALISED_AT_STARTUP), codex (RESTART_DEDUP_INITIALIZATION_IS_NEVER_CALLED), glm, kimi  
**File:** `core/crates/cli/src/run.rs` · **Lines:** 172–178  
**Description:** `init_from_storage()` was implemented but never called at daemon startup. `last_running_hash` always reset to `None` on restart.  
**Suggestion:** Call `detector.init_from_storage().await` before spawning the drift loop.  
**Claude's assessment:** Agree.  
**Status:** ✅ SUPERSEDED — fixed in commit 97d5f42 (partial remediation)

---

### F003 · [HIGH] DEFER_MAPS_TO_ROLLEDBACK
**Consensus:** UNANIMOUS · flagged by: qwen (HIGH), code_adversarial (WARNING), codex (WARNING), security (SUGGESTION)  
**File:** `core/crates/adapters/src/drift.rs` · **Lines:** 349–353  
**Description:** `ResolutionKind::Defer` was mapped to `DriftResolution::RolledBack`, collapsing two semantically distinct outcomes in the audit trail.  
**Suggestion:** Add `DriftResolution::Deferred` variant and map correctly.  
**Claude's assessment:** Agree.  
**Status:** ✅ SUPERSEDED — fixed in commit 97d5f42 (partial remediation)

---

### F004 · [HIGH] FRAGILE_RESOLUTION_DESERIALIZATION
**Consensus:** MAJORITY · flagged by: kimi (HIGH), glm (WARNING), qwen (WARNING)  
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 971, 998  
**Description:** Resolution deserialization used `format!("\"{r}\"")` round-trip with `.ok()`, silently returning `None` on any unknown variant.  
**Suggestion:** Exhaustive match or `FromStr` impl.  
**Claude's assessment:** Agree.  
**Status:** ✅ SUPERSEDED — fixed in commit 97d5f42 (partial remediation)

---

### F010 · [HIGH] NON_ATOMIC_DUAL_WRITE_IN_RECORD
**Consensus:** MAJORITY · flagged by: code_adversarial (HIGH), security (WARNING)  
**File:** `core/crates/adapters/src/drift.rs` · **Lines:** 280–335  
**Description:** `record()` writes the audit row then the drift row as two separate async operations. A crash between the two leaves an orphan audit row with no corresponding drift event, corrupting the audit trail.  
**Suggestion:** Wrap both writes in a SQLite transaction, or document the trade-off explicitly.  
**Claude's assessment:** Agree at HIGH. Orphan audit rows would make audit queries misleading. A transaction is the right fix; the mutex guard gives an illusion of atomicity but doesn't provide it.

---

### F011 · [HIGH] APPLY_MUTEX_NOT_SHARED
**Consensus:** MAJORITY · flagged by: code_adversarial (WARNING), scope_guardian (HIGH)  
**File:** `core/crates/cli/src/run.rs` · **Lines:** 239–271  
**Description:** `apply_mutex` is created inside `build_drift_detector` but never passed to `CaddyApplier`. As a result, `SkippedApplyInFlight` can never trigger — the mutex is never held during actual apply.  
**Suggestion:** Create `apply_mutex` at `run_with_shutdown` level and pass the same `Arc<Mutex>` into both the applier and the detector.  
**Claude's assessment:** Agree at HIGH. This makes the in-flight skip logic dead code, which was a key correctness property of slice 8.5.

---

### F012 · [HIGH] BOX_LEAK_STATIC_REFS
**Consensus:** UNANIMOUS · flagged by: code_adversarial (HIGH, MEMORY_LEAK_VIA_BOX_LEAK), glm (SUGGESTION, BOX_LEAK), scope_guardian (WARNING, BOX_LEAK_FOR_STATIC_REFS), security (WARNING, BOX_LEAK_MEMORY)  
**File:** `core/crates/cli/src/run.rs` · **Lines:** 249–252  
**Description:** `Box::leak` is used to create `&'static` references for `SchemaRegistry` and `Sha256AuditHasher`. These allocations are never reclaimed and grow on every daemon restart in a test harness.  
**Suggestion:** Use `Arc` wrappers and pass `Arc::clone` to consumers. `SchemaRegistry` likely already exists in `run_with_shutdown` scope — share that instance.  
**Claude's assessment:** Agree. `Box::leak` should not appear in a long-running daemon path.

---

### F013 · [HIGH] ADOPT_MUTATION_OCC_GUARD_WRONG_VERSION
**Consensus:** SINGLE · flagged by: codex (HIGH)  
**File:** `core/crates/core/src/diff/resolve.rs` · **Lines:** 79–81  
**Description:** `adopt_running_state` sets `expected_version` from `running_state.version` (the running config's version) instead of the current desired-state version. Optimistic concurrency should guard against concurrent writes to the desired state, not the running config.  
**Suggestion:** Pass the current desired-state version into `adopt_running_state` and use that for `expected_version`.  
**Claude's assessment:** Agree at HIGH. Using the wrong version for OCC means concurrent desired-state updates go undetected, allowing lost updates.

---

## WARNING Findings

### F006 · [WARNING] IGNORED_COUNT_DOUBLE_COUNTS
**Consensus:** SINGLE · flagged by: kimi  
**File:** `core/crates/core/src/diff.rs` · **Lines:** 213–214, 231–232  
**Description:** `ignored_count` was incremented in both the before and after loops, double-counting paths present in both.  
**Status:** ✅ SUPERSEDED — fixed in commit 97d5f42 (partial remediation)

---

### F007 · [WARNING] MISLEADING_ATOMICITY_COMMENT
**Consensus:** SINGLE · flagged by: qwen  
**File:** `core/crates/adapters/src/drift.rs` · **Lines:** 277–279  
**Status:** ✅ SUPERSEDED — fixed in commit 97d5f42 (partial remediation)

---

### F014 · [WARNING] INSTANCE_ID_UNUSED_IN_QUERY
**Consensus:** UNANIMOUS (7 reviewers) · flagged by: code_adversarial, codex, glm, kimi, minimax, qwen, security  
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 977–985  
**Description:** `latest_unresolved_drift_event` accepts an `instance_id` parameter but the SQL query never filters by it. All instances share drift event state.  
**Suggestion:** Add `caddy_instance_id` column to `drift_events` and filter queries by instance, OR document that this is intentionally single-instance and remove the parameter.  
**Claude's assessment:** Agree at WARNING. The API is misleading. If multi-instance is not in scope, the parameter should be removed to avoid implying isolation that doesn't exist.

---

### F015 · [WARNING] RESOLVE_NO_UNIQUE_CONSTRAINT
**Consensus:** MAJORITY · flagged by: glm (WARNING), qwen (WARNING as RESOLVE_DRIFT_EVENT_MULTI_ROW)  
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 1004–1026  
**Description:** `resolve_drift_event` updates by `correlation_id` without a unique constraint on that column. If duplicates exist (e.g., due to bug), multiple rows update silently.  
**Suggestion:** Add a `UNIQUE` index on `correlation_id` in the migration, and verify `rows_affected() == 1` (not just `> 0`).  
**Claude's assessment:** Agree. The `rows_affected() > 0` check added in F005 (partial remediation) is necessary but not sufficient without a uniqueness guarantee.

---

### F016 · [WARNING] POINTER_REMOVE_IGNORES_OOB
**Consensus:** SINGLE · flagged by: kimi  
**File:** `core/crates/core/src/diff.rs` · **Lines:** 493–495  
**Description:** `pointer_remove` silently ignores out-of-bounds array indices rather than returning an error.  
**Suggestion:** Return `DiffError::MissingParentPath` for OOB indices.  
**Claude's assessment:** Agree at WARNING. Silent ignore masks bugs in callers.

---

### F017 · [WARNING] OBJECTKIND_DEAD_VARIANTS
**Consensus:** MAJORITY · flagged by: kimi, codex  
**File:** `core/crates/core/src/diff.rs` · **Lines:** 640–673  
**Description:** `ObjectKind::Upstream` and `ObjectKind::Policy` variants are defined but `classify()` never returns either one — dead code that misleads readers about what the classifier handles.  
**Suggestion:** Either add classification patterns for upstream/policy paths, or remove the dead variants (or mark them with a comment explaining they are reserved for a future phase).  
**Claude's assessment:** Agree at WARNING. Dead enum variants that appear to represent real state are a maintenance hazard.

---

### F018 · [WARNING] INTERVAL_NOT_WIRED_FROM_SETTINGS
**Consensus:** SINGLE · flagged by: scope_guardian  
**File:** `core/crates/cli/src/run.rs` · **Lines:** 258–261  
**Description:** Drift check interval is hardcoded to 60s in `build_drift_detector` despite the TODO requiring it to be configuration-overridable.  
**Suggestion:** Wire `drift_interval_secs` from `DaemonConfig` / `settings.toml` and pass into `DriftDetector`.  
**Claude's assessment:** Agree at WARNING. The TODO spec called this out explicitly.

---

### F019 · [WARNING] DIFF_JSON_STORED_WITHOUT_REDACTION
**Consensus:** SINGLE · flagged by: security  
**File:** `core/crates/adapters/src/drift.rs` · **Lines:** 251–254  
**Description:** Diff JSON is stored in `drift_events` without passing through `SecretsRedactor`, potentially persisting plaintext secrets from Caddy config values.  
**Suggestion:** Route diff JSON through `SecretsRedactor` before persisting.  
**Claude's assessment:** Agree at WARNING. Caddy configs can contain TLS private keys and API tokens.

---

## SUGGESTION Findings

### F005 · [SUGGESTION] RESOLVE_SILENTLY_SUCCEEDS
**Consensus:** SINGLE · flagged by: code_adversarial  
**Status:** ✅ SUPERSEDED — fixed in commit 97d5f42 (partial remediation, promoted to check rows_affected)

---

### F020 · [SUGGESTION] DUPLICATE_ROW_MAPPING
**Consensus:** SINGLE · flagged by: glm  
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 963–974, 990–1001  
**Description:** Identical row-mapping closures duplicated in two query methods.  
**Suggestion:** Extract a shared `map_drift_row` helper.  
**Claude's assessment:** Minor but agree. Three-use rule is nearly met.

---

### F021 · [SUGGESTION] TICKERROR_ERASES_TYPES
**Consensus:** SINGLE · flagged by: kimi  
**File:** `core/crates/adapters/src/drift.rs` · **Lines:** 84–98  
**Description:** `TickError` variants use `String` fields, discarding the original error types and making programmatic error handling impossible.  
**Suggestion:** Wrap original error types (e.g. `StorageError`, `io::Error`) instead of `.to_string()`.  
**Claude's assessment:** Agree in principle. Thiserror makes this easy.

---

### F022 · [SUGGESTION] INITIAL_TICK_ON_SHUTDOWN
**Consensus:** SINGLE · flagged by: kimi  
**File:** `core/crates/adapters/src/drift.rs` · **Lines:** 141–158  
**Description:** `run()` executes one tick before checking shutdown signal, wasting work if shutdown was already signaled.  
**Suggestion:** Check `shutdown.borrow()` before first tick.  
**Claude's assessment:** Mild. Only relevant if shutdown races startup extremely tightly.

---

### F023 · [SUGGESTION] DIFF_IS_EMPTY_DOC
**Consensus:** SINGLE · flagged by: qwen  
**File:** `core/crates/core/src/diff.rs` · **Lines:** 117  
**Description:** `Diff::is_empty()` ignores `ignored_count`. Doc comment should clarify.  
**Suggestion:** Add doc comment clarifying that ignored paths are excluded from the emptiness check.  
**Claude's assessment:** Agree, trivial fix.

---

### F024 · [SUGGESTION] PUBLIC_TEST_MODULE
**Consensus:** SINGLE · flagged by: qwen  
**File:** `core/crates/core/src/diff/resolve.rs` · **Lines:** 136  
**Description:** Test module declared `pub mod tests` instead of `mod tests`.  
**Suggestion:** Change to `mod tests`.  
**Claude's assessment:** Agree, trivial.

---

## Out-of-scope / Superseded

| ID | Title | Reason |
|----|-------|--------|
| F001 | DETECTED_AT_BYPASSES_CLOCK | Fixed in partial remediation commit 97d5f42 |
| F002 | INIT_FROM_STORAGE_NEVER_CALLED | Fixed in partial remediation commit 97d5f42 |
| F003 | DEFER_MAPS_TO_ROLLEDBACK | Fixed in partial remediation commit 97d5f42 |
| F004 | FRAGILE_RESOLUTION_DESERIALIZATION | Fixed in partial remediation commit 97d5f42 |
| F005 | RESOLVE_SILENTLY_SUCCEEDS | Fixed in partial remediation commit 97d5f42 |
| F006 | IGNORED_COUNT_DOUBLE_COUNTS | Fixed in partial remediation commit 97d5f42 |
| F007 | MISLEADING_ATOMICITY_COMMENT | Fixed in partial remediation commit 97d5f42 |
| — | MISSING_8_6_TESTS_IN_DIFF (scope_guardian WARNING) | Verified: all 8.6 adapter tests present. Superseded by F009 (missing CLI test) |
| — | CADDY_DIFF_ENGINE_TRAIT_SPLIT (scope_guardian SUGGESTION) | Out-of-scope for remediation — design note only |
| — | REGEX_VERSION_PIN (security SUGGESTION) | security reviewer noted "no action needed" |
| — | Learnings match patterns (3 WARNING) | Informational — patterns apply but no code change required; reviewed |

---

## Summary statistics

| Severity | Unanimous | Majority | Single | Total |
|----------|-----------|----------|--------|-------|
| CRITICAL | 0 | 1 | 1 | 2 |
| HIGH | 2 | 2 | 1 | 5 |
| WARNING | 1 | 2 | 4 | 7 |
| SUGGESTION | 0 | 0 | 4 | 4 |
| **Total** | **3** | **5** | **10** | **18** |

*(7 findings superseded/already fixed excluded from counts above)*
