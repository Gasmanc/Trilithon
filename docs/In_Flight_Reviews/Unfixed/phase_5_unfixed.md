---
id: security:area::phase-5-unfixed:legacy-uncategorized
category: security
kind: process
location:
  area: phase-5-unfixed
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


## gemini — Canonicalizer Corrupts Large Integers
**Date:** 2026-05-05
**Severity:** CRITICAL
**Status:** fixed
**Reason not fixed:** Identified by gemini in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## codex — MONOTONICITY_CHECK_IS_RACEABLE
**Date:** 2026-05-05
**Severity:** CRITICAL
**Status:** fixed
**Reason not fixed:** Identified by codex in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## gemini — Missing Database Schema Updates
**Date:** 2026-05-05
**Severity:** HIGH
**Status:** open
**Reason not fixed:** Identified by gemini in phase-end multi-review, awaiting fix-pass

## codex — CANONICALIZER_CAN_MUTATE_LARGE_INTEGER_VALUES
**Date:** 2026-05-05
**Severity:** HIGH
**Status:** fixed
**Reason not fixed:** Identified by codex in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## kimi — Missing content-hash validation on snapshot insert
**Date:** 2026-05-05
**Severity:** HIGH
**Status:** fixed
**Reason not fixed:** Identified by kimi in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## kimi — canonical_json_version not persisted to database
**Date:** 2026-05-05
**Severity:** HIGH
**Status:** open
**Reason not fixed:** Identified by kimi in phase-end multi-review, awaiting fix-pass

## minimax — Hardcoded caddy_instance_id breaks monotonicity check
**Date:** 2026-05-05
**Severity:** HIGH
**Status:** open
**Reason not fixed:** V1 single-instance design is intentional per ADR-0009; added // V1: single local instance comments to document the decision. Multi-instance support is a future phase concern.

## code_adversarial — HARDCODED caddy_instance_id BREAKS MULTI-INSTANCE MONOTONICITY CHECK
**Date:** 2026-05-05
**Severity:** HIGH
**Status:** open
**Reason not fixed:** V1 single-instance design is intentional per ADR-0009; added // V1: single local instance comments to document the decision. Multi-instance support is a future phase concern.

## code_adversarial — InMemoryStorage DIVERGES FROM SqliteStorage ON DUPLICATE SEMANTICS
**Date:** 2026-05-05
**Severity:** HIGH
**Status:** fixed
**Reason not fixed:** Identified by code_adversarial in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## code_adversarial — DEDUPLICATION PATH RETURNS EARLY INSIDE AN OPEN TRANSACTION
**Date:** 2026-05-05
**Severity:** HIGH
**Status:** fixed
**Reason not fixed:** Identified by code_adversarial in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## scope_guardian — Canonical hash not computed in write path
**Date:** 2026-05-05
**Severity:** HIGH
**Status:** fixed
**Reason not fixed:** Identified by scope_guardian in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## codex — BROKEN_ADR_LINK_IN_CORE_README
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** Identified by codex in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## gemini — Low-Precision Monotonic Clock Implementation
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by gemini in phase-end multi-review, awaiting fix-pass

## qwen — created_at_monotonic_nanos semantic mismatch
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by qwen in phase-end multi-review, awaiting fix-pass

## qwen — caddy_instance_id hardcoded to 'local'
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by qwen in phase-end multi-review, awaiting fix-pass

## kimi — created_at_monotonic_nanos misnamed and loses precision
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by kimi in phase-end multi-review, awaiting fix-pass

## kimi — Snapshot fetches not scoped to caddy_instance_id
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by kimi in phase-end multi-review, awaiting fix-pass

## kimi — Large integer precision loss in canonical JSON
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** Identified by kimi in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## glm — IN_MEMORY_STORAGE_DEADLOCK_RISK
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by glm in phase-end multi-review, awaiting fix-pass

## glm — IN_MEMORY_DUPLICATE_DIVERGENCE
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** Identified by glm in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## glm — INTENT_PRIVACY_DOC_MISMATCH
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** Identified by glm in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## glm — SUPPRESSION_MISSING_TRACKED_ID
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** Identified by glm in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## code_adversarial — MONOTONICITY GUARD DEDUP BYPASS MASKS STALE VERSION
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by code_adversarial in phase-end multi-review, awaiting fix-pass

## code_adversarial — fetch_by_date_range FULL TABLE SCAN WITH NO LIMIT
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by code_adversarial in phase-end multi-review, awaiting fix-pass

## code_adversarial — IMMUTABILITY TRIGGERS NOT PRESENT UNTIL MIGRATION 0004 RUNS
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by code_adversarial in phase-end multi-review, awaiting fix-pass

## code_adversarial — canonicalise_value SORT_UNSTABLE UNDEFINED ORDER ON DUPLICATE KEYS
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by code_adversarial in phase-end multi-review, awaiting fix-pass

## scope_guardian — caddy_instance_id hardcoded without comment
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** Identified by scope_guardian in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## scope_guardian — Monotonicity property test uses loop not proptest
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by scope_guardian in phase-end multi-review, awaiting fix-pass

## security — INTENT FIELD BOUND NOT ENFORCED AT WRITE PATH
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** Identified by security in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## security — caddy_instance_id HARDCODED MONOTONICITY BYPASS
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by security in phase-end multi-review, awaiting fix-pass

## security — fetch_by_date_range SQL BUILT WITH format! — STRUCTURALLY FRAGILE
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** fixed
**Reason not fixed:** Identified by security in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## minimax — fetch_by_parent_id ordering inconsistency
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by minimax in phase-end multi-review, awaiting fix-pass

## gemini — Non-Atomic Transaction in Snapshot Insertion
**Date:** 2026-05-05
**Severity:** SUGGESTION
**Status:** fixed
**Reason not fixed:** Identified by gemini in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## gemini — Missing Instance Filtering in Version Fetch
**Date:** 2026-05-05
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by gemini in phase-end multi-review, awaiting fix-pass

## qwen — let _ = parse_actor_kind discards value awkwardly
**Date:** 2026-05-05
**Severity:** SUGGESTION
**Status:** fixed
**Reason not fixed:** Identified by qwen in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## qwen — canonical_json_version defaults to current constant
**Date:** 2026-05-05
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by qwen in phase-end multi-review, awaiting fix-pass

## qwen — InMemoryStorage diverges from SqliteStorage on duplicate handling
**Date:** 2026-05-05
**Severity:** SUGGESTION
**Status:** fixed
**Reason not fixed:** Identified by qwen in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## kimi — Snapshot::intent documentation contradicts implementation
**Date:** 2026-05-05
**Severity:** SUGGESTION
**Status:** fixed
**Reason not fixed:** Identified by kimi in phase-end multi-review, awaiting fix-pass
**Fix commit:** 78f5954

## kimi — SnapshotId accepts arbitrary strings
**Date:** 2026-05-05
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by kimi in phase-end multi-review, awaiting fix-pass

## glm — DUPLICATE_CONTENT_ADDRESS functions
**Date:** 2026-05-05
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by glm in phase-end multi-review, awaiting fix-pass

## glm — MONOTONIC_NANOS_SEMANTIC_CONFUSION
**Date:** 2026-05-05
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by glm in phase-end multi-review, awaiting fix-pass

## glm — HARDCODED_LOCAL_INSTANCE_ID needs comment
**Date:** 2026-05-05
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by glm in phase-end multi-review, awaiting fix-pass

## minimax — cast_sign_loss relies on implicit invariant
**Date:** 2026-05-05
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by minimax in phase-end multi-review, awaiting fix-pass

## security — snapshot_id NOT VALIDATED AS 64-CHAR HEX BEFORE STORAGE
**Date:** 2026-05-05
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by security in phase-end multi-review, awaiting fix-pass

## security — actor_kind DISCARDED ON READ STORED AS FIXED system ON WRITE
**Date:** 2026-05-05
**Severity:** SUGGESTION
**Status:** open
**Reason not fixed:** Identified by security in phase-end multi-review, awaiting fix-pass

## scope_guardian — SnapshotWriter not a named struct
**Date:** 2026-05-05
**Severity:** WARNING
**Status:** open
**Reason not fixed:** Identified by scope_guardian in phase-end multi-review, awaiting fix-pass
