# Phase 7 — GLM Review Findings

**Reviewer:** glm
**Date:** 2026-05-10
**Diff range:** ddda146..HEAD
**Phase:** 7

---

[HIGH] TLS observer spawned with empty hostnames — feature is dead code
File: core/crates/adapters/src/applier_caddy.rs
Lines: 520-525
Description: The TLS observer is spawned with `vec![]` for hostnames, but `TlsIssuanceObserver::observe` returns immediately when `hostnames.is_empty()` (tls_observer.rs:94-97). The entire TLS-issuance observation from Slice 7.8 never executes — no follow-up audit rows are ever written, no timeouts are ever detected.
Suggestion: Either extract the actual managed hostnames from the rendered desired state and pass them to `observer.observe()`, or remove the `hostnames.is_empty()` early return in `observe()` and have it unconditionally poll for all pending certs.

[HIGH] InMemoryStorage CAS reads wrong counter — diverges from production semantics
File: core/crates/core/src/storage/in_memory.rs
Lines: 330-336
Description: `InMemoryStorage::cas_advance_config_version` reads `MAX(snapshots.config_version)` — the highest *inserted* snapshot version. The production `SqliteStorage` reads `caddy_instances.applied_config_version` — the last *applied* version. Because the mutation pipeline inserts snapshots before apply is called, `MAX(config_version)` is always >= the applied version. Any test using InMemoryStorage for CAS will observe different conflict behavior than production.
Suggestion: Add an `applied_config_version: Mutex<i64>` field to `InMemoryStorage` that mirrors the SQLite column. `current_config_version` and `cas_advance_config_version` should read/write this field instead of scanning snapshots.

[WARNING] Duplicate `sort_keys` and `notes_to_string` functions across two adapter files
File: core/crates/adapters/src/applier_caddy.rs (lines 93-114) and core/crates/adapters/src/tls_observer.rs (lines 61-82)
Description: Identical `notes_to_string` and `sort_keys` private functions copied between `applier_caddy.rs` and `tls_observer.rs`. Violates the project's "reuse before new code" rule.
Suggestion: Extract both functions to a shared location (e.g., a small `audit_notes` module in `adapters`) and call from both files.

[WARNING] CAS advance doesn't verify snapshot config_version matches expected_version + 1
File: core/crates/adapters/src/storage_sqlite/snapshots.rs
Lines: 82-98
Description: `advance_config_version_if_eq` only checks `SELECT COUNT(*) FROM snapshots WHERE id = ? AND caddy_instance_id = ?` — it does not validate the snapshot's `config_version`. A snapshot with a mismatched version would pass the existence check and advance the applied pointer to a version that doesn't correspond to the snapshot.
Suggestion: Change the query to also bind `config_version = expected_version + 1`, and return `StorageError::Integrity` if the count is zero.

[SUGGESTION] Advisory lock stale-reap DELETE and retry are not atomic
File: core/crates/adapters/src/storage_sqlite/locks.rs
Lines: 206-261
Description: Between reading the stale holder's PID, deleting the stale row, and retrying the INSERT, another process can win the race. The code handles this correctly but the multi-step dance is vulnerable to contention bursts under high concurrency.
Suggestion: Wrap the stale-reap DELETE + retry INSERT in a single `BEGIN IMMEDIATE` transaction so the reap-and-reclaim is atomic.

[SUGGESTION] `process_alive` shells out to `kill -0` — fragile and platform-limited
File: core/crates/adapters/src/storage_sqlite/locks.rs
Lines: 144-158
Description: Spawning an external `kill` process for every stale-lock check is heavyweight. On non-Unix targets the function unconditionally returns `false`, causing all locks to be reaped.
Suggestion: Use the `nix::sys::signal::kill` crate function on Unix for a direct syscall without process spawning.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-13 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | InMemoryStorage CAS reads MAX(snapshots) | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 2 | rollback() CAS uses wrong expected | ✅ Fixed | `36af1e7` | — | 2026-05-13 | |
| 3 | Duplicate sort_keys + notes_to_string | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 4 | TLS observer spawned with empty hostnames | ✅ Fixed | `36af1e7` | — | 2026-05-13 | |
| 5 | advance_config_version_if_eq missing version check | ✅ Fixed | `36af1e7` | — | 2026-05-13 | |
| 6 | process_alive shells out to PATH kill | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
