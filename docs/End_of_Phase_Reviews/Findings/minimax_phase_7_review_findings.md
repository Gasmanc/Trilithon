# Phase 7 — Minimax Review Findings

**Reviewer:** minimax
**Date:** 2026-05-10
**Diff range:** ddda146..HEAD
**Phase:** 7

---

[HIGH] try_insert_lock blanket-catches BEGIN IMMEDIATE failure — silent race
File: core/crates/adapters/src/storage_sqlite/locks.rs
Lines: 286–298
Description: `try_insert_lock` blanket-catches "within a transaction" message from ANY `BEGIN IMMEDIATE` failure and treats it as success. But `BEGIN IMMEDIATE` can fail with that message for reasons other than an already-started transaction — including lock contention, WAL busy, or corruption. The error-ignore path silently succeeds when the INSERT might not have run.
Suggestion: Remove the `BEGIN IMMEDIATE` escalation entirely — use `pool.begin().await` directly, which acquires an IMMEDIATE transaction in WAL mode automatically, or narrow the catch to only the post-begin() case.

[WARNING] InMemoryStorage reads MAX(config_version) instead of applied_config_version
File: core/crates/core/src/storage/in_memory.rs
Lines: 308–320
Description: `current_config_version` returns `MAX(snapshots.config_version)` from all snapshots. The trait docstring specifies it should read `caddy_instances.applied_config_version` — the last *applied* version. The InMemory impl conflates "latest snapshot version" with "latest applied version", breaking CAS semantics.
Suggestion: Add an explicit `applied_config_version: i64` field to `InMemoryStorage` and update it atomically in `cas_advance_config_version`.

[WARNING] TLS observer always spawned with empty hostnames — dead code
File: core/crates/adapters/src/applier_caddy.rs
Lines: 520–526
Description: `CaddyApplier::apply` always spawns the TLS observer with an empty `hostnames` vec. `TlsIssuanceObserver::observe` returns immediately without doing any work when `hostnames.is_empty()`. The observer is dead code in the V1 applier path.
Suggestion: Either pass actual hostnames from the desired state to the observer, or remove `tls_observer` from `CaddyApplier` until the hostname-detection logic is wired up.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-13 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | TLS observer spawned with empty hostnames | ✅ Fixed | `36af1e7` | — | 2026-05-13 | |
| 2 | try_insert_lock DEFERRED tx TOCTOU | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 3 | InMemoryStorage CAS reads MAX(snapshots) | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
