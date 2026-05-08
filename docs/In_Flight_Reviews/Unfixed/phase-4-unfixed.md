---
id: duplicate:area::phase-4-unfixed:legacy-uncategorized
category: duplicate
kind: process
location:
  area: phase-4-unfixed
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


## gemini — Hostname length limit bypass for wildcards
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## gemini — Incorrect ValidationRule for UpstreamId conflict
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## gemini — Missing validation for ImportFromCaddyfile
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by gemini in phase-end multi-review, awaiting fix-pass

## gemini — Inefficient non-granular diff for ImportFromCaddyfile
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by gemini in phase-end multi-review, awaiting fix-pass

## gemini — Non-specific error path in check_upstreams_exist
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by gemini in phase-end multi-review, awaiting fix-pass

## gemini — Misleading error for empty hostname labels
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by gemini in phase-end multi-review, awaiting fix-pass

## codex — DeleteUpstream can break referential integrity
**Date:** 2026-05-03
**Severity:** CRITICAL
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## codex — Option<Option<T>> patch clear semantics broken
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## codex — UpdateRoute does not validate patched hostnames
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## codex — UpgradePolicy accepts nonexistent target version
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## codex — ImportFromCaddyfile bypasses state invariant validation
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** open
**Reason not fixed:** Identified by codex in phase-end multi-review, awaiting fix-pass

## codex — Per-variant schema stub $refs are invalid
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by codex in phase-end multi-review, awaiting fix-pass

## codex — ValidationRules misclassified (DuplicateRouteId + PolicyAttachmentMissing)
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## qwen — Wrong ValidationRule for duplicate upstream id
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## qwen — Wrong ValidationRule for missing route
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## qwen — unreachable!() in production code
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## qwen — Incorrect patch application for Option<Option<T>> in TLS
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## qwen — Misleading test name idempotency vs determinism
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by qwen in phase-end multi-review, awaiting fix-pass

## qwen — Outdated comment in build.rs
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by qwen in phase-end multi-review, awaiting fix-pass

## qwen — Overly broad allow(clippy::option_option) on patches structs
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by qwen in phase-end multi-review, awaiting fix-pass

## qwen — Suppression comment missing standard format fields
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by qwen in phase-end multi-review, awaiting fix-pass

## glm — unreachable! in production code path (apply.rs:109)
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## glm — Duplicate AuditEvent enum across modules
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## glm — Duplicate UnixSeconds type alias
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## glm — ValidationRule::PolicyAttachmentMissing overloaded for unrelated failures
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## glm — ValidationRule::DuplicateRouteId used for upstream duplicate
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## glm — UpdateRoute patch does not validate hostnames
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## glm — DeleteUpstream does not check for orphan route references
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## glm — gen_mutation_schemas binary blurs three-layer boundary
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by glm in phase-end multi-review, awaiting fix-pass

## glm — RoutePatch doc comment inaccurate about Option<Option<T>> convention
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by glm in phase-end multi-review, awaiting fix-pass

## glm — MutationKind missing schemars::JsonSchema derive
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## minimax — schemars in core [dependencies]
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by minimax in phase-end multi-review, awaiting fix-pass

## minimax — Hardcoded relative path in build.rs
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by minimax in phase-end multi-review, awaiting fix-pass

## minimax — gen_mutation_schemas uses process::exit(1)
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by minimax in phase-end multi-review, awaiting fix-pass

## minimax — Schema stubs use non-standard x-variant field
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by minimax in phase-end multi-review, awaiting fix-pass

## minimax — No 13-variant idempotency test coverage in proptest
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by minimax in phase-end multi-review, awaiting fix-pass

## minimax — All-None GlobalConfigPatch is unvalidated no-op
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by minimax in phase-end multi-review, awaiting fix-pass

## minimax — build.rs missing cargo:rerun-if-changed for schema dir
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by minimax in phase-end multi-review, awaiting fix-pass

## minimax — DesiredState::empty() is identical to Default::default()
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by minimax in phase-end multi-review, awaiting fix-pass

## minimax — apply_mutation error variants lack context fields
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by minimax in phase-end multi-review, awaiting fix-pass

## minimax — Duplicate Debug derive and manual impl on AuditEvent
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by minimax in phase-end multi-review, awaiting fix-pass

## minimax — Schema stubs lack $id field
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by minimax in phase-end multi-review, awaiting fix-pass

## kimi — Three-state patch semantics broken on RoutePatch and UpstreamPatch
**Date:** 2026-05-03
**Severity:** CRITICAL
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## kimi — ValidationRule misuse: PolicyAttachmentMissing for missing routes
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## kimi — ValidationRule misuse: DuplicateRouteId for upstream collision
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## kimi — DesiredState.policies is dead state
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** open
**Reason not fixed:** Design decision deferred to Phase 5 — removing policies field would break the desired_state serde test, and wiring AttachPolicy to populate it requires evaluating the Phase 5 canonical hash impact. Skipped as a cross-phase design decision.

## kimi — PolicyAttachment and RoutePolicyAttachment are structural duplicates
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** open
**Reason not fixed:** Type alias consolidation would conflate two semantically distinct types (one represents a resolved attachment in DesiredState.policies, the other is embedded in Route). Requires design evaluation before merging — skipped to avoid premature coupling.

## kimi — apply_set_global_config / apply_set_tls_config emit no-op diffs
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by kimi in phase-end multi-review, awaiting fix-pass

## kimi — apply_import_caddyfile coarse diffs and silent overwrites
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by kimi in phase-end multi-review, awaiting fix-pass

## kimi — Hostname total-length check bypassed for wildcards
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## kimi — Identifier newtypes have no ULID validation
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by kimi in phase-end multi-review, awaiting fix-pass

## kimi — proptest commutativity test checks membership not state equality
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## kimi — storage::types::AuditEvent and audit::AuditEvent name collision
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** N/A
**Fix commit:** cf425a4

## kimi — Dead audit_event_for arm for Rollback
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by kimi in phase-end multi-review, awaiting fix-pass

## kimi — MutationOutcome.kind should be renamed audit_event
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by kimi in phase-end multi-review, awaiting fix-pass

## kimi — Diff serialization swallows errors silently
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by kimi in phase-end multi-review, awaiting fix-pass

## kimi — build.rs hardcodes relative path to schema dir
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by kimi in phase-end multi-review, awaiting fix-pass

## phase-end simplify — VOCAB const duplicates AUDIT_KINDS slice
**Date:** 2026-05-03
**Severity:** CRITICAL
**Status:** open
**Reason not fixed:** `crate::storage::audit_vocab::AUDIT_KINDS` does not exist in Phase 4. The test comment explicitly documents this. No action until the storage module is introduced.

## phase-end simplify — check_hostnames_valid discards HostnameError details
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** fixed
**Fix commit:** 23ba033

## phase-end simplify — Dead second s.len() > 253 check in validate_hostname
**Date:** 2026-05-03
**Severity:** WARNING
**Status:** fixed
**Fix commit:** 23ba033

## phase-end simplify — serde_json::to_value(...).ok() repeated ~20× — extract helper
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** fixed
**Fix commit:** 23ba033

## phase-end simplify — Policy mutation preamble copy-pasted 3× in apply.rs
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** fixed
**Fix commit:** 23ba033

## phase-end simplify — Four inline route not found checks instead of calling check_route_exists
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** fixed
**Fix commit:** 23ba033

## phase-end simplify — check_delete_upstream uses UpstreamReferenceMissing for wrong condition
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** fixed
**Fix commit:** 23ba033

## phase-end simplify — capability.rs duplicates route-module derivation between CreateRoute and UpdateRoute
**Date:** 2026-05-03
**Severity:** HIGH
**Status:** open
**Reason not fixed:** Only 2 call sites — three-use rule not met per project conventions. The CreateRoute and UpdateRoute arms differ structurally (UpdateRoute wraps checks in Option guards), making a shared helper awkward without improving clarity.

## phase-end simplify — Rename caps_with_everything to empty_caps in proptest
**Date:** 2026-05-03
**Severity:** SUGGESTION
**Status:** fixed
**Fix commit:** 23ba033
