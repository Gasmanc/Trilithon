# Phase 7 — Kimi Review Findings

**Reviewer:** kimi
**Date:** 2026-05-10
**Diff range:** ddda146..HEAD
**Phase:** 7

---

[CRITICAL] TLS observer receives empty hostnames and never polls
File: core/crates/adapters/src/applier_caddy.rs
Lines: 520-526
Description: When a `TlsIssuanceObserver` is configured, `apply()` spawns it with `vec![]` for `hostnames`. The observer's `observe()` returns immediately when `hostnames.is_empty()`, so no polling ever occurs and no follow-up audit rows (success or timeout) are emitted. The TLS issuance observation feature is completely non-functional.
Suggestion: Extract managed hostnames from `desired_state.routes` before the observer spawn and pass them to `observer.observe(correlation_id, hostnames, Some(sid))`.

[HIGH] IPv6 upstream addresses rendered without brackets
File: core/crates/core/src/reconciler/render.rs
Lines: 353-356
Description: `resolve_upstream_dial` formats TCP upstreams as `{host}:{port}`. For IPv6 addresses (e.g. `::1`) this produces an ambiguous string like `::1:8080` instead of the bracketed form `[::1]:8080` that Caddy expects in `dial`.
Suggestion: Detect IPv6 addresses (colon-containing hosts) and wrap them in brackets: `format!("[{host}]:{port}")` when `host.contains(':')`.

[HIGH] CAS advance UPDATE does not verify row existence
File: core/crates/adapters/src/storage_sqlite/snapshots.rs
Lines: 100-106
Description: `advance_config_version_if_eq` executes an UPDATE that may affect zero rows if the `instance_id` row is missing or was deleted. It returns `Ok(new_version)` regardless, so the caller believes the CAS succeeded when the version was never persisted.
Suggestion: Check `query_result.rows_affected() == 1` after the UPDATE and return `Err(StorageError::Integrity { detail: "instance row missing" })` if not.

[HIGH] Post-load equivalence errors misclassified as Unreachable
File: core/crates/adapters/src/applier_caddy.rs
Lines: 307-313
Description: `verify_equivalence` maps ANY `CaddyError` from `get_running_config` to `ApplyError::Unreachable`, including 5xx `BadStatus` and `ProtocolViolation`. This misleads callers into treating a reachable-but-failing Caddy as down.
Suggestion: Distinguish `CaddyError::Unreachable`/`Timeout` from `BadStatus` and `ProtocolViolation`, mapping the latter to `ApplyError::CaddyRejected` or a new dedicated variant.

[WARNING] Non-4xx Caddy errors mapped to Storage
File: core/crates/adapters/src/applier_caddy.rs
Lines: 300-301
Description: In `load_or_fail`, the catch-all arm maps every non-4xx `CaddyError` (including 5xx `BadStatus`, `ProtocolViolation`, and `InvalidEndpoint`) to `ApplyError::Storage`.
Suggestion: Add explicit match arms for 5xx `BadStatus` → `ApplyError::CaddyRejected`, `ProtocolViolation` → `ApplyError::Unreachable`.

[WARNING] Invalid preset JSON silently discarded during render
File: core/crates/core/src/reconciler/render.rs
Lines: 328-330
Description: When embedding a policy attachment, `serde_json::from_str` failure is silently ignored with `if let Ok(body) = ...`. A corrupted or non-JSON preset body disappears from the rendered config without raising an error.
Suggestion: Treat parse failure as a render error (e.g. add `RenderError::InvalidPresetBody`) instead of silently omitting the policy.

[WARNING] Duplicate sort_keys / notes_to_string logic
File: core/crates/adapters/src/applier_caddy.rs (lines 93-114) and core/crates/adapters/src/tls_observer.rs (lines 61-82)
Description: Both modules contain nearly identical `notes_to_string` and `sort_keys` helpers. This duplication risks divergence if one is updated and the other is not.
Suggestion: Extract the helpers to a shared module (e.g. `crate::json_utils`).

[WARNING] Advisory lock helper assumes DEFERRED is equivalent to IMMEDIATE
File: core/crates/adapters/src/storage_sqlite/locks.rs
Lines: 286-299
Description: `try_insert_lock` calls `pool.begin()` (DEFERRED), then issues `BEGIN IMMEDIATE`. When the latter fails with "within a transaction", the fallback treats the existing DEFERRED transaction as sufficient.
Suggestion: Issue `BEGIN IMMEDIATE` directly on the pool without `pool.begin()`, managing the transaction manually.

[SUGGESTION] Notes JSON serialisation falls back to empty object on failure
File: core/crates/adapters/src/applier_caddy.rs
Lines: 93-99
Description: `notes_to_string` returns `"{}"` if serde serialisation fails. While unlikely, losing all structured audit metadata on the success path is a blind spot in the audit trail.
Suggestion: Use `expect` or `unwrap` so a failure panics in tests rather than silently writing empty notes in production.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-13 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | TLS observer spawned with empty hostnames | ✅ Fixed | `36af1e7` | — | 2026-05-13 | |
| 2 | Duplicate sort_keys + notes_to_string | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 3 | verify_equivalence maps all CaddyError to Unreachable | ✅ Fixed | `569b149` | — | 2026-05-13 | |
| 4 | advance_config_version_if_eq UPDATE rows_affected unchecked | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 5 | Invalid preset JSON silently discarded | ✅ Fixed | `569b149` | — | 2026-05-13 | |
| 6 | Preset body embedded without allowlist validation | ⏭️ Deferred | — | — | — | Phase 12+ scope; F016 fixes the silent discard |
| 7 | IPv6 upstream addresses without brackets | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 8 | 5xx response mapped to Storage error | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
