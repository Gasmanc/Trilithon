# Phase 22 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:** CLAUDE.md, docs/architecture/architecture.md, docs/architecture/trait-signatures.md, docs/planning/PRD.md, docs/adr/ (0001–0016, primarily 0001 and 0008), docs/todo/phase-22-access-log-viewer.md, docs/architecture/seams.md, docs/architecture/contract-roots.toml, docs/architecture/contracts.md
**Slices analysed:** 7

## Proposed Tags

### 22.1: Rolling on-disk store and ingest
**Proposed tag:** [cross-cutting]
**Reasoning:** The slice spans three crates — it adds the `AccessLogStore` adapter, modifies `cli/src/runtime.rs` to spawn the ingest task at startup, modifies `core/src/config.rs` to add the `[access_logs]` config section, and extends the existing `caddy_admin.rs` adapter with `set_global_log_sink`. It introduces a new long-lived daemon task (the ingest socket listener) that every later slice in the phase depends on, and it crosses the cli↔adapters↔core boundary. The store and its `AccessLogEntry` type become the foundational data structure that 22.2–22.7 all build on, making this a migration-style slice others depend on.
**Affected seams:** none (no existing seam matches; the access-log store is a new in-phase boundary)
**Planned contract additions:** none mandatory — `AccessLogStore`, `AccessLogConfig`, `AccessLogEntry`, `AccessLogStoreError` are new public symbols in `adapters` but no contract-root marker is required by the TODO; flag as a candidate for `/phase-merge-review` if 22.5's HTTP layer exposes them across a stable boundary
**Confidence:** high
**If low confidence, why:** —

### 22.2: Hourly index and capacity alarm
**Proposed tag:** [cross-cutting]
**Reasoning:** Although the new index code is confined to `adapters`, the slice introduces a brand-new tracing event name `access-logs.capacity-90-percent` that it MUST add to architecture §12.1 in the same commit — this is exactly the "introduces a tracing/observability convention others follow" trigger for cross-cutting. The §12.1 vocabulary is a shared authoritative document; an addition there is a workspace-wide convention change, and the TODO's open question explicitly calls out that the new event must be ratified.
**Affected seams:** none
**Planned contract additions:** none — `HourIndex`, `rebuild_for_file`, `RebuildError` are new `adapters` symbols but no contract marker is mandated
**Confidence:** high
**If low confidence, why:** —

### 22.3: Filter engine
**Proposed tag:** [standard]
**Reasoning:** The filter engine is confined to one crate (`core`), under one new module tree `core/src/access_log/`. It adds the `FilterSource` trait but that trait is local to the access-log module and is not a shared workspace trait in `trait-signatures.md`; it implements pure filtering logic with no I/O and no async. The `lib.rs` edit is a one-line `pub mod` declaration, not a cross-layer dependency. It is self-contained and extends a single coherent unit of functionality.
**Affected seams:** none
**Planned contract additions:** none — `Filter`, `LatencyBucket`, `FilterSource` are new `core` public symbols; not mandated as contract roots by the TODO
**Confidence:** medium
**If low confidence, why:** `FilterSource` is a new trait, and if a future reviewer decides it qualifies as a shared trait surface it would push toward cross-cutting, but as scoped here it is module-local.

### 22.4: Explanation engine
**Proposed tag:** [cross-cutting]
**Reasoning:** The slice spans two crates — the pure `Explanation` types live in `core/src/access_log/explanation.rs` while the I/O `AccessLogExplainer` lives in `adapters/src/access_log_explainer.rs` — and the explainer reaches across multiple existing adapters (`snapshot_store`, `policy_store`, `access_log_store`) to correlate an entry against the snapshot active at its timestamp. It depends on Phase 7/8 snapshot history and Phase 18 policy presets, references multiple PRD IDs (T2.6, T2.2, T1.2), and consumes `core::policy::SlotName`. It crosses the core↔adapters boundary and integrates several prior phases' outputs.
**Affected seams:** PROPOSED: access-log-explanation-snapshot — "Access Log Explainer ↔ Snapshot/Policy History" (the explainer correlates an access-log entry against the snapshot and policy attachment active at the entry timestamp; contracts would include `trilithon_adapters::access_log_explainer::AccessLogExplainer`, `trilithon_adapters::snapshot_store::SnapshotStore`, `trilithon_adapters::policy_store::PolicyStore`)
**Planned contract additions:** none mandated — `Explanation`, `DecisionLayer`, `SlotOutcome`, `SlotDecision`, `UnmatchedReason`, `DecisionCoverage` (core) and `AccessLogExplainer`, `ExplainError` (adapters) are new public symbols; recommend `/phase-merge-review` evaluate `AccessLogExplainer` as a contract root given the cross-phase correlation it performs
**Confidence:** medium
**If low confidence, why:** Whether the explainer↔snapshot correlation is a durable enough boundary to warrant a ratified seam is a judgement call for `/phase-merge-review`; the proposed seam is staged, not asserted.

### 22.5: HTTP endpoints (paginated and SSE tail)
**Proposed tag:** [cross-cutting]
**Reasoning:** The slice adds three handlers in `cli/src/http/access_logs.rs` and mounts them in `cli/src/http/router.rs`, wiring `cli` to `core` (`Filter`, `Explanation`) and `adapters` (`AccessLogExplainer`, the live ingest broadcast channel). It introduces an SSE streaming surface with a typed backpressure-warning convention, and its design explicitly interacts with the Phase 19 H16 gateway-envelope boundary (the TODO notes the gateway-wrapped vs raw-entry shape distinction). It crosses the cli↔core↔adapters boundary and touches the HTTP router shared by every other web feature.
**Affected seams:** none existing; the H16 gateway envelope referenced is a Phase 19 boundary, not yet a registered seam
**Planned contract additions:** none mandated — `AccessLogsListResponse`, `TailEvent`, `TailWarning` are new `cli` HTTP types
**Confidence:** high
**If low confidence, why:** —

### 22.6: Web UI viewer page
**Proposed tag:** [standard]
**Reasoning:** The slice is confined to the `web/` frontend crate under one feature directory `web/src/features/access_logs/`. It adds React components, hooks, and TypeScript types that mirror the Phase 22.5 API shapes but introduces no new trait, no Rust cross-layer dependency, and no backend I/O. It does add a new frontend dependency (`@tanstack/react-virtual` or equivalent), which is a single-package addition within one crate — consistent with standard scope, not cross-cutting.
**Affected seams:** none
**Planned contract additions:** none (frontend TypeScript types are not tracked in the Rust contract registry)
**Confidence:** high
**If low confidence, why:** —

### 22.7: Performance harness and 95% explanation coverage tests
**Proposed tag:** [standard]
**Reasoning:** The slice adds three integration test files plus a corpus generator script, all under `adapters/tests/` and `adapters/fixtures/`. It is test-only code in a single crate with no production trait, contract, or layer changes. The gateway-envelope test exercises the Phase 19 H16 boundary and the explanation-coverage test exercises 22.4's output, but exercising existing contracts in tests does not itself make a slice cross-cutting — no shared convention is introduced or modified here.
**Affected seams:** none (the gateway-envelope test verifies a Phase 19 boundary; verification, not introduction)
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** —

## Summary
- 0 trivial / 3 standard (22.3, 22.6, 22.7) / 4 cross-cutting (22.1, 22.2, 22.4, 22.5) / 2 low-confidence (22.3, 22.4)

## Notes

- **No trivial slices.** Every slice either introduces a new module/trait, crosses a layer boundary, modifies a shared vocabulary, or adds I/O — none meets the strict trivial bar.
- **22.2 is cross-cutting despite single-crate scope.** The deciding factor is the new tracing event `access-logs.capacity-90-percent` added to architecture §12.1. The §12.1 vocabulary is authoritative and workspace-wide; the TODO's own open question flags that the event name is not pre-listed in the phased-plan and must be confirmed. Treat the §12.1 edit as a coherence-sensitive change.
- **22.1 is the migration backbone.** Slices 22.2–22.7 all depend on the `AccessLogStore` and `AccessLogEntry` shapes from 22.1. Its three-crate footprint (config in `core`, store in `adapters`, runtime wiring in `cli`) plus a new daemon task make it unambiguously cross-cutting.
- **Proposed seam (22.4).** The access-log explainer correlates an entry against the snapshot and policy attachment active at the entry's timestamp — a real cross-phase boundary against Phase 7/8 snapshots and Phase 18 policy presets. Staged as `PROPOSED: access-log-explanation-snapshot` for `/phase-merge-review` to ratify or reject; per `seams.md` rules, `/tag-phase` cannot write directly into the registry.
- **Contract registry is currently empty.** `contracts.md` declares zero contracts and `contract-roots.toml` lists only the Phase 7 apply-path roots. Phase 22 introduces several public symbols (`AccessLogStore`, `AccessLogExplainer`, `Filter`, `Explanation` and friends). None is mandated as a contract root by the TODO, but `/phase-merge-review` should consider whether `AccessLogExplainer` and the access-log read surface warrant root entries given they sit on a cross-phase seam.
- **H16 / Phase 19 interaction.** 22.5 and 22.7 both touch the Phase 19 tool-gateway H16 envelope (`read.access-logs` scope, untrusted-input wrapper). This is a consume/verify relationship, not an introduction — the envelope wrapping itself lands in Phase 19 slice 19.6 per the TODO. It informed the cross-cutting tag on 22.5 (which must respect the gateway-vs-web shape split) but not on 22.7 (test-only verification).

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Notes from user
Auto-accepted.
