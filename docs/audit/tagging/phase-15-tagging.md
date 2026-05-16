# Phase 15 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:** CLAUDE.md, docs/architecture/architecture.md, docs/architecture/trait-signatures.md, docs/planning/PRD.md, docs/adr/ (0001–0016, focus 0001/0002), docs/todo/phase-15-dual-pane-editor.md, docs/architecture/seams.md, docs/architecture/contract-roots.toml, docs/architecture/contracts.md
**Slices analysed:** 6

## Proposed Tags

### 15.1: Caddyfile renderer in `core`
**Proposed tag:** [standard]
**Reasoning:** Adds a single new module (`core/crates/core/src/caddyfile/renderer.rs`) with one pure `pub fn render` confined to the `core` crate; no I/O, no async, no new trait, and the algorithm only consumes the existing `DesiredState` type and the Phase 13 parser. It is not [trivial] only because it adds a new `pub` function module-registered in `caddyfile/mod.rs` and is consumed across a layer boundary by slice 15.4's render endpoint (the renderer becomes a contract surface other slices call), and its round-trip property is structurally coupled to the Phase 13 corpus. It stays inside one crate and one layer, so it is not [cross-cutting].
**Affected seams:** none
**Planned contract additions:** `trilithon_core::caddyfile::renderer::render`
**Confidence:** high
**If low confidence, why:** n/a

### 15.2: `POST /api/v1/desired-state/validate` endpoint
**Proposed tag:** [cross-cutting]
**Reasoning:** The slice spans two crates (`cli` HTTP handler plus an extension to `core::validation::ValidationError` adding a `pane` field) and crosses the core↔cli layer boundary, wiring `core::caddyfile`, `core::validation`, and the `Applier::validate` trait surface into a new HTTP endpoint. It emits the `http.request.received` / `http.request.completed` tracing events that other HTTP slices must follow as a convention, and the new `ValidationError` shape (with `pane`/`line`/`column`/`path`) is a wire contract consumed by every editor slice (15.3–15.6) and the language-model gateway's `ValidationErrorSet`. The endpoint is the integration point both panes call.
**Affected seams:** PROPOSED: desired-state-validate-endpoint (cli HTTP validation surface ↔ core validation/caddyfile/Applier::validate)
**Planned contract additions:** `trilithon_cli::http::desired_state_validate::post_validate`, `ValidateBody`, `ValidateFormat`, `ValidateResponse`, `ValidationError` (extended with `pane`), `ValidatePane`
**Confidence:** medium
**If low confidence, why:** Whether the `core::validation::ValidationError` extension is a shared-trait-level change or a localised struct edit depends on how widely Phase 4's type is already consumed.

### 15.3: Editor state machine in TypeScript
**Proposed tag:** [trivial]
**Reasoning:** A single pure-TypeScript module (`web/src/features/editor/state.ts`) plus its co-located test; the reducer is a side-effect-free function with no React, no I/O, no network, and no audit/tracing events. It does not touch the Rust workspace or any layer boundary and introduces no shared abstraction beyond its own feature folder. It is fully self-contained within `web/`.
**Affected seams:** none
**Planned contract additions:** none (frontend feature module, not in the Rust contract registry)
**Confidence:** high
**If low confidence, why:** n/a

### 15.4: `DualPaneEditor` shell layout
**Proposed tag:** [standard]
**Reasoning:** Self-contained within the `web/src/features/editor/` module group: it wires slice 15.3's reducer into React components (`DualPaneEditor`, `CaddyfilePane`, `JsonPane`, `highlighter.ts`) and consumes the 15.2 validation endpoint plus a small `POST /api/v1/desired-state/render-caddyfile` extension. The render-endpoint addition lives in the same `cli` file as 15.2 and reuses 15.1's renderer rather than introducing a new trait or convention, so it is a localised I/O addition, not a cross-cutting one. It touches one frontend module group and emits no tracing/audit events others depend on.
**Affected seams:** none (consumes the seam proposed by 15.2)
**Planned contract additions:** `trilithon_cli::http::desired_state_validate::post_render_caddyfile`, `RenderResponse`
**Confidence:** medium
**If low confidence, why:** The render-caddyfile endpoint nudges toward [cross-cutting] since it adds a second HTTP route in a different crate; treated as [standard] because it is a thin reuse of 15.1/15.2 rather than a new contract or convention.

### 15.5: Apply gating, diff preview, commit handler
**Proposed tag:** [standard]
**Reasoning:** Extends `DualPaneEditor.tsx` and adds `DiffPreviewModal.tsx` within the same editor feature module, reusing the existing Phase 11 `DiffPreview` component. The commit flow delegates to a `props.onCommit` callback supplied by the host rather than introducing a new mutation path, and the Phase 17 "Rebase" call-to-action is a deliberate no-op placeholder. No new crate, no layer crossing, no new trait — work is confined to `web/` and emits no audit/tracing events directly.
**Affected seams:** none
**Planned contract additions:** none (frontend feature module)
**Confidence:** high
**If low confidence, why:** n/a

### 15.6: Dual-pane Vitest test corpus
**Proposed tag:** [trivial]
**Reasoning:** Test-only work: extends `DualPaneEditor.test.tsx` and adds a `fixtures/` directory, introducing no public types and using the existing context-injected validation-adapter test scaffolding. It touches one module, crosses no layer, and emits nothing other phases depend on.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

## Summary
- 2 trivial / 3 standard / 1 cross-cutting / 0 low-confidence

## Notes

- Phase 15 introduces **no new Rust traits** and **no new trait methods** (stated explicitly in the TODO's "Trait surfaces consumed" section). It reuses `core::reconciler::Applier::validate` (trait-signatures.md §6) and the Phase 13 caddyfile parser/translator. This caps the cross-cutting count: only 15.2 genuinely crosses the core↔cli boundary and establishes a shared wire contract.
- The new HTTP routes (`POST /api/v1/desired-state/validate` in 15.2, `POST /api/v1/desired-state/render-caddyfile` in 15.4) are not in `contract-roots.toml` today; `[roots]` currently lists only Phase 7 apply-path symbols. If the `cli` HTTP surface is to be tracked as a contract, the endpoint handlers and their request/response types should be added to `contract-roots.toml` via `/phase-merge-review` — flagged here, not assumed.
- One **proposed seam** is surfaced for 15.2 (`desired-state-validate-endpoint`). `seams.md` currently enumerates only Phase 7 apply-path seams; per the seam lifecycle, `/tag-phase` writes proposed seams to `seams-proposed.md` staging and `/phase-merge-review` ratifies. The validation endpoint is a real boundary: the editor (Phase 15 frontend) consumes outputs produced by `core` validation/caddyfile logic through the `cli` layer.
- The validation endpoint is **read-only by cross-cutting invariant** (no audit row, no mutation, no `config_version` advance). It therefore emits tracing events (`http.request.*`, already in architecture §12.1) but no audit `kind` — so 15.2's cross-cutting status comes from layer-crossing and the shared `ValidationError` wire contract, not from an audit convention.
- 15.2's `ValidationError` extension (adding `pane`) touches a Phase 4 type. If that type is widely consumed, the change ripples; if it is local to the validation module, the edit is contained. This is the one medium-confidence judgement and the reason 15.2 is tagged [cross-cutting] conservatively.
- ADRs in scope: ADR-0001 (Caddy as the only supported reverse proxy) and ADR-0002 (Caddy JSON Admin API as source of truth / Caddyfile is one-way import). No slice references 3+ ADRs, consistent with the low cross-cutting count.

ANALYSED: phase-15 — 6 slices tagged (2 trivial, 3 standard, 1 cross-cutting, 0 low-confidence). Output: docs/audit/tagging/phase-15-tagging.md

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Notes from user
Auto-accepted.
