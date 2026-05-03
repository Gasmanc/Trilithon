# Phase 04 — Tagging Analysis
**Generated:** 2026-05-03
**Model:** opus (extended thinking)
**Documents read:**
- CLAUDE.md (184 lines)
- docs/architecture/architecture.md (1124 lines)
- docs/architecture/trait-signatures.md (734 lines)
- docs/planning/PRD.md (952 lines)
- docs/phases/phase-04-mutation-algebra.md (245 lines)
- docs/adr/0009-immutable-content-addressed-snapshots-and-audit-log.md (185 lines)
- docs/adr/0012-optimistic-concurrency-on-monotonic-config-version.md (186 lines)
- docs/adr/0013-capability-probe-gates-optional-caddy-features.md (187 lines)
- docs/adr/0016-route-policy-attachment-records-preset-version.md (116 lines)
- docs/adr/0003-rust-three-layer-workspace-architecture.md (160 lines)
- docs/todo/phase-04-mutation-algebra.md (1382 lines)
**Slices analysed:** 10

---

## Proposed Tags

### 4.1: Aggregate identifier types and primitive value types
**Proposed tag:** [trivial]
**Reasoning:** All file changes within `core/crates/core/src/model/` plus a `lib.rs` mod registration. Pure newtype wrappers and primitive value types (`UnixSeconds`, identifier types). No traits introduced, no I/O, no cross-layer dep, no audit/tracing event. While many later slices will *use* these identifiers, that is normal type reuse — the slice itself does not establish a shared convention beyond a few `pub struct` newtypes.
**Confidence:** high

### 4.2: Route, Upstream, MatcherSet, HeaderRules, RedirectRule
**Proposed tag:** [standard]
**Reasoning:** Multiple cohesive value-type files within a single module (`core/src/model/`), plus a validator function (RFC 952/1123 hostname validation per phase doc §"DesiredState"). One layer, one crate, no traits, no I/O. Slightly more than trivial because of the validator function and the breadth of types, but clearly self-contained — does not introduce a convention other slices must conform to beyond field shapes.
**Confidence:** high

### 4.3: TLS, GlobalConfig, policy attachment value types
**Proposed tag:** [standard]
**Reasoning:** Several value-type files within `core/src/model/`. References ADR-0016 (single ADR) for the `RoutePolicyAttachment { preset_id, preset_version }` shape. No traits, no I/O, no cross-layer dep. Bigger than trivial because it spans multiple files (tls, global, policy) and pins a contract (preset version on attachment) that Phase 18 will read — but contract is local to value-type field selection.
**Confidence:** high

### 4.4: `DesiredState` aggregate, serde round-trip, BTreeMap invariants
**Proposed tag:** [standard]
**Reasoning:** Single new file plus mod.rs edit, all within `core/src/model/`. Aggregates types from 4.1–4.3. The BTreeMap-ordering invariant is load-bearing for the Phase 5 snapshot writer's content-addressed hash (ADR-0009), which is a structural dependency — but it is established by data-type choice, not by introducing a shared trait or tracing convention. Stays standard rather than cross-cutting because the dependency is a deterministic-serialisation property, not a multi-crate API surface.
**Confidence:** medium
**If low confidence, why:** Borderline — the BTreeMap invariant *is* something Phase 5 structurally depends on; arguments exist for cross-cutting on that basis.

### 4.5: Patch types (RoutePatch, UpstreamPatch, ParsedCaddyfile)
**Proposed tag:** [trivial]
**Reasoning:** Two new files in a fresh `mutation/` module, all within `core`. Patch value types only — no traits, no I/O, no audit event, no cross-layer dep. These are inputs to the `Mutation` enum closed in slice 4.6 and consumed only inside `core/mutation/`. Pure type addition.
**Confidence:** high

### 4.6: `Mutation` enum, `MutationId`, `expected_version` envelope
**Proposed tag:** [standard]
**Reasoning:** Two new files within `core/mutation/`, single module. Closes the `Mutation` enum verbatim per phase doc with every variant carrying `expected_version: i64` (ADR-0012) and `MutationId` for idempotency. Stages — but does not yet define — the `AuditEvent::MutationRejectedMissingExpectedVersion` variant that 4.7 introduces. Self-contained type definitions; the cross-cutting consequences land in 4.7 and 4.9. Crosses the line above trivial because the enum shape is the Phase 4 contract every later phase consumes.
**Confidence:** high

### 4.7: `MutationOutcome`, `MutationError`, `Diff`, `AuditEvent` integration
**Proposed tag:** [cross-cutting]
**Reasoning:** Introduces the `AuditEvent` enum mapping ALL §6.6 audit `kind` values — this is the shared vocabulary every phase from 5 onward (snapshot writer, reconciler, API, UI) MUST emit and consume. Architecture §6.6 explicitly mandates `core::audit::AuditEvent` with PascalCase variants whose `Display` impl produces wire `kind` strings. Even though all files live in `core`, this slice establishes a project-wide event convention referenced by ADR-0009 (audit log substrate) and by tracing event names (architecture §12.1). Adds a new top-level module to `lib.rs`. That convention-setting role is the textbook "cross-cutting" trigger in the rubric.
**Confidence:** high

### 4.8: Capability-gating algorithm
**Proposed tag:** [standard]
**Reasoning:** Single file within `core/mutation/`. Adds `referenced_caddy_modules()` and `kind()` impls on `Mutation` plus the gating pseudocode from the phase doc. Consumes `CapabilitySet`/`CaddyCapabilities` produced by Phase 3 — that is consumption of an existing surface, not introduction of a new shared one. References a single ADR (ADR-0013). Pure algorithm in one module; no I/O, no new trait, no audit event introduction (only emits `MutationError::CapabilityMissing` already defined in 4.7).
**Confidence:** high

### 4.9: `apply_mutation` per-variant pure implementation
**Proposed tag:** [standard]
**Reasoning:** Two new files within `core/mutation/`. Pure function with the signature pinned in the phase doc, operating over `DesiredState` + `CapabilitySet`. References three ADRs (0009, 0012, 0016) and trait-signatures §5 (`DiffEngine`), but only as type *consumers* — `MutationOutcome.kind` carries the AuditEvent introduced in 4.7; `apply_mutation` does not itself introduce a new trait or convention. Single-module algorithm. The 3+ ADR mention in the rubric is descriptive, not prescriptive — these are passive references rather than new contracts.
**Confidence:** medium
**If low confidence, why:** The 3-ADR threshold in the rubric flags this as potentially cross-cutting; I weight the "introduces something other slices must consume" question more heavily than ADR count, since the AuditEvent vocabulary 4.9 emits is *defined* in 4.7.

### 4.10: Property tests, schema generation, mutation README
**Proposed tag:** [cross-cutting]
**Reasoning:** Spans `core/crates/core/` Cargo.toml, source files, tests directory, a new `build.rs`, and `docs/schemas/mutations/*.json` plus a README outside the crate. Generates committed artefacts (one JSON schema per mutation variant) that `just check` validates for drift — a project-wide build/check convention other phases will need to respect when mutations evolve. Adds a `build.rs` (build-script dependency on schemars or equivalent) and modifies `Cargo.toml`. The schema-drift gate is a new CI invariant, which fits the rubric's "introduces a convention other slices must follow" trigger.
**Confidence:** medium
**If low confidence, why:** Could argue standard since all source code lives in `core` and the docs artefacts are generated outputs; the cross-cutting nature is the build/CI gate rather than runtime API surface.

---

## Summary
- 2 trivial
- 6 standard
- 2 cross-cutting
- 2 low-confidence (4.4 and 4.9 medium; 4.10 medium — three flagged for human review)

## Notes

- Phase 4 is overwhelmingly a pure-core typing phase. The architectural three-layer rule (ADR-0003) is never crossed in any slice — every file lives in `core/crates/core/`. That alone rules out the most common cross-cutting trigger ("crosses a layer boundary") for all 10 slices.
- The two genuine cross-cutting slices are 4.7 (introduces the `AuditEvent` vocabulary every later phase emits/consumes) and 4.10 (introduces the schema-drift CI gate and build-script convention).
- Slice 4.6 stages an audit variant that 4.7 actually introduces. If executed out of order or if 4.7's enum shape is wrong, 4.6's missing-envelope test cannot land. Flag for the executor: 4.6 → 4.7 ordering matters; 4.6 may need a follow-up touch-up after 4.7 lands.
- Slice 4.9 references three ADRs but only as a *consumer* of the contracts those ADRs established (concurrency anchor from ADR-0012, snapshot expectations from ADR-0009, attachment record from ADR-0016). Treating multi-ADR reference as automatically cross-cutting would over-tag here.
- Slices 4.1–4.5 build a strict file/type dependency chain (4.1 → 4.2/4.3 → 4.4; 4.5 feeds 4.6). Implementation order should follow numbering.

---

## User Decision
**Date:** 2026-05-03
**Decision:** accepted

### Modifications
None.

### Notes from user
None.
