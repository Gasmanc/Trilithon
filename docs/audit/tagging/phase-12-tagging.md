# Phase 12 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:**
- CLAUDE.md (project instructions) — read
- docs/architecture/architecture.md — 1125 lines
- docs/architecture/trait-signatures.md — 735 lines
- docs/planning/PRD.md — 953 lines
- docs/adr/ — 16 ADRs present; read 0009, 0012, 0013 in full (cited by the phase)
- docs/todo/phase-12-rollback-preflight.md — 944 lines (slice list under analysis)
- docs/phases/phase-12-rollback-preflight.md — phase reference, read
- docs/architecture/seams.md — 138 lines
- docs/architecture/seams-proposed.md — read (proposed_seams empty)
- docs/architecture/contract-roots.toml — 36 lines
- docs/architecture/contracts.md — empty registry (0 contracts)
**Slices analysed:** 7

---

## Proposed Tags

### 12.1: `RollbackRequest` mutation type and snapshot reachability check
**Proposed tag:** [standard]
**Reasoning:** All file changes are confined to `core/crates/core` (`mutation.rs`, `snapshot/reachable.rs`, `snapshot/mod.rs`, `lib.rs`) — one crate, one layer, no I/O, no audit/tracing emission. However it is not [trivial]: it adds a new variant to the shared `TypedMutation` enum (a closed-set type that the validator, the tool gateway, and later slices all match on) and introduces a new public error type `RollbackPreconditionError` plus the `is_reachable` function. A variant addition to `TypedMutation` is structural surface other slices (12.5, 12.6) consume, but it stays inside one crate and modifies an enum rather than a *trait*, so it does not reach [cross-cutting].
**Affected seams:** none
**Planned contract additions:** `trilithon_core::mutation::RollbackRequest`, `trilithon_core::snapshot::reachable::RollbackPreconditionError`, `trilithon_core::snapshot::reachable::is_reachable` (none currently in contracts.md; the registry only tracks Phase 7 apply-path roots — these would be new candidate roots if `TypedMutation` is promoted)
**Confidence:** high
**If low confidence, why:** n/a

### 12.2: `Preflight` engine and condition algebra in `core`
**Proposed tag:** [cross-cutting]
**Reasoning:** This slice introduces a brand-new shared trait — `core::preflight::conditions::Condition` — that three subsequent slices (12.3 `UpstreamTcpReachableCondition` + `TlsIssuanceValidCondition`, 12.4 `ModuleAvailableCondition`) each implement. A new trait surface that other slices structurally depend on is an explicit [cross-cutting] trigger in the rubric. It also establishes the `PreflightReport`/`ConditionOutcome`/`ConditionStatus` report convention (the serialised wire shape consumed by the HTTP layer in 12.5 and the React types in 12.7) and the `ConditionId` kebab-case identifier convention that the override-audit notes in 12.6 must follow. Files are in one crate (`core`), but the trait-surface and cross-slice-convention triggers dominate.
**Affected seams:** none in `seams.md`. PROPOSED: `preflight-condition-engine` — boundary where `core::preflight::PreflightEngine` composes `Box<dyn Condition>` implementations supplied by adapters/cli wiring; multiple later slices and Phase 14 (`ProbeAdapter` reuse) cross this seam.
**Planned contract additions:** `trilithon_core::preflight::conditions::Condition`, `trilithon_core::preflight::PreflightEngine`, `trilithon_core::preflight::report::PreflightReport`, `trilithon_core::preflight::report::ConditionOutcome`, `trilithon_core::preflight::report::ConditionStatus`, `trilithon_core::preflight::report::ConditionId`
**Confidence:** high
**If low confidence, why:** n/a

### 12.3: TCP reachability and TLS validity probes in `adapters`
**Proposed tag:** [cross-cutting]
**Reasoning:** Files span two crates — `core/crates/core/src/preflight/upstream_tcp.rs` + `tls_issuance.rs` (the `Condition` impls live in `core` because the logic is pure) and `core/crates/adapters/src/probe_tokio.rs` + an `adapters` integration test. The slice implements two `Condition` trait instances, consumes two cross-layer trait surfaces (`core::probe::ProbeAdapter` and `core::caddy::CaddyClient`) injected as `Arc<dyn _>`, and introduces real I/O (TCP connect, TLS handshake, Caddy admin fetch). Multi-crate span plus the core↔adapters layer involvement are explicit [cross-cutting] triggers; it also extends `TokioProbeAdapter`, an existing adapter consumed by Phase 4 and Phase 14.
**Affected seams:** none in `seams.md`. PROPOSED: `preflight-condition-engine` (same proposed seam as 12.2 — this slice supplies the first concrete probe-backed conditions across the core/adapters boundary).
**Planned contract additions:** `trilithon_core::preflight::upstream_tcp::UpstreamTcpReachableCondition`, `trilithon_core::preflight::tls_issuance::TlsIssuanceValidCondition` (no new trait; both implement the 12.2 `Condition` trait)
**Confidence:** medium
**If low confidence, why:** Whether the `Condition` impl files truly land in `core` (as the slice text states) versus `adapters` affects the crate-span count, but the slice already touches `adapters/src/probe_tokio.rs` and consumes the core↔adapters boundary regardless, so the [cross-cutting] verdict holds either way.

### 12.4: `module-available` condition wired to capability cache
**Proposed tag:** [standard]
**Reasoning:** All files are in `core/crates/core` (`preflight/module.rs`, `snapshot/referenced_modules.rs`) — one crate, one layer, no I/O (the `CapabilitySet` is passed in as `Arc<CapabilitySet>`, already loaded; this slice does not read `capability_probe_results`). It implements one instance of the already-defined `Condition` trait and adds a pure helper `referenced_modules`. It does not define a new trait, does not cross a layer boundary, and emits no audit/tracing events. Implementing (not defining) one trait in a single crate is the textbook [standard] case.
**Affected seams:** none
**Planned contract additions:** `trilithon_core::preflight::module::ModuleAvailableCondition`, `trilithon_core::snapshot::referenced_modules::referenced_modules`
**Confidence:** high
**If low confidence, why:** n/a

### 12.5: HTTP endpoints `POST /api/v1/snapshots/{id}/preflight` and `/rollback`
**Proposed tag:** [cross-cutting]
**Reasoning:** Files span two crates — `core/crates/cli/src/http/snapshots.rs` + `http/mod.rs` (the `cli`/entry layer) and `core/crates/core/src/preflight/override.rs` (the `core` layer). The slice crosses the core↔cli layer boundary, wires `core::reconciler::Applier::rollback` (a contract root tracked in `contract-roots.toml`), `core::storage::Storage`, and `PreflightEngine` together, performs HTTP I/O, and emits five audit kinds (`mutation.submitted`, `config.rolled-back`, `config.applied`, `config.apply-failed`, `mutation.rejected`) plus four tracing events. It also references three+ ADRs (0009, 0012) and PRD T1.3 with §7.6/§10/§12.1 architecture sections. Multi-crate span, layer crossing, and the audit-emission surface other slices (12.6, 12.7) depend on are all [cross-cutting] triggers.
**Affected seams:** `applier-audit-writer` (the rollback path produces apply outcomes that must emit typed audit rows — exercises `ApplyOutcome`/`AuditEvent`), `applier-caddy-admin` (`Applier::rollback` drives Caddy via the applier seam), `snapshots-config-version-cas` (the `expected_version` check at step 3 hits the CAS version gate). PROPOSED: `rollback-http-preflight` — the HTTP boundary where preflight + override validation gate an apply; consumed structurally by 12.6 and 12.7.
**Planned contract additions:** `trilithon_core::preflight::override::OverrideSet`, `trilithon_core::preflight::override::OverrideError`, `trilithon_core::preflight::override::validate_overrides` (handlers in `cli` are entry-layer, not contract surface)
**Confidence:** high
**If low confidence, why:** n/a

### 12.6: Audit row authoring for rollback request, overrides, and apply outcome
**Proposed tag:** [cross-cutting]
**Reasoning:** Files span two crates — `core/crates/core/src/audit.rs` (extends the shared `core::audit::AuditEvent` enum, a Phase 6 type whose `Display` impl is the authoritative Rust↔wire `kind` mapping per architecture §6.6) and `core/crates/adapters/src/audit_log_store.rs` (the §6.6 vocabulary table consumed by `record_audit_event`), plus call-site wiring in `core/crates/cli/src/http/snapshots.rs` — three crates across core/adapters/cli. It modifies a shared, cross-phase type (`AuditEvent`) and touches the audit-vocabulary enforcement boundary that every phase emitting audit rows depends on. Even though no new wire `kind` strings are introduced, extending `AuditEvent` and crossing core↔adapters↔cli are explicit [cross-cutting] triggers.
**Affected seams:** `applier-audit-writer` (this slice is the audit-row authoring for the rollback apply outcome — directly exercises `AuditEvent` and the apply-outcome→audit contract), `apply-audit-notes-format` (the `notes` JSON payload format for override rows must follow the established serialisation contract).
**Planned contract additions:** `trilithon_core::audit::AuditEvent` variants `RollbackRequested`, `RollbackOverrideAccepted`, `ConfigRolledBack` (extension of an existing shared enum — `AuditEvent` is a candidate contract root)
**Confidence:** high
**If low confidence, why:** n/a

### 12.7: Web UI snapshot history tab and rollback dialog
**Proposed tag:** [standard]
**Reasoning:** All files are under `web/src/` (`features/routes/HistoryTab.tsx`, `features/rollback/RollbackDialog.tsx`, `api.ts`, `types.ts`, and tests) — one project (the React frontend), no Rust crate, no trait, no layer boundary in the Rust sense. It consumes the HTTP contract from slice 12.5 but introduces no new server-side surface. The TypeScript `types.ts` mirrors Rust shapes but mirroring is not a shared-contract modification. The slice may ship a thin read endpoint wrapper for `/routes/{id}/history` only if Phase 11 did not — a contingent, self-contained addition. Self-contained frontend feature work touching one module group is [standard].
**Affected seams:** none
**Planned contract additions:** none (frontend-only; TypeScript types are not registry contracts)
**Confidence:** medium
**If low confidence, why:** If the `/api/v1/routes/{routeId}/history` endpoint is genuinely absent from Phase 11 and must be added here, that adds a `cli` crate file and a second layer — which would push the slice toward [cross-cutting]; the slice text treats this as a low-probability fallback, so [standard] is the primary tag.

---

## Summary
- 2 trivial → **0 trivial**
- 3 standard (12.1, 12.4, 12.7)
- 4 cross-cutting (12.2, 12.3, 12.5, 12.6)
- 2 low-confidence (12.3, 12.7)

## Notes

- **No [trivial] slices.** Every slice in Phase 12 either introduces a trait, implements one, crosses a crate boundary, performs I/O, or emits audit/tracing events. Phase 12 is a vertically integrated feature (rollback with preflight) that wires `core` → `adapters` → `cli` → `web`, so trivial-grade work is absent by design.

- **Trait-surface concentration in 12.2.** The `Condition` trait is the single most load-bearing new surface in the phase. Tagging 12.2 [cross-cutting] ensures the implementer reasons about object-safety (the trait must stay `Box<dyn Condition>`-able), the `async-trait` convention, the `ConditionId` kebab-case identifier convention shared with 12.6's override-audit notes, and the test-double convention from trait-signatures.md §"Test doubles". Under-reasoning here would cascade into 12.3 and 12.4.

- **Contract registry is empty.** `contracts.md` declares 0 contracts; `contract-roots.toml` only lists Phase 7 apply-path roots. None of Phase 12's planned additions are currently tracked. If `/phase-merge-review` decides the preflight surface (`Condition`, `PreflightReport`, `PreflightEngine`, `AuditEvent` extensions, `TypedMutation::RollbackRequest`) should be registry-tracked, `contract-roots.toml` needs new entries — a contract change in its own right.

- **Proposed seams.** Two new seams are proposed but NOT written to `seams-proposed.md` by this analysis (tagging is read-only on the seam registry; `/tag-phase` writes proposals only on user approval). Candidates: `preflight-condition-engine` (12.2/12.3 — the `Box<dyn Condition>` composition boundary, also reused by Phase 14 upstream-health) and `rollback-http-preflight` (12.5 — the HTTP preflight/override gate before apply). 12.5 and 12.6 also exercise the existing `applier-audit-writer`, `applier-caddy-admin`, and `snapshots-config-version-cas` seams; their cross-phase tests under `tests/cross_phase/` should be reviewed for rollback-path coverage.

- **`AuditEvent` is cross-phase shared state.** Slice 12.6 extends `core::audit::AuditEvent`, a Phase 6 type. Architecture §6.6 mandates the Rust variant ↔ wire `kind` mapping be one-to-one and that vocabulary changes land in the same commit. No new `kind` strings are added (all five reuse existing §6.6 entries), which keeps 12.6 from being even more invasive — but extending the enum still touches every phase that pattern-matches `AuditEvent`.

- **Dependency chain is strictly linear** (12.1 → 12.2 → {12.3, 12.4} → 12.5 → 12.6 → 12.7 with 12.7 depending on 12.5). The two [standard] core slices (12.1, 12.4) are the safe parallelisation-free checkpoints; the four [cross-cutting] slices each warrant a fresh full-context window under `/phase`.

ANALYSED: phase-12 — 7 slices tagged (0 trivial, 3 standard, 4 cross-cutting, 2 low-confidence). Output: docs/audit/tagging/phase-12-tagging.md

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Modifications (if any)
None.

### Notes from user
None.
