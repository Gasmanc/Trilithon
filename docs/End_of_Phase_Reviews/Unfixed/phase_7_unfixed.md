# Phase 7 — Unfixed Findings

**Run date:** 2026-05-13T00:00:00Z  
**Total unfixed:** 12 (11 won't fix · 1 deferred · 0 conflicts pending)

| ID | Severity | Consensus | Title | File | Status | Reason |
|----|----------|-----------|-------|------|--------|--------|
| F002 | CRITICAL | MAJORITY | COMMIT result silently discarded | `core/crates/adapters/src/sqlite_storage.rs` | wont_fix | Already fixed in Phase 7 implementation (COMMIT propagates via .map_err) |
| F003 | CRITICAL | MAJORITY | try_insert_lock DEFERRED tx TOCTOU | `core/crates/adapters/src/storage_sqlite/locks.rs` | wont_fix | Already fixed in Phase 7 implementation (pool.acquire + BEGIN IMMEDIATE) |
| F006 | HIGH | MAJORITY | InMemoryStorage CAS reads MAX(snapshots) | `core/crates/core/src/storage/in_memory.rs` | wont_fix | Already fixed in Phase 7 implementation (applied_config_version field) |
| F008 | HIGH | UNANIMOUS | Duplicate sort_keys + notes_to_string | `core/crates/adapters/src/applier_caddy.rs` | wont_fix | Already fixed in Phase 7 implementation (shared audit_notes module) |
| F010 | HIGH | MAJORITY | LockError::AlreadyHeld reports own PID | `core/crates/adapters/src/storage_sqlite/locks.rs` | wont_fix | Already fixed in Phase 7 implementation (fresh SELECT after second INSERT fails) |
| F011 | HIGH | MAJORITY | process_alive shells out to PATH kill | `core/crates/adapters/src/storage_sqlite/locks.rs` | wont_fix | Already fixed in Phase 7 implementation (nix crate, kill(pid, 0)) |
| F013 | HIGH | MAJORITY | 5xx Caddy response mapped to Storage error | `core/crates/adapters/src/applier_caddy.rs` | wont_fix | Already fixed in Phase 7 implementation (CaddyServerError taxonomy) |
| F015 | WARNING | SINGLE | advance_config_version_if_eq UPDATE rows_affected unchecked | `core/crates/adapters/src/storage_sqlite/snapshots.rs` | wont_fix | Already fixed in Phase 7 implementation (rows_affected() == 1 check) |
| F019 | WARNING | SINGLE | IPv6 upstream addresses without brackets | `core/crates/core/src/reconciler/render.rs` | wont_fix | Already fixed in Phase 7 implementation (host.contains(':') bracket wrapping) |
| F023 | SUGGESTION | SINGLE | AcquiredLock::drop spawns new Tokio runtime | `core/crates/adapters/src/storage_sqlite/locks.rs` | wont_fix | Superseded by F012 fix (block_in_place replaces nested runtime) |
| F025 | SUGGESTION | SINGLE | Conflict outcome versions swapped in handle_conflict | `core/crates/adapters/src/applier_caddy.rs` | wont_fix | Verified not a bug — expected_version maps to stale_version and observed maps to current_version correctly |
| F026 | SUGGESTION | MAJORITY | Preset body JSON embedded without structural validation | `core/crates/core/src/reconciler/render.rs` | deferred | F016 fixes the silent discard; full allowlist validation is Phase 12+ scope when policy schemas are defined |
