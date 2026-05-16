# Phase 16 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:** CLAUDE.md, docs/architecture/architecture.md, docs/architecture/trait-signatures.md, docs/planning/PRD.md, docs/adr/0001–0016 (directory listing; ADR set spans 0001–0016, phase references ADR-0001–0014), docs/todo/phase-16-tier-1-hardening.md, docs/architecture/seams.md, docs/architecture/contract-roots.toml, docs/architecture/contracts.md
**Slices analysed:** 9

## Proposed Tags

### 16.1: Failure-mode batch A — Caddy unreachable (startup and mid-flight)
**Proposed tag:** [cross-cutting]
**Reasoning:** The slice adds a new shared test helper `CaddyStub` in `adapters/tests/helpers/` that later slices (16.7's failure-mode chain, 16.8's demo) compose against, and the integration test spans cli → adapters → core, exercising the `Applier`, `CaddyClient`, and `Storage` traits plus the apply-path contracts in `contract-roots.toml`. It asserts behaviour at the applier-caddy-admin and applier-audit-writer seams (back-off budget, untouched desired-state pointer, single typed `config.apply-failed` audit row). It references PRD T1.1 and architecture §8.1/§10/§12.1 — three-plus authority IDs and a cross-layer reach.
**Affected seams:** applier-caddy-admin, applier-audit-writer, snapshots-config-version-cas
**Planned contract additions:** none (asserts existing `trilithon_core::reconciler::ApplyError`, `ApplyOutcome`; `CaddyStub` is a test helper, not a contract)
**Confidence:** high
**If low confidence, why:** n/a

### 16.2: Failure-mode batch B — SQLite locked, SQLite corruption
**Proposed tag:** [cross-cutting]
**Reasoning:** Two integration tests that drive the daemon end-to-end, exercising the `Storage` trait's `SqliteBusy`/maintenance-mode paths and asserting the `storage.integrity-check.failed` tracing event and a typed `system` audit row — observability conventions other slices depend on. The corruption test asserts a daemon-wide maintenance-mode state transition (every mutation endpoint returns 503), which crosses cli/adapters/core and the HTTP surface. References PRD T1.6, hazard H14, and architecture §10/§12.1/§14.
**Affected seams:** snapshots-config-version-cas (CAS path under contention)
**Planned contract additions:** none (uses existing `StorageError::SqliteBusy`, `StorageError::Integrity`)
**Confidence:** high
**If low confidence, why:** n/a

### 16.3: Failure-mode batch C — Docker socket gone, capability probe failure
**Proposed tag:** [cross-cutting]
**Reasoning:** Integration tests spanning cli → adapters, exercising `DockerWatcher` and the capability probe path, and asserting cross-layer behaviour: negative-space Docker handling, the `caddy.capability-probe-completed` audit kind, the `caddy.capability-probe.completed` tracing event, and validator rejection in `core` against an unknown module. It touches the Docker boundary, the Caddy capability boundary, and `core` validation in one slice (hazard H5, PRD T1.11/T2.1 substrate).
**Affected seams:** none (no registered seam covers the Docker or capability-probe boundary)
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 16.4: Failure-mode batch D — bootstrap credentials unwritable, master-key access denied
**Proposed tag:** [cross-cutting]
**Reasoning:** Spans cli (process exit code 3, structured stderr) and adapters (`SecretsVault` keychain/file fallback), exercising `core::secrets::SecretsVault` and asserting a `system` audit row plus a `KeyringUnavailable` tracing warning. The slice explicitly flags a possible new audit kind (`secrets.master-key-fallback-engaged`) that would require an architecture §6.6 vocabulary change — an audit-convention decision other phases follow. References PRD T1.14/T1.15, hazard H13, architecture §10/§11.
**Affected seams:** none
**Planned contract additions:** none (the `secrets.master-key-fallback-engaged` kind already exists in §6.6; open question #2 in the TODO is about whether to *use* it vs the `system` family — no new contract symbol)
**Confidence:** medium
**If low confidence, why:** The audit-kind decision (open question #2) could become a §6.6 vocabulary change in this slice's commit, which is a cross-cutting convention edit; the tag holds regardless of resolution.

### 16.5: Performance verification
**Proposed tag:** [cross-cutting]
**Reasoning:** Adds a `criterion` bench harness and wall-clock assertions in `cli` plus a Vitest perf test in `web/` — two crates and the frontend in one slice. The measurements exercise nearly every trait surface (cold start, mutation apply percentiles, drift tick, idle memory) and assert against architecture §13's shared performance budget, which is a project-wide contract. Spans the Rust workspace and the TypeScript frontend; references PRD T1.1/T1.4/T1.8/T1.13.
**Affected seams:** applier-caddy-admin, snapshots-config-version-cas (mutation-apply percentile path)
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 16.6: Security review document covering H1–H17
**Proposed tag:** [cross-cutting]
**Reasoning:** Authors `docs/architecture/security-review.md`, a project-wide document confirming the implementation against all 17 hazards and reviewed end-to-end against ADR-0001 through ADR-0014 — far more than three authority IDs. Although the deliverable is a doc plus one small completeness test in `cli`, the content is a cross-cutting audit spanning every layer and every phase's controls.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 16.7: Strict-mode `just check` upgrade
**Proposed tag:** [cross-cutting]
**Reasoning:** Modifies the project-wide gate (`justfile`) and CI workflow (`.github/workflows/ci.yml`), chaining four strict suites that span `trilithon-core`, `trilithon-adapters`, and `trilithon-cli`. This is an infrastructure change every subsequent phase's gate runs through — a convention other slices depend on. Introduces a new `secrets::leak_simulation` test in `core` if not already present.
**Affected seams:** none (the gate is project infrastructure, not a code seam)
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 16.8: End-to-end demo script
**Proposed tag:** [cross-cutting]
**Reasoning:** Adds `core/crates/cli/tests/demo_e2e.rs`, a new CI workflow (`.github/workflows/demo.yml`), and a narrative doc. The test drives all eight scripted steps through the full HTTP API against a real Caddy 2.8 container — bootstrap, route create, Caddyfile import, drift, adopt, rollback, secrets reveal — exercising every layer and asserting roughly nine audit kinds and four tracing events. References PRD T1.1–T1.15 (the full Tier 1 set).
**Affected seams:** applier-caddy-admin, applier-audit-writer, snapshots-config-version-cas
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 16.9: Documentation pass — doc-comments, header comments, user README
**Proposed tag:** [cross-cutting]
**Reasoning:** Touches every file under `core/crates/**/*.rs` and `web/src/**/*.{ts,tsx}` (additive doc/header comments), adds a new local ESLint rule and registers it in `web/eslint.config.js`, and authors `docs/README.md`. The new ESLint rule is a lint convention every future web file must follow, and `cargo doc -D rustdoc::missing_docs` becomes an enforced gate. Spans both the Rust workspace and the frontend.
**Affected seams:** none
**Planned contract additions:** none (doc comments are additive; no new public symbols)
**Confidence:** medium
**If low confidence, why:** The changes are additive and per-file with no semantic edits, which reads "trivial" in isolation; the cross-cutting tag is driven by the workspace-wide ESLint-rule convention and the new enforced doc gate rather than by behavioural risk.

## Summary
- 0 trivial / 0 standard / 9 cross-cutting / 2 low-confidence (16.4, 16.9 — both medium, not low)

Note: no slice scored *low* confidence; 16.4 and 16.9 are medium and flagged above. Counting strictly, low-confidence = 0.

## Notes

- Phase 16 introduces no new product surface — it is the Tier 1 closing gate. Every slice is a verification, measurement, gate, or documentation sweep that by construction spans multiple crates and exercises shared trait surfaces. That is why all nine slices land on [cross-cutting]: the rubric's cross-cutting triggers (spans multiple crates, crosses layer boundaries, references 3+ authority IDs, introduces a convention others follow) fire on every slice. There is no slice confined to one crate with no cross-layer reach.
- No new contract symbols are planned in any slice. The apply-path contracts in `contract-roots.toml` (`Applier`, `ApplyOutcome`, `ApplyAuditNotes`, `ApplyError`, `AppliedState`, `ReloadKind`) are *exercised* by the failure-mode and demo tests but not extended.
- No new seams are proposed. The existing five seams (`applier-caddy-admin`, `applier-audit-writer`, `snapshots-config-version-cas`, `apply-lock-coordination`, `apply-audit-notes-format`) are exercised by 16.1/16.2/16.5/16.8. The Docker boundary and the capability-probe boundary exercised by 16.3 are *not* registered seams; if cross-phase tests there are wanted, `/tag-phase` should stage proposed seams in `seams-proposed.md` — but Phase 16 only adds failure-mode tests, not seam tests, so no staging is required by this phase.
- `CaddyStub` (16.1) is a test helper under `adapters/tests/helpers/`, not a public contract or seam symbol. Later slices reuse it; that reuse is the cross-cutting signal for 16.1, not a registry change.
- Open question #2 in the phase TODO (whether 16.4 emits `secrets.master-key-fallback-engaged` or reuses the `system` family) — the architecture §6.6 vocabulary already lists `secrets.master-key-fallback-engaged`, so either choice is conformant; no §6.6 edit is forced. The cross-cutting tag on 16.4 stands regardless.
- 16.7 modifies CI and the gate: any implementer of a later phase inherits the stricter `just check`. Treat this slice as a hard dependency for the rest of the phase's exit checklist.

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Notes from user
Auto-accepted.
