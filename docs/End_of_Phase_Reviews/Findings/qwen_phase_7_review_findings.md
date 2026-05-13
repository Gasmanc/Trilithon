# Phase 7 — Qwen Review Findings

**Reviewer:** qwen
**Date:** 2026-05-10
**Diff range:** ddda146..HEAD
**Phase:** 7

---

[CRITICAL] CONFLICT_OUTCOME_VERSIONS_SWAPPED
File: core/crates/adapters/src/applier_caddy.rs
Lines: 458-461
Description: `handle_conflict` is called with `(expected, observed)` but the parameters are `(stale_version, current_version)`. The `OptimisticConflict` error's `expected` field is what the *caller* expected (the stale value), and `observed` is what the DB actually has (the current value). This swap writes wrong version numbers into `ApplyOutcome::Conflicted` and the `mutation.conflicted` audit row, making conflict debugging and retry logic unreliable.
Suggestion: Swap the arguments to `self.handle_conflict(correlation_id, &snapshot_id, observed, expected)` — or rename `handle_conflict` parameters to `(expected_version, observed_version)` to match the error variant and call with the destructured names directly.

[CRITICAL] TRY_INSERT_LOCK_USES_DEFERRED_NOT_IMMEDIATE
File: core/crates/adapters/src/storage_sqlite/locks.rs
Lines: 280-299
Description: `try_insert_lock` calls `pool.begin().await` which creates a sqlx `SqliteTransaction` that has already issued `BEGIN` (deferred) on the connection. The subsequent `BEGIN IMMEDIATE` then fails with "cannot start a transaction within a transaction", which is silently swallowed. The INSERT therefore runs inside a deferred transaction, not an IMMEDIATE one. This breaks the TOCTOU protection that the code comment explicitly claims.
Suggestion: Use `pool.acquire().await` to get a raw connection, then `sqlx::query("BEGIN IMMEDIATE").execute(...)` to explicitly start the transaction. Do not use `pool.begin()` which interferes with the transaction mode.

[WARNING] TLS_OBSERVER_SPAWNED_WITH_EMPTY_HOSTNAMES
File: core/crates/adapters/src/applier_caddy.rs
Lines: 520-526
Description: The TLS observer is spawned with `vec![]` for hostnames. In `tls_observer.rs:95`, `observe` returns immediately if `hostnames.is_empty()`, making the TLS observer effectively a no-op for all callers.
Suggestion: Extract managed hostnames from `desired_state` (e.g. TLS-enabled virtual hosts) and pass them to the observer.

[WARNING] ROLLBACK_BYPASSES_CAS_BY_PASSING_CONFIG_VERSION
File: core/crates/adapters/src/applier_caddy.rs
Lines: 569-581
Description: `rollback` calls `self.apply(&snapshot, snapshot.config_version)`, passing the snapshot's own `config_version` as `expected_version`. Since `applied_config_version` was already advanced past this snapshot's version, the CAS will almost always conflict.
Suggestion: Either read the current applied version and pass it explicitly, or implement a dedicated rollback path that does not rely on CAS.

[SUGGESTION] DUPLICATE sort_keys FUNCTION
File: core/crates/adapters/src/applier_caddy.rs, core/crates/adapters/src/tls_observer.rs
Lines: applier_caddy.rs:102-114, tls_observer.rs:68-82
Description: `sort_keys` is defined identically in both modules, as is `notes_to_string`. Two of the "three uses before extracting" threshold.
Suggestion: Extract to a shared helper in `crate::audit_writer` or a small `json_util` module.

[SUGGESTION] ACQUIREDFLOCK_DROP_SPAWNS_NEW_RUNTIME
File: core/crates/adapters/src/storage_sqlite/locks.rs
Lines: 96-131
Description: `AcquiredLock::drop` constructs a fresh `CurrentThread` runtime via `tokio::runtime::Builder::new_current_thread().enable_all().build()` just to run a single DELETE query. This is heavyweight and the `mem::forget` in `release()` makes this path unreachable in the happy case.
Suggestion: Use `task::block_in_place` instead of spawning a blocking task with a nested runtime.

[SUGGESTION] COMMIT_RESULT_SILENTLY_DISCARDED
File: core/crates/adapters/src/sqlite_storage.rs
Lines: 990-995
Description: Both `COMMIT` and `ROLLBACK` results are discarded with `let _ = ...`. A failed COMMIT means the CAS "success" path returned `Ok(new_version)` but the transaction was never persisted.
Suggestion: Propagate the COMMIT result and map a failure to `StorageError`.

[SUGGESTION] BOUNDED_EXCERPT_CAN_EXCEED_LIMIT_WITH_ELLIPSIS
File: core/crates/adapters/src/applier_caddy.rs
Lines: 73-84
Description: When input exceeds `EXCERPT_MAX_BYTES` (512), the function truncates then appends the UTF-8 ellipsis (3 bytes). The result can be up to 515 bytes — exceeding the stated maximum.
Suggestion: Reserve 3 bytes for the ellipsis by truncating at `EXCERPT_MAX_BYTES - 3`.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-13 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | COMMIT result silently discarded | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 2 | TLS observer spawned with empty hostnames | ✅ Fixed | `36af1e7` | — | 2026-05-13 | |
| 3 | try_insert_lock DEFERRED tx TOCTOU | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 4 | rollback() CAS uses wrong expected | ✅ Fixed | `36af1e7` | — | 2026-05-13 | |
| 5 | AcquiredLock::drop spawns new runtime | 🚫 Won't Fix | — | — | — | Superseded by F012 fix (block_in_place) |
| 6 | bounded_excerpt 3 bytes over maximum | ✅ Fixed | `569b149` | — | 2026-05-13 | |
| 7 | Conflict outcome versions possibly swapped | 🚫 Won't Fix | — | — | — | Verified not a bug — mapping is correct |
