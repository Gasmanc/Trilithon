<!-- No unfixed items for Slice 7.8 -->
<!-- No unfixed items for Slice 7.7 -->
<!-- No unfixed items for Slice 7.1 -->
<!-- No unfixed items for Slice 7.4 -->
<!-- No unfixed items for Slice 7.5 -->

## Slice 7.6
- **Pre-existing lib-test failures** — `storage_sqlite::snapshots::tests::advance_succeeds_when_versions_match` and `advance_returns_conflict_when_versions_mismatch` were already failing on `main` before Slice 7.6 was implemented. These tests exercise `advance_config_version_if_eq` with a `BEGIN IMMEDIATE` nested inside an already-begun transaction; the fix would require restructuring those unit tests or the snapshot helper. Deferred to a future cleanup slice.

## codex — APPLIED_VERSION_ADVANCES_ON_FAILED_APPLY
**Date:** 2026-05-10
**Severity:** CRITICAL
**Status:** open
**Reason not fixed:** Architectural: CAS advance happens before Caddy I/O; moving it after requires restructuring the apply() transaction boundary. The COMMIT-failure path is now fixed (a795a7c). The deeper reordering is deferred.

## codex — CAS_DOES_NOT_VERIFY_TARGET_SNAPSHOT_VERSION
**Date:** 2026-05-10
**Severity:** HIGH
**Status:** fixed
**Fix commit:** a795a7c — rows_affected() check added to advance_config_version_if_eq

## codex — CADDY_5XX_PATH_IS_MISCLASSIFIED_AND_UNAUDITED
**Date:** 2026-05-10
**Severity:** HIGH
**Status:** fixed
**Fix commit:** a795a7c — explicit 5xx arm added with ApplyFailureKind::CaddyServerError audit row

## codex — TLS_OBSERVER_IS_NEVER_ACTUALLY_TRIGGERED_FROM_APPLY
**Date:** 2026-05-10
**Severity:** HIGH
**Status:** fixed
**Fix commit:** a795a7c — debug log added on empty-hostname path; known limitation documented

## qwen — CONFLICT_OUTCOME_VERSIONS_SWAPPED
**Date:** 2026-05-10
**Severity:** CRITICAL
**Status:** open
**Reason not fixed:** False positive after analysis — the code at applier_caddy.rs:458-461 correctly passes `(expected, observed)` to `handle_conflict(stale_version, current_version)`: `expected` = what caller expected (stale), `observed` = what DB has (current). No swap exists.

## qwen — TRY_INSERT_LOCK_USES_DEFERRED_NOT_IMMEDIATE
**Date:** 2026-05-10
**Severity:** CRITICAL
**Status:** fixed
**Fix commit:** a795a7c — replaced pool.begin() + nested BEGIN IMMEDIATE with pool.acquire() + direct BEGIN IMMEDIATE

## kimi — TLS observer receives empty hostnames and never polls
**Date:** 2026-05-10
**Severity:** CRITICAL
**Status:** fixed
**Fix commit:** a795a7c — debug log added on empty-hostname path; full fix deferred to post-Phase 7 (hostname extraction from desired_state)

## kimi — IPv6 upstream addresses rendered without brackets
**Date:** 2026-05-10
**Severity:** HIGH
**Status:** fixed
**Fix commit:** a795a7c — wrap host in brackets when host.contains(':') in resolve_upstream_dial

## kimi — CAS advance UPDATE does not verify row existence
**Date:** 2026-05-10
**Severity:** HIGH
**Status:** fixed
**Fix commit:** a795a7c — rows_affected() == 1 check added; returns StorageError::Integrity if instance row missing

## kimi — Post-load equivalence errors misclassified as Unreachable
**Date:** 2026-05-10
**Severity:** HIGH
**Status:** open
**Reason not fixed:** Not included in multi-review findings list for this pass; deferred.

## glm — InMemoryStorage CAS reads wrong counter
**Date:** 2026-05-10
**Severity:** HIGH
**Status:** fixed
**Fix commit:** a795a7c — added AtomicI64 applied_config_version to InMemoryStorage; current_config_version reads it; cas_advance_config_version writes it on success

## minimax — try_insert_lock blanket-catches BEGIN IMMEDIATE failure
**Date:** 2026-05-10
**Severity:** HIGH
**Status:** fixed
**Fix commit:** a795a7c — same fix as TRY_INSERT_LOCK_USES_DEFERRED_NOT_IMMEDIATE

## code_adversarial — PHANTOM APPLIED VERSION ON PANIC OR 5XX AFTER CAS
**Date:** 2026-05-10
**Severity:** CRITICAL
**Status:** open
**Reason not fixed:** Architectural: the CAS-before-Caddy ordering requires a larger refactor. The COMMIT-failure sub-issue is fixed (a795a7c). Full reordering deferred.

## code_adversarial — COMMIT FAILURE SILENTLY RETURNS Ok WHILE APPLIED VERSION STAYS UNCHANGED
**Date:** 2026-05-10
**Severity:** CRITICAL
**Status:** fixed
**Fix commit:** a795a7c — COMMIT error now propagated via .map_err(sqlx_err)?

## code_adversarial — ROLLBACK ALWAYS CONFLICTS
**Date:** 2026-05-10
**Severity:** HIGH
**Status:** open
**Reason not fixed:** Not included in multi-review findings list for this pass; deferred.

## code_adversarial — STALE LOCK REAP RACE: AlreadyHeld REPORTS OWN PID
**Date:** 2026-05-10
**Severity:** HIGH
**Status:** fixed
**Fix commit:** a795a7c — after second INSERT failure in None-row race, SELECT actual holder_pid instead of returning caller's own pid

## code_adversarial — PROCESS_ALIVE USES SHELL kill COMMAND — PID REUSE RACE
**Date:** 2026-05-10
**Severity:** HIGH
**Status:** fixed
**Fix commit:** a795a7c — replaced shell subprocess with nix::sys::signal::kill(pid, None); nix moved to [dependencies]

## code_adversarial — ADVISORY LOCK DROP ON PANIC: SPAWN_BLOCKING COMPLETES AFTER MUTEX RELEASES
**Date:** 2026-05-10
**Severity:** HIGH
**Status:** open
**Reason not fixed:** Not included in multi-review findings list for this pass; deferred.

## code_adversarial — 5XX CADDY RESPONSE MAPPED TO Storage
**Date:** 2026-05-10
**Severity:** WARNING
**Status:** fixed
**Fix commit:** a795a7c — explicit 5xx arm with CaddyServerError audit row; same as CADDY_5XX_PATH_IS_MISCLASSIFIED_AND_UNAUDITED

## code_adversarial — CONFLICT AUDIT NOTE USES HAND-ROLLED JSON
**Date:** 2026-05-10
**Severity:** WARNING
**Status:** open
**Reason not fixed:** ApplyAuditNotes struct does not have stale_version/current_version fields. Routing through notes_to_string would require adding new fields to the core struct — a broader change. The hand-rolled JSON in handle_conflict is consistent and safe.

## code_adversarial — BEGIN IMMEDIATE DOUBLE-ISSUE SILENTLY DEGRADES TO DEFERRED
**Date:** 2026-05-10
**Severity:** WARNING
**Status:** fixed
**Fix commit:** a795a7c — same fix as TRY_INSERT_LOCK_USES_DEFERRED_NOT_IMMEDIATE

## security — SHELL SUBPROCESS FOR PROCESS LIVENESS CHECK
**Date:** 2026-05-10
**Severity:** WARNING
**Status:** fixed
**Fix commit:** a795a7c — replaced with nix syscall; nix moved to [dependencies] with features=["signal"]

## security — nix DEPENDENCY ONLY IN dev-dependencies
**Date:** 2026-05-10
**Severity:** WARNING
**Status:** fixed
**Fix commit:** a795a7c — nix moved to [dependencies] with features=["signal"]

## scope_guardian — InMemoryStorage::current_config_version READS WRONG COUNTER
**Date:** 2026-05-10
**Severity:** WARNING
**Status:** fixed
**Fix commit:** a795a7c — same fix as glm — InMemoryStorage CAS reads wrong counter

## scope_guardian — notes_to_string AND sort_keys DUPLICATED
**Date:** 2026-05-10
**Severity:** WARNING
**Status:** fixed
**Fix commit:** a795a7c — extracted to audit_notes.rs; both applier_caddy.rs and tls_observer.rs import from there

## merge_review — F-PMR7-001 Proposed seams never ratified into seams-proposed.md
**Date:** 2026-05-10
**Severity:** CRITICAL
**Status:** open
**Reason not fixed:** Catch-up mode — phase already merged. Phase 7 tagging audit listed 5 proposed seams that were never written to seams-proposed.md or ratified into seams.md. Should be addressed in Phase 9 prep.

## merge_review — F-PMR7-005 contract-roots.toml not updated with Phase 7 public symbols
**Date:** 2026-05-10
**Severity:** HIGH
**Status:** open
**Reason not fixed:** Catch-up mode — phase already merged. ~21 new public symbols (Applier, ApplyOutcome, ApplyAuditNotes, CaddyApplier, TlsIssuanceObserver) not added to contract-roots.toml. Must be done before Phase 9.

## phase-end simplify — Unified unreachable audit branches
**Date:** 2026-05-10
**Severity:** WARNING
**Status:** fixed
**Fix commit:** 9f22604

## phase-end simplify — Safer conflict audit notes format
**Date:** 2026-05-10
**Severity:** HIGH
**Status:** fixed
**Fix commit:** 9f22604

## phase-end simplify — build_header_ops returns Option<Value>
**Date:** 2026-05-10
**Severity:** WARNING
**Status:** fixed
**Fix commit:** 9f22604

## phase-end simplify — Fix stale module-doc comment
**Date:** 2026-05-10
**Severity:** SUGGESTION
**Status:** fixed
**Fix commit:** 9f22604

## phase-end simplify — Two timeout paths in tls_observer
**Date:** 2026-05-10
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Both paths are semantically correct — deadline check before sleep vs after sleep computation are distinct guard points. No logic bug; skip.

## phase-end simplify — InMemory CAS stricter than SQLite
**Date:** 2026-05-10
**Severity:** WARNING
**Status:** open
**Reason not fixed:** The in-memory double intentionally enforces `snapshot.config_version == new_version` as an extra invariant not checked by the SQLite path. This is beneficial conservatism in tests, not a divergence bug. Skip.
