# Phase 7 — Code Adversarial Review Findings

**Reviewer:** code_adversarial
**Date:** 2026-05-10
**Diff range:** ddda146..HEAD
**Phase:** 7

---

[CRITICAL] PHANTOM APPLIED VERSION ON PANIC OR 5XX AFTER CAS
File: `core/crates/adapters/src/applier_caddy.rs`
Lines: 452–466, 258–301
Description: The CAS gate (`cas_advance_config_version`) runs at Step 0 and unconditionally advances `applied_config_version` in the database before any Caddy I/O occurs. If anything subsequently fails — a panic inside `load_config`, a 5xx from Caddy, a failed equivalence check — the database pointer is permanently advanced while Caddy is still running the previous config. The panic test explicitly acknowledges this: "config_version is now 1 (the panicking apply did CAS-advance before panicking)."
Technique: Assumption Violation
Suggestion: Move the CAS advance to after `verify_equivalence` succeeds, or implement a compensating `cas_rollback_config_version` that is called in every early-exit path after the CAS has fired.

[CRITICAL] COMMIT FAILURE SILENTLY RETURNS Ok WHILE APPLIED VERSION STAYS UNCHANGED
File: `core/crates/adapters/src/sqlite_storage.rs`
Lines: 981–991
Description: After `advance_config_version_if_eq` returns `Ok(new_version)`, the outer `cas_advance_config_version` issues `COMMIT` with `let _ = ...`, discarding the result. If the COMMIT fails, the transaction is implicitly rolled back by SQLite, but the function has already returned `Ok(new_version)`. The applier proceeds as though the CAS succeeded, writes audit rows, pushes config to Caddy, and returns `ApplyOutcome::Succeeded` — while the database `applied_config_version` is actually unchanged.
Technique: Assumption Violation
Suggestion: Replace `let _ = sqlx::query("COMMIT")...` with `.map_err(sqlx_err)?` and propagate the COMMIT error as `StorageError`.

[HIGH] IN-MEMORY AND SQLITE CAS IMPLEMENTATIONS DIVERGE ON APPLIED-POINTER SEMANTICS
File: `core/crates/core/src/storage/in_memory.rs`, `core/crates/adapters/src/sqlite_storage.rs`
Lines: in_memory.rs cas_advance impl, sqlite_storage.rs lines 960–1000
Description: The `Storage` trait doc for `cas_advance_config_version` says "Read `MAX(config_version)` for `instance_id`." The SQLite implementation reads `applied_config_version`. The in-memory implementation reads `MAX(config_version)` across all inserted snapshots. These produce different conflict behaviour: in a test scenario where snapshot N+1 has been inserted but not applied, in-memory CAS with `expected=N` fails while SQLite CAS succeeds.
Technique: Composition Failure
Suggestion: Add a dedicated `applied_config_version: AtomicI64` to `InMemoryStorage`, update it only inside `cas_advance_config_version` when the CAS succeeds, and use it as the observed value.

[HIGH] ROLLBACK ALWAYS CONFLICTS WHEN TARGET IS NOT THE IMMEDIATELY PRIOR VERSION
File: `core/crates/adapters/src/applier_caddy.rs`
Lines: 569–581
Description: `rollback()` retrieves the target snapshot, sets `expected = snapshot.config_version`, then calls `self.apply(&snapshot, expected)`. The CAS gate compares `expected_version` against `applied_config_version`. If the current applied version is `N` and the rollback target is version `M < N-1`, `expected=M` never equals the DB value `N`. Rollback is structurally non-functional for any genuine reversion.
Technique: Assumption Violation
Suggestion: `rollback()` should bypass the CAS gate or use a dedicated `force_apply` path that writes `applied_config_version = target_snapshot.config_version` directly.

[HIGH] TLS OBSERVER IS DEAD CODE: EMPTY HOSTNAMES ALWAYS PASSED
File: `core/crates/adapters/src/applier_caddy.rs`, `core/crates/adapters/src/tls_observer.rs`
Lines: `applier_caddy.rs` line 524; `tls_observer.rs` lines 97–99
Description: The applier spawns the TLS observer with `observer.observe(correlation_id, vec![], Some(sid))`. The observer's first action is `if hostnames.is_empty() { return; }`. No TLS issuance audit row will ever be written, and no timeout row will ever be written. The entire TLS-state separation described in Slice 7.8 is inert.
Technique: Composition Failure
Suggestion: Extract hostnames from `desired_state` before spawning the observer, or remove the `hostnames.is_empty()` early-return and change the observer to poll unconditionally.

[HIGH] STALE LOCK REAP RACE: LockError::AlreadyHeld REPORTS OWN PID ON DOUBLE-RACE
File: `core/crates/adapters/src/storage_sqlite/locks.rs`
Lines: 214–226
Description: In `acquire_apply_lock`, when the first INSERT fails and the subsequent SELECT finds no row (row was deleted between INSERT and SELECT), the code retries the INSERT. If that second INSERT also fails, the code returns `LockError::AlreadyHeld { pid: holder_pid }` — where `holder_pid` is the caller's own PID, not the PID of the process that holds the lock. Operators investigating lock contention will see the daemon's own PID as the contender.
Technique: Composition Failure
Suggestion: After the second failed INSERT, perform a SELECT to retrieve the actual current `holder_pid` before constructing `LockError::AlreadyHeld`.

[HIGH] PROCESS_ALIVE USES SHELL `kill` COMMAND NOT SYSCALL — PID REUSE RACE
File: `core/crates/adapters/src/storage_sqlite/locks.rs`
Lines: 144–155
Description: `process_alive` forks a shell to run `/usr/bin/kill -0 <pid>`. Between the stale lock detection and the shell invocation, the dead process's PID can be recycled by an unrelated process. The shell `kill -0` will then report that PID as alive, preventing stale lock reaping. The lock row remains permanently until the new unrelated process also exits.
Technique: Assumption Violation
Suggestion: Use `nix::sys::signal::kill(Pid::from_raw(pid), Signal::try_from(0).ok())` instead of spawning a shell process.

[HIGH] ADVISORY LOCK DROP ON PANIC: SPAWN_BLOCKING TASK COMPLETES AFTER MUTEX RELEASES
File: `core/crates/adapters/src/storage_sqlite/locks.rs`, `core/crates/adapters/src/applier_caddy.rs`
Lines: `locks.rs` 100–125; `applier_caddy.rs` 432–444
Description: On panic inside the async apply block, Rust drops local variables in reverse declaration order: `advisory_lock` drops first. Its `Drop` impl calls `tokio::task::spawn_blocking(...)`, which submits a background task and returns immediately without awaiting it. Then `_process_guard` drops, releasing the in-process `Mutex`. A subsequent caller can acquire the `Mutex` and proceed to `acquire_apply_lock` before the `spawn_blocking` task has actually executed its `DELETE FROM apply_locks`.
Technique: Cascade Construction
Suggestion: Consider holding the `Mutex` guard inside the async block rather than outside it, and only releasing it after `advisory_lock.release().await` completes.

[WARNING] 5XX CADDY RESPONSE MAPPED TO `ApplyError::Storage`
File: `core/crates/adapters/src/applier_caddy.rs`
Lines: 301
Description: `Err(other_err)` in `load_or_fail` catches 5xx responses and maps them to `ApplyError::Storage(other_err.to_string())`. No audit row is written for 5xx failures. `ApplyFailureKind::CaddyServerError` exists but is never constructed.
Technique: Cascade Construction
Suggestion: Add a `status / 100 == 5` match arm, write an audit row with `error_kind = "CaddyServerError"`, and return `Ok(ApplyOutcome::Failed { kind: ApplyFailureKind::CaddyServerError, .. })`.

[WARNING] CONFLICT AUDIT NOTE USES HAND-ROLLED JSON, NOT `notes_to_string`
File: `core/crates/adapters/src/applier_caddy.rs`
Lines: 359
Description: `handle_conflict` builds its audit note with a `format!()` string literal rather than constructing an `ApplyAuditNotes` struct and calling `notes_to_string`. This bypasses the key-sorting path and diverges from every other audit row written by the applier.
Technique: Abuse Case
Suggestion: Add `stale_version` and `current_version` fields to `ApplyAuditNotes` and route through `notes_to_string`.

[WARNING] `BEGIN IMMEDIATE` DOUBLE-ISSUE IN `try_insert_lock` SILENTLY DEGRADES TO DEFERRED
File: `core/crates/adapters/src/storage_sqlite/locks.rs`
Lines: 281–300
Description: `try_insert_lock` calls `pool.begin()`, which starts a DEFERRED transaction, then issues `BEGIN IMMEDIATE` as a raw query. The error handler silently swallows "cannot start a transaction within a transaction" errors, leaving the transaction in DEFERRED mode. WAL mode does not make all transactions IMMEDIATE by default.
Technique: Assumption Violation
Suggestion: Use `sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn)` on a raw acquired connection (not `pool.begin()`). Remove the `pool.begin()` call entirely.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-13 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | CAS advance fires before Caddy load | ✅ Fixed | `36af1e7` | — | 2026-05-13 | |
| 2 | COMMIT result silently discarded | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 3 | try_insert_lock DEFERRED tx TOCTOU | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 4 | LockError::AlreadyHeld reports own PID | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 5 | process_alive shells out to PATH kill | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 6 | Advisory lock Drop on panic ordering | ✅ Fixed | `af38262` | — | 2026-05-13 | |
| 7 | Duplicate sort_keys + notes_to_string | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 8 | InMemoryStorage CAS reads MAX(snapshots) | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 9 | rollback() CAS uses wrong expected | ✅ Fixed | `36af1e7` | — | 2026-05-13 | |
| 10 | 5xx response mapped to Storage error | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 11 | Conflict note uses hand-rolled format! | ✅ Fixed | `569b149` | — | 2026-05-13 | |
