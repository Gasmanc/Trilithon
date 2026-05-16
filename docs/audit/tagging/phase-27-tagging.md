# Phase 27 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:** CLAUDE.md, docs/architecture/architecture.md, docs/architecture/trait-signatures.md, docs/planning/PRD.md, docs/adr/0001–0016, docs/todo/phase-27-tier-2-hardening.md, docs/architecture/seams.md, docs/architecture/contract-roots.toml, docs/architecture/contracts.md (contracts.md not present; contract-roots.toml + seams.md used as the registry surface)
**Slices analysed:** 10

## Proposed Tags

### 27.1: T2.10 conflict + rebase end-to-end flow test
**Proposed tag:** [cross-cutting]
**Reasoning:** The flow test drives the full mutation lifecycle across `core` (Mutation, AuditEvent) and `adapters` (TestHarness, mutation queue, snapshot store, applier) and asserts an ordered audit-kind sequence (`mutation.submitted` → `mutation.conflicted` → `mutation.rebased.*` → `config.applied`) that is the contract several phases depend on. It also adds `.github/workflows/e2e-flows.yml`, a shared CI convention registering this and the five subsequent flow tests. It references ADR-0012, PRD T2.10, and the architecture §6.6 audit vocabulary — a multi-source, cross-layer, multi-phase integration surface.
**Affected seams:** snapshots-config-version-cas, applier-caddy-admin, applier-audit-writer, apply-audit-notes-format
**Planned contract additions:** none (exercises existing `trilithon_core::reconciler::*` and `Storage` CAS contracts; adds no new public symbols)
**Confidence:** high
**If low confidence, why:** —

### 27.2: T2.2 policy preset capability degradation flow
**Proposed tag:** [standard]
**Reasoning:** A single new test file in the `adapters` crate exercising the `PresetRegistry` trait and capability-probe degradation through `TestHarness`. It introduces no new traits, no new audit kinds (`policy-preset.attached`, `caddy.capability-probe-completed` already exist in §6.6), and no shared convention. It is a self-contained verification flow over one trait surface, depends only on Phase 18, and references two ADRs (0013, 0016) — below the 3-ADR cross-cutting threshold.
**Affected seams:** none (no current seam covers the preset registry; the flow only consumes existing behaviour)
**Planned contract additions:** none
**Confidence:** medium
**If low confidence, why:** It rides the shared `e2e-flows.yml` workflow introduced in 27.1, but does not author or modify it, so it stays standard.

### 27.3: T2.3 + T2.4 explain-then-propose end-to-end flow
**Proposed tag:** [cross-cutting]
**Reasoning:** The flow spans the tool gateway (`ToolGateway` trait in `core`, adapter in `adapters`), the proposal store, the mutation queue, and the applier, and asserts a seven-event audit sequence (`tool-gateway.session-opened` through `config.applied`). It exercises the language-model trust boundary (hazard H16), depends on two upstream phases (19, 20), and references ADR-0008 plus PRD T2.3/T2.4. It crosses the explain→propose→approve→apply layer chain end to end.
**Affected seams:** applier-caddy-admin, applier-audit-writer (PROPOSED: tool-gateway-proposal-pipeline — explain/propose path has no current seam entry)
**Planned contract additions:** none
**Confidence:** medium
**If low confidence, why:** Whether the tool-gateway→proposal pipeline already has an unlisted seam is uncertain; flagged as PROPOSED for `/phase-merge-review` to ratify.

### 27.4: T2.1 + T2.11 Docker discovery wildcard-callout flow
**Proposed tag:** [cross-cutting]
**Reasoning:** The flow spans the `DockerWatcher` trait (`core`/`adapters`), the proposal store, the wildcard-certificate callout logic, and the applier, asserting the proposal→ack→apply chain and a 5-second discovery latency budget. It exercises hazards H3 and H11, depends on Phase 21, and references ADR-0007 plus PRD T2.1/T2.11. It crosses the Docker event boundary into the apply path — a multi-crate, multi-layer integration.
**Affected seams:** applier-caddy-admin, applier-audit-writer (PROPOSED: docker-discovery-proposal-pipeline — Docker discovery path has no current seam entry)
**Planned contract additions:** none
**Confidence:** medium
**If low confidence, why:** The Docker-discovery→proposal seam is not in seams.md; flagged PROPOSED pending ratification.

### 27.5: T2.5 + T2.6 access log viewer 10-million-line flow
**Proposed tag:** [standard]
**Reasoning:** A single new test file in `adapters` exercising the access-log store's `ingest_synthetic`, `filter`, and `explain` calls against a 200ms performance budget. It emits no new audit kinds, introduces no traits, and touches one adapter subsystem (the rolling access-log store). The flow calls the in-process API only and explicitly does not exercise the HTTP boundary. References two PRD IDs (T2.5, T2.6) and the §13 performance budget — within standard scope.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** —

### 27.6: T2.9 + T2.12 native bundle round-trip flow
**Proposed tag:** [cross-cutting]
**Reasoning:** The flow exports a native bundle and restores it, asserting canonical-JSON byte equality of `DesiredState` and a cross-machine restore audit row (`system.restore-cross-machine`). It spans the export adapter, the secrets vault (`SecretsVault` trait), the storage layer, and the canonical-JSON serialiser in `core`, and depends on two upstream phases (25, 26). It references ADR-0009, ADR-0014, the bundle-format-v1 spec, and PRD T2.9/T2.12 — a multi-source, cross-layer migration/round-trip surface touching the master-key handoff.
**Affected seams:** applier-audit-writer (PROPOSED: bundle-export-restore-roundtrip — no current seam covers export/restore)
**Planned contract additions:** none
**Confidence:** medium
**If low confidence, why:** Canonical-JSON byte equality is a cross-phase invariant that may warrant a dedicated seam; flagged PROPOSED.

### 27.7: Performance verification at 5,000 routes
**Proposed tag:** [standard]
**Reasoning:** Adds a criterion benchmark file (`perf_5000_routes.rs`) in `adapters` and a `perf.yml` CI workflow enforcing four budgets. The benchmark seeds a harness and measures existing behaviour; it introduces no traits, no audit kinds, no new contracts, and does not cross a layer boundary in code. It references a `perf-soak` binary (likely an existing or trivially-added `cli`/`adapters` bin) and PRD T1.1/T1.4/T1.8 plus §13 — verification tooling, not new system behaviour.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** medium
**If low confidence, why:** The `perf-soak` binary may not yet exist; if it must be newly created in `cli`, that nudges toward cross-cutting, but the slice frames it as a measurement harness.

### 27.8: Install/upgrade matrix
**Proposed tag:** [cross-cutting]
**Reasoning:** Adds a CI matrix workflow exercising six deployment cells (Compose, systemd, schema upgrade) plus a `schema_upgrade_tier1_to_tier2` integration test asserting a Phase 16 database upgrades cleanly to the Phase 27 schema. The schema-upgrade test is a migration verification other slices depend on, emits a `storage.migrations.applied` audit row, and spans deploy artefacts, the migration runner in `adapters`, and CI. It depends on three phases (23, 24, 26) and references architecture §14 plus PRD T2.7/T2.8/T2.12.
**Affected seams:** none (migration runner has no seam entry; the schema-upgrade contract is architecture §14, not a registry seam)
**Planned contract additions:** none
**Confidence:** medium
**If low confidence, why:** The bulk of the slice is CI/deploy plumbing; the cross-cutting weight comes from the schema-upgrade migration test, which is a genuine cross-phase dependency.

### 27.9: Security review document and H11/H16 dedicated re-review
**Proposed tag:** [standard]
**Reasoning:** Authors `docs/architecture/security-review.md` (one section per hazard H1–H17) and a heading-lint shell script. It is a documentation deliverable plus a lint; it touches no Rust crate, no trait, no audit/tracing vocabulary, and no layer boundary. Though it references all 17 hazards, it produces no code and introduces no convention other slices follow — the rubric's "3+ ADRs/PRD IDs" cross-cutting trigger is about code-bearing slices, not a docs audit.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** —

### 27.10: V1 release readiness: release notes, doc audit, Tier 1 regression guard
**Proposed tag:** [cross-cutting]
**Reasoning:** Beyond release notes and a docs index, this slice enables `rustdoc::missing_docs = "deny"` in `core/Cargo.toml` workspace lints — a workspace-wide change forcing every public Rust item across all crates to carry a doc comment — and adds the `tier-1-regression.yml` workflow re-running every Phase 16 acceptance check on every push. The workspace-lint change is a shared convention every crate must satisfy, affecting all layers at once. It references ADRs 0001–0016 and PRD T1.1–T1.15/T2.1–T2.12.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** —

## Summary
- 4 trivial / 4 standard / 6 cross-cutting / 5 low-confidence

Note: counts — 0 trivial, 4 standard (27.2, 27.5, 27.7, 27.9), 6 cross-cutting (27.1, 27.3, 27.4, 27.6, 27.8, 27.10), 5 medium-confidence (27.2, 27.3, 27.4, 27.6, 27.7); 0 low-confidence.

## Notes

- Phase 27 contains **no `[trivial]` slices**. Every slice is at minimum a self-contained verification flow; none is a one-file change with no shared surface.
- `docs/architecture/contracts.md` was named in the read list but is not present in the repo. `contract-roots.toml` (Phase 7 apply-path roots only) and `seams.md` (five Phase-7 seams) are the actual registry surface. No Phase 27 slice adds a public contract symbol — every slice is verification, benchmarking, or documentation, so the registry surface is unchanged.
- Four flow slices (27.3 explain/propose, 27.4 Docker discovery, 27.6 bundle round-trip) exercise integration boundaries that have **no seam entry** in `seams.md`. They are flagged `PROPOSED:` so `/phase-merge-review` can decide whether to ratify dedicated seams. The current seam registry only covers the Phase-7 apply path; Tier-2 integration boundaries are under-represented and Phase 27 is the natural place to surface that gap.
- 27.1 authors the shared `e2e-flows.yml` workflow; 27.2–27.6 are registered into it but do not modify it. Only 27.1 carries the "introduces a CI convention others follow" weight.
- The cross-cutting verdicts for the flow slices (27.1, 27.3, 27.4, 27.6) rest on the audit-kind ordering assertions: each test pins a multi-event sequence from §6.6 that spans `core` and `adapters`, making them de facto cross-phase contract tests.
- 27.10's cross-cutting status is driven specifically by the workspace-lint change (`rustdoc::missing_docs = "deny"`), not by the release-notes authoring — the lint forces a uniform doc-comment requirement on every crate.

ANALYSED: phase-27 — 10 slices tagged (0 trivial, 4 standard, 6 cross-cutting, 0 low-confidence). Output: docs/audit/tagging/phase-27-tagging.md

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Notes from user
Auto-accepted.
