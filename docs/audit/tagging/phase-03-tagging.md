# Phase 03 — Tagging Analysis
**Generated:** 2026-05-02
**Model:** opus (extended thinking)
**Documents read:**
- CLAUDE.md (184 lines)
- docs/architecture/architecture.md (1124 lines)
- docs/architecture/trait-signatures.md (734 lines)
- docs/planning/PRD.md (952 lines)
- docs/phases/phase-03-caddy-adapter.md (91 lines)
- docs/adr/0001-caddy-as-the-only-supported-reverse-proxy.md (135 lines)
- docs/adr/0002-caddy-json-admin-api-as-source-of-truth.md (133 lines)
- docs/adr/0010-two-container-deployment-with-unmodified-official-caddy.md (182 lines)
- docs/adr/0011-loopback-only-by-default-with-explicit-opt-in-for-remote-access.md (194 lines)
- docs/adr/0013-capability-probe-gates-optional-caddy-features.md (187 lines)
- docs/adr/0015-instance-ownership-sentinel-in-caddy-config.md (217 lines)
- docs/adr/0003-rust-three-layer-workspace-architecture.md (160 lines)
- docs/todo/phase-03-caddy-adapter.md (860 lines)

**Slices analysed:** 8

---

## Proposed Tags

### 3.1: `CaddyClient` trait, `CaddyError`, value types
**Proposed tag:** [cross-cutting]
**Reasoning:** Introduces a new shared trait surface (`CaddyClient`) in `core` that every subsequent slice in Phase 3 (3.4 implements; 3.5/3.6/3.7 consume) and downstream phases (Phase 4 mutation algebra, Phase 7 apply path) depend on. Although all files live in one crate, "introduces a new trait surface that other slices implement" is an explicit cross-cutting trigger in the rubric. Also defines the `CaddyError` taxonomy that flows across layer boundaries and references trait-signatures §2, ADR-0001, ADR-0002.
**Confidence:** high

### 3.2: `CaddyCapabilities` value type and capability migration `0002`
**Proposed tag:** [cross-cutting]
**Reasoning:** Spans two crates (`core` for the value type, `adapters` for the SQL migration `0002_capability_probe.sql`) and introduces a schema migration that slices 3.5, 3.6, and 3.8 structurally depend on (insert/update of `capability_probe_results`). Migration-other-slices-depend-on is an explicit cross-cutting trigger. The `CapabilitySet` alias is also reused by Phase 4.
**Confidence:** high

### 3.3: Configuration validator: loopback-only, `--allow-remote-admin` exits 2
**Proposed tag:** [cross-cutting]
**Reasoning:** Files span two crates and cross the adapters↔cli layer boundary: adds a validator in `adapters`, modifies `adapters/src/config_loader.rs`, and modifies `cli/src/cli.rs` to add a global flag and exit-code mapping. References ADR-0011 directly and contributes to the H1 mitigation invariant. Modifying the shared `Cli` struct is a cross-phase touch point.
**Confidence:** high

### 3.4: `HyperCaddyClient` over Unix socket and loopback-mTLS
**Proposed tag:** [cross-cutting]
**Reasoning:** Although all files are within `adapters`, this slice establishes the `traceparent` header convention that every future Caddy admin call (Phase 4 apply, Phase 7, Phase 8 drift, Phase 12 rollback) must follow — explicitly an "introduces tracing/audit/logging convention other slices must follow" trigger. It also implements the cross-layer `CaddyClient` trait from `core`, references ADR-0002 and ADR-0010, and adds significant external I/O (hyper, hyperlocal, hyper-rustls). The phase-reference spec calls out the `traceparent` propagation invariant at the sign-off checklist level.
**Confidence:** high

### 3.5: Capability probe at startup with persisted row
**Proposed tag:** [standard]
**Reasoning:** All three new files live in `adapters/src/caddy/`, implementing functionality on top of the existing `CaddyClient` trait and `Storage` from prior slices — no new trait introduced and no shared trait modified. Emits the `caddy.capability-probe.completed` tracing event, but per architecture §12.1 this is a documented event consumed (not redefined) here, and slice 3.6 re-emits the same event rather than depending on a new convention. Single layer (adapters) and the I/O is contained.
**Confidence:** medium
**If low confidence, why:** Emits an event that 3.6 also emits — borderline "convention" but the convention is set in architecture §12.1, not by this slice; standard is the better fit.

### 3.6: Reconnect loop with capped exponential backoff
**Proposed tag:** [cross-cutting]
**Reasoning:** Introduces a brand-new `ShutdownObserver` trait that is a shared lifecycle abstraction expected to be reused (the slice list flags it explicitly as "a new shared lifecycle trait"). New trait surface other slices will implement — explicit cross-cutting trigger. Emits the `caddy.connected`/`caddy.disconnected` event pair that becomes the canonical liveness signal for downstream phases (drift detection, apply-path retry).
**Confidence:** high

### 3.7: Ownership sentinel write and `--takeover`
**Proposed tag:** [cross-cutting]
**Reasoning:** Spans `adapters` and `cli` (modifies `cli.rs` again to add `--takeover`), introduces the `<data_dir>/installation_id` file convention that future phases (audit log, snapshot writer) reference for installation identity, and stages an audit-event variant the Phase 6 audit writer must consume. References ADR-0015 and the architecture §6.6 audit-kind question (Open question 4). Modifies a shared `Cli` struct — the second time in Phase 3.
**Confidence:** high

### 3.8: Wire startup; integration tests against real Caddy 2.8
**Proposed tag:** [cross-cutting]
**Reasoning:** Files span three crates (`cli/src/main.rs`, `cli/src/exit.rs`, `adapters/tests/caddy/end_to_end.rs`, `core/README.md`), cross every layer boundary, wire all Phase 3 components into startup, and add new `ExitCode` mappings (`StartupPreconditionFailure` for `SentinelError::Conflict` and `EndpointPolicyError::NonLoopback`). References 5 ADRs (0001/0002/0011/0013/0015) and emits the `daemon.started` event as the post-condition for the entire phase. Textbook cross-cutting integration slice.
**Confidence:** high

---

## Summary
- 0 trivial
- 1 standard
- 7 cross-cutting
- 0 low-confidence (require human review)

## Notes
Phase 3 is unusually integration-heavy: it introduces a new trait surface (`CaddyClient`), a new lifecycle trait (`ShutdownObserver`), a new schema migration, a new tracing convention (`traceparent`), a new file-system convention (`installation_id`), and modifies the shared `Cli` struct twice (3.3 and 3.7). The high cross-cutting count reflects that Phase 3 is the bridge from the persistence-only daemon (Phase 2) to a Caddy-aware daemon, so most slices either cross layer boundaries or establish conventions that downstream phases consume. Slice 3.5 is the lone "standard" because it sits cleanly inside `adapters` and only consumes existing abstractions. There are real implementation dependencies between slices (3.4 depends on 3.1+3.3; 3.5 depends on 3.4; 3.6 depends on 3.4+3.5; 3.7 depends on 3.4; 3.8 depends on 3.5–3.7), which the slice-plan summary table already documents.

---

## User Decision
**Date:** 2026-05-02
**Decision:** accepted

### Modifications
None.

### Notes from user
None.
