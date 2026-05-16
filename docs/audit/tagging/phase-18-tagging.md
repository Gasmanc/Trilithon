# Phase 18 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:** CLAUDE.md, architecture.md, trait-signatures.md, PRD.md (via §T2.2 references), ADRs 0001/0013/0014/0016 (full), docs/todo/phase-18-policy-presets.md, seams.md, contract-roots.toml, contracts.md
**Slices analysed:** 12

## Proposed Tags

### 18.1: Policy core types and registry scaffolding
**Proposed tag:** [cross-cutting]
**Reasoning:** Lands the entire `core::policy` public type surface (`PolicyBody`, `PolicyDefinition`, `LossyWarning`, `PolicyAttachError`, `SlotName`, etc.) plus the `PRESET_REGISTRY` constant — a new module that slices 18.2–18.12 and the `PresetRegistry` trait (trait-signatures.md §4) all build on. It introduces the foundational shared vocabulary that other slices follow and adds `pub mod policy;` to `core/src/lib.rs`. Although it touches only one crate with no I/O, it is the load-bearing contract every downstream slice depends on, and references ADR-0016/0013/0014 plus PRD T2.2.
**Affected seams:** PROPOSED: policy-preset-registry (`PresetRegistry` trait surface ↔ static registry, introduced this phase)
**Planned contract additions:** `trilithon_core::policy::PolicyBody`, `PolicyDefinition`, `LossyWarning`, `PolicyAttachError`, `SlotName`, `PRESET_REGISTRY` (the `PresetRegistry` trait is already a documented contract surface)
**Confidence:** high
**If low confidence, why:** —

### 18.2: Persistence migration and seeding
**Proposed tag:** [cross-cutting]
**Reasoning:** Spans `core` (new `AuditEvent::PolicyRegistryMismatch` variant), `adapters` (new `policy_store.rs` module, migration `0018`), and `cli` (`startup.rs` seeding call) — three crates and two layer boundaries. It introduces a schema migration that slices 18.4–18.12 depend on, emits the `policy.registry-mismatch` audit kind (a §6.6 vocabulary entry other code follows), and the `storage.migrations.applied` tracing event. Startup-abort semantics make it a convention other slices rely on.
**Affected seams:** PROPOSED: policy-store-persistence (`PolicyStore` seed/get ↔ `policy_presets` / `route_policy_attachments` tables)
**Planned contract additions:** `trilithon_adapters::policy_store::PolicyStore`, `PolicyStoreError`, `SeedReport`; `trilithon_core::audit::AuditEvent::PolicyRegistryMismatch`
**Confidence:** high
**If low confidence, why:** —

### 18.3: Capability degradation table
**Proposed tag:** [standard]
**Reasoning:** Confined to `core::policy` (three new sibling modules: `capability.rs`, `render.rs`, `validate.rs`) — one crate, one layer, no I/O. It consumes the existing `CapabilitySet` and `Route` types rather than modifying them, adds no new trait, and emits no audit or tracing events others depend on. It establishes the `render`/`validate` pure functions but those are within the policy module landed in 18.1.
**Affected seams:** none
**Planned contract additions:** `trilithon_core::policy::render::render`, `RenderResult`, `CaddyJsonFragment`; `trilithon_core::policy::validate::validate`, `PolicyValidationError`; `trilithon_core::policy::capability::DEGRADATION_TABLE`
**Confidence:** medium
**If low confidence, why:** `render`/`validate` may be referenced cross-phase by the apply path, which could elevate it to cross-cutting if those become registry contracts.

### 18.4: Preset `public-website@1`
**Proposed tag:** [standard]
**Reasoning:** Authors one static `PolicyDefinition` value in a new `core` preset module and registers it; the accompanying integration test lives in `adapters/tests`. It implements no trait, adds no I/O of its own (the test exercises existing attach plumbing), and touches one logical feature. The two-crate spread is only test placement, which the rubric explicitly permits for tightly related crates.
**Affected seams:** none
**Planned contract additions:** none (preset is a value behind `PRESET_REGISTRY`)
**Confidence:** high
**If low confidence, why:** —

### 18.5: Preset `public-application@1`
**Proposed tag:** [standard]
**Reasoning:** Same shape as 18.4 — one static preset definition plus registration, with an integration test in `adapters/tests`. No new trait, no shared-convention change, no migration. Self-contained authoring slice.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** —

### 18.6: Preset `public-admin@1`
**Proposed tag:** [standard]
**Reasoning:** Authors one static preset with basic-auth required; the integration test attaches with credentials from the Phase 10 secrets vault. It consumes the existing `SecretsVault` boundary rather than modifying it, implements no trait, and adds no shared convention. Self-contained authoring slice within one logical feature.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** —

### 18.7: Preset `internal-application@1`
**Proposed tag:** [standard]
**Reasoning:** One static preset definition plus registration with an `adapters/tests` integration test. The IP-allowlist-required behaviour is enforced by validation landed in 18.11, not here. No trait, no migration, no shared convention. Self-contained.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** —

### 18.8: Preset `internal-admin@1`
**Proposed tag:** [standard]
**Reasoning:** Same shape as the other preset-authoring slices — one static `PolicyDefinition`, registration, and an integration test. No trait, no migration, no shared-convention change. Self-contained.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** —

### 18.9: Preset `api@1`
**Proposed tag:** [cross-cutting]
**Reasoning:** Beyond authoring the preset, this slice changes the shared `PolicyBody::rate_limit` field type from `Option<RateLimitSlot>` to `Option<Vec<RateLimitSlot>>` and migrates every previously-landed preset (18.4–18.8) to the new shape. Modifying a shared core type that earlier slices already consumed is a cross-cutting change — it touches the `core::policy` contract surface introduced in 18.1 and forces a fan-out edit.
**Affected seams:** none (no seam registry entry, but `PolicyBody` is a contract symbol from 18.1)
**Planned contract additions:** none new — modifies the existing `PolicyBody` contract (`rate_limit` field type change)
**Confidence:** medium
**If low confidence, why:** If the `Vec<RateLimitSlot>` representation were adopted in 18.1 upfront, this would collapse to [standard]; as written the type migration is what makes it cross-cutting.

### 18.10: Preset `media-upload@1`
**Proposed tag:** [cross-cutting]
**Reasoning:** Authors the preset but also extends `core::policy::render.rs` with a verbatim `reverse_proxy` stanza emitter and adds `require_authentication_or_error` to `validate.rs`, both shared modules other presets and the mutation pipeline (18.11) consume. It depends on a queryable `Route` authentication property and introduces per-attachment body-size bounds the mutation pipeline enforces. The render/validate extensions are shared-convention changes, not isolated authoring.
**Affected seams:** none
**Planned contract additions:** `trilithon_core::policy::validate::require_authentication_or_error`; extends `trilithon_core::policy::render::render`
**Confidence:** medium
**If low confidence, why:** The render/validate touch is modest; if those functions are not registry contracts it leans toward [standard].

### 18.11: Mutation pipeline (attach, detach, upgrade)
**Proposed tag:** [cross-cutting]
**Reasoning:** Adds three variants to the shared `core::mutation::TypedMutation` enum (a closed enumeration every mutation consumer matches on), extends `validate.rs` with `validate_attachment`, adds HTTP handlers in `cli/src/http/policy.rs`, and mounts new routes — spanning `core` and `cli` across the layer boundary. It emits `policy-preset.attached/detached/upgraded` audit kinds and the `http.request.*` tracing events, and depends on all seven preset slices plus the 18.2 migration. References ADR-0016 and the Phase 4 mutation pipeline.
**Affected seams:** PROPOSED: policy-mutation-pipeline (`TypedMutation` policy variants ↔ validation ↔ snapshot writer ↔ audit log)
**Planned contract additions:** `trilithon_core::mutation::AttachPolicy`, `DetachPolicy`, `UpgradeAttachedPolicy` (variants of `TypedMutation`); `trilithon_core::policy::validate::validate_attachment`
**Confidence:** high
**If low confidence, why:** —

### 18.12: Web UI (PolicyTab, PresetPicker, PresetUpgradePrompt, CapabilityNotice)
**Proposed tag:** [standard]
**Reasoning:** Entirely within the `web/` frontend — new feature and component files plus hooks. It consumes the `/api/v1/policy` endpoints landed in 18.11 over HTTP but introduces no Rust trait, no migration, no audit/tracing convention. It is a self-contained module group (the Route management / Policy frontend area) confined to one tier; the Rust↔TS split is an HTTP boundary, not a shared in-process type boundary.
**Affected seams:** none
**Planned contract additions:** none (TypeScript types are frontend-local; not Rust contract symbols)
**Confidence:** medium
**If low confidence, why:** Frontend slices that bind a new API surface sometimes warrant cross-cutting if the wire contract is novel; here the wire contract is owned by 18.11.

## Summary
- 0 trivial / 7 standard / 5 cross-cutting / 0 low-confidence
- Standard (7): 18.3, 18.4, 18.5, 18.6, 18.7, 18.8, 18.12
- Cross-cutting (5): 18.1, 18.2, 18.9, 18.10, 18.11
- Medium-confidence (4): 18.3, 18.9, 18.10, 18.12 — none low-confidence

## Notes

- No slice qualifies as [trivial]: every slice either lands shared types, modifies a shared type/enum, crosses a layer boundary, or extends a shared render/validate module.
- The five cross-cutting slices form the phase's spine: 18.1 (shared type surface), 18.2 (migration + audit kind + cli wiring), 18.9 (shared-type migration), 18.10 (shared render/validate extension), 18.11 (shared `TypedMutation` enum + layer-crossing HTTP).
- The seam registry (seams.md) currently contains only Phase 7 apply-path seams. Phase 18 introduces three candidate seams that `/tag-phase` must stage in `seams-proposed.md` for `/phase-merge-review` ratification: `policy-preset-registry`, `policy-store-persistence`, `policy-mutation-pipeline`.
- contract-roots.toml currently roots only Phase 7 `reconciler` symbols; the registry (contracts.md) is empty. Phase 18 adds a substantial `trilithon_core::policy::*` surface plus `trilithon_adapters::policy_store::*`. Whoever lands 18.1/18.2/18.11 should propose contract-root additions in the same commit per the contract-roots.toml curation rule.
- The `PresetRegistry` trait (trait-signatures.md §4) is already documented as a stable trait surface; slice 18.1 lands its supporting types. Implementers must keep 18.1's `PolicyDefinition` consistent with the trait's `PresetDefinition`/`PresetSummary` naming, or open a fix-up commit against trait-signatures.md per its authority rule.

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Notes from user
Auto-accepted.
