---
id: duplicate:area::phase-3-unfixed:legacy-uncategorized
category: duplicate
kind: process
location:
  area: phase-3-unfixed
  multi: false
finding_kind: legacy-uncategorized
phase_introduced: unknown
status: open
created_at: migration
created_by: legacy-migration
last_verified_at: 0a795583ea9c4266e7d9b0ae0f56fd47d2ecf574
severity: medium
do_not_autofix: false
---

## slice-3.5 — duplicated sqlx_err helper
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** `sqlx_err` in `capability_store.rs` is an exact copy of the same function in `sqlite_storage.rs`. The "three uses before extracting" rule in CLAUDE.md means extraction to a shared private module is not warranted until a third consumer appears.

## slice-3.4 — collect_module_ids unbounded recursion
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** `collect_module_ids` recurses into every node of the Caddy JSON config with no depth limit. A pathological or adversarially crafted config could overflow the stack. The production risk is low because Caddy configs are operator-supplied and bounded in practice, and adding a depth counter would require a breaking signature change or a wrapper. Left for a dedicated hardening pass.

## gemini — Missing sync_all Before Rename
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## gemini — No UUID Validation On Read
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## gemini — SQLite Error Code Extended Codes Not Masked
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## gemini — Missing Request-Level Timeouts
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** skipped
**Reason not fixed:** `load_config` and `patch_config` already wrap their futures in `tokio::time::timeout(self.apply_timeout, ...)`. Other methods use `connect_timeout`. The timeout coverage is already implemented; no clear remaining action.

## codex — PATCH Semantics Do Not Match Caddy API
**Date:** 2026-05-03
**Severity:** CRITICAL
**Status:** skipped
**Reason not fixed:** Structural change touching hyper_client.rs, sentinel.rs, and the CaddyClient trait simultaneously. The sentinel tests use an in-memory double so would need redesign too. Deferred to a dedicated phase where Caddy integration tests can validate the fix against a real Caddy 2.8 process.

## codex — Sentinel Creation Uses Replace-Only Path Update
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** skipped
**Reason not fixed:** Closely linked to the PATCH semantics CRITICAL finding. Deferred together with it — fixing in isolation without resolving the underlying PATCH semantics would leave the code in an inconsistent intermediate state.

## codex — Capability Probe Persistence Can Fail On Fresh DB
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## codex — Reconnect Backoff Neutralized By Fixed 15s Sleep
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## codex — Reconnect Logic Test Incorrectly E2E-Gated
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** skipped
**Reason not fixed:** The test uses real `tokio::time::sleep` with a 45-second overall budget. The scripted dead window (2-7s) is shorter than HEALTH_INTERVAL (15s), so the first health check after the window reopens would never observe a disconnect; `caddy.connected` would never fire. Removing the E2E gate would produce a slow test that fails unconditionally. Requires a test redesign using `tokio::time::pause`/`advance` before the gate can be dropped.

## codex — Takeover Audit Event Dropped
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## qwen — Incomplete Caddy Server Block In Sentinel Write
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## qwen — Unbounded Recursion In collect_module_ids
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## qwen — Misleading Lock-Free Doc Comment
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## qwen — Traceparent Doc Mismatches Implementation
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## qwen — Duplicate Step Numbering In config_loader
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## qwen — validate_endpoint Does Not Normalize Domain Case
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## qwen — Sentinel Pointer Could Collide With User Servers
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** skipped
**Reason not fixed:** Low V1 risk (operator-controlled config). Suggestion is to document the reserved name — acceptable as a Phase 6 ADR note rather than a code change now.

## qwen — conflict_error Embeds Logging Side Effect In Error Builder
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** skipped
**Reason not fixed:** Moving the log to run.rs call site would require ensure_sentinel to propagate additional context upward. The current placement avoids double-logging because run.rs does not log the conflict error itself. Low risk of double-logging in practice; deferring to avoid a refactor that could introduce silent-log regressions.

## minimax — CaddyError::OwnershipMismatch Dead Code
**Date:** 2026-05-03
**Severity:** CRITICAL
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## minimax — caddy_version Always Returns "unknown"
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## minimax — collect_module_ids Unbounded Recursion
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## minimax — localhost Not A Reliable Loopback Indicator
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** skipped
**Reason not fixed:** Changing this would break existing configs and tests that explicitly permit `localhost`. The existing tests include `loopback_localhost_ok` which validates this acceptance. Rejecting `localhost` is a breaking ADR-level change requiring explicit design decision; deferred to Phase 5 network model work.

## minimax — Takeover Audit Event Dropped At Call Site
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## minimax — Replace On Non-Existent Sub-Path In Takeover
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** skipped
**Reason not fixed:** Closely related to the PATCH semantics CRITICAL finding. Deferred together with it — fixing in isolation risks inconsistent intermediate state.

## minimax — Probe Event Missing correlation_id
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## minimax — sqlx_err Helper Duplicated
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** skipped
**Reason not fixed:** Two uses exist (capability_store.rs and sqlite_storage.rs). CLAUDE.md rule requires three uses before extracting a helper. Defer to Phase 4 when a third consumer appears.

## minimax — E2E Test Hardcoded Socket Path
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** 8a5180d

## phase-end simplify — sqlx_err duplication
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Only 2 uses of identical function; three-use extraction rule not yet met. Extract when a third use appears in Phase 4.

## phase-end simplify — Two ShutdownObserver traits
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Structural refactor touching core + adapters + cli; risky mid-phase. Deferred to a dedicated refactor pass.

## phase-end simplify — Sentinel raw JSON map
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Requires typed sentinel struct; deferred to Phase 6 design review.

## phase-end simplify — Unconditional DB write on probe
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Low-churn path (reconnect events only, not health ticks); complexity of pre-check outweighs benefit at current scale.

## phase-end simplify — Double TOML round-trip in config_loader
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Startup-only, sub-millisecond cost; deferred.
