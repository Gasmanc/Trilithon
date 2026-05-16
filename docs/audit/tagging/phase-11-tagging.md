# Phase 11 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:**
- CLAUDE.md — 209 lines
- docs/architecture/architecture.md — 1124 lines
- docs/architecture/trait-signatures.md — 735 lines
- docs/planning/PRD.md — 953 lines
- docs/adr/ — 16 ADRs present; read in full: ADR-0004 (frontend stack), ADR-0011 (loopback-only), ADR-0014 (secrets vault). Others scanned by title.
- docs/todo/phase-11-web-ui-shell.md — 851 lines (9 slices)
- docs/architecture/seams.md — 138 lines (5 seams, all Rust apply-path; phase 7)
- docs/architecture/contract-roots.toml — 36 lines (Phase 7 Rust apply-path roots only)
- docs/architecture/contracts.md — 18 lines (empty registry — 0 contracts)
**Slices analysed:** 9

---

## Preamble — phase-wide framing

Phase 11 is a **frontend-only phase**. Every slice's primary files live under
`web/` (TypeScript + React + Vite). The Rust three-layer rubric (core ↔
adapters ↔ cli/entry) does not apply within this phase: there is no Rust crate
boundary, no Rust trait, and no Rust layer to cross. The `web/` package is a
single compilation unit.

Consequences for tagging:

- The contract registry (`contracts.md`) is empty and the contract roots
  (`contract-roots.toml`) enumerate only Phase 7 Rust apply-path symbols. No
  Phase 11 slice can add a *registry* contract — the registry tracks Rust
  symbols only. "Planned contract additions" is therefore "none" for every
  slice.
- The seam registry lists only Rust apply-path seams (Phase 7). The Phase 11 ↔
  Phase 9 boundary (the OpenAPI/HTTP API the frontend consumes) is a real
  cross-phase seam but is **not currently registered**. It is flagged as a
  PROPOSED seam on slice 11.2, where the typed client is generated from the
  Phase 9 OpenAPI document.
- "Audit/tracing events" in the rubric: every slice's "Audit kinds emitted"
  section says the events come *from the daemon*, not from the frontend. The
  frontend emits no audit rows and no tracing events. So no slice introduces a
  cross-cutting audit/tracing convention in the Rust sense.

The rubric's "files span multiple crates" / "crosses a layer boundary" tests
are inert here. The meaningful cross-cutting axis within a frontend phase is
**shared frontend infrastructure that later slices structurally depend on** —
the router/layout tree, the generated API client, and the shared type/hook
surface. Slices 11.1–11.3 build that foundation; 11.4–11.9 are leaf features
that consume it. I tag the foundation slices by how broadly downstream slices
depend on them and how many modules they establish.

---

## Proposed Tags

### 11.1: Replace Vite skeleton with `App` shell, router, layouts, error boundary
**Proposed tag:** [cross-cutting]
**Reasoning:** This slice establishes the application-wide structural surface
that every other slice in the phase (and downstream phases 12–13's UI work)
builds on: the `react-router-dom` route table and `ROUTE_PATHS` constant, two
layouts, the `RequireAuth` guard, and the global `ErrorBoundary`. It introduces
the routing/layout convention (`<RequireAuth><AuthenticatedLayout>` wrapping,
`ROUTE_PATHS` as the single path source) that slices 11.4–11.9 must conform to,
and adds the `react-router-dom` dependency to `web/package.json`. It also
"lands the rule" banning CSS-in-JS that later slices inherit. That is a
phase-wide convention other slices structurally depend on — the cross-cutting
trigger.
**Affected seams:** none (no Rust seam; the frontend↔daemon boundary is exercised starting at 11.2)
**Planned contract additions:** none (frontend; registry is Rust-only)
**Confidence:** high

---

### 11.2: OpenAPI-generated typed API client
**Proposed tag:** [cross-cutting]
**Reasoning:** This slice creates the typed boundary between Phase 11's
frontend and Phase 9's HTTP API: a generator script, `openapi-typescript` /
`openapi-fetch` devDependencies, and the generated `schema.ts` + runtime
`apiClient`. Every data-fetching slice (11.3–11.9) depends on this client, and
CI gains a `pnpm generate:api && git diff --exit-code` gate that asserts the
committed schema stays in sync with the daemon's published OpenAPI document —
a structural cross-phase dependency on Phase 9's API surface. This is the
frontend equivalent of an FFI/contract boundary: it is the seam at which one
phase's output (Phase 9's OpenAPI doc) becomes this phase's typed input.
**Affected seams:** PROPOSED: `web-openapi-client` — "Web UI ↔ daemon HTTP API (OpenAPI schema sync)"; contracts are the Phase 9 `/api/v1/openapi.json` document and the generated `web/src/lib/api/generated/schema.ts`. Not in `seams.md` today (registry holds Rust seams only); routed to `seams-proposed.md` per the seam lifecycle.
**Planned contract additions:** none (registry is Rust-only; the OpenAPI schema is the de-facto contract but is not a registry symbol)
**Confidence:** medium
**If low confidence, why:** The seam registry currently models only Rust symbols; whether `/phase-merge-review` will accept a TypeScript/OpenAPI seam into `seams.md` is a process question, so the PROPOSED seam may instead be tracked outside the registry.

---

### 11.3: Mutation envelope and shared API types
**Proposed tag:** [cross-cutting]
**Reasoning:** This slice hand-writes the shared type vocabulary (`Route`,
`Upstream`, branded `RouteId`/`SnapshotId`/`JsonPointer`, `MutationEnvelope<T>`,
the `Mutation` discriminated union) and the `useAuthSession` context hook plus
`submitMutation` helper. `useAuthSession` is consumed by `RequireAuth` (11.1)
and the auth pages (11.4); the `Mutation` types and `submitMutation` are
consumed by 11.7, 11.8; the `DesiredState`/`Route` types by 11.6, 11.9. It also
fixes the `expected_version: number` envelope invariant (T2.10 optimistic
concurrency) that every mutating slice must honour. Five-plus downstream slices
structurally depend on this module, and it establishes the
context-provider/hook convention for the phase — cross-cutting by the
"shared surface other slices depend on" criterion.
**Affected seams:** none directly (consumes the 11.2 PROPOSED `web-openapi-client` seam)
**Planned contract additions:** none (frontend; registry is Rust-only)
**Confidence:** high

---

### 11.4: Login page, bootstrap step-up, change-password page
**Proposed tag:** [standard]
**Reasoning:** Three self-contained page components under
`web/src/features/auth/` plus their tests. The slice consumes the
`login`/`changePassword`/`useAuthSession` surface from 11.3 and the layouts
from 11.1; it introduces no new shared abstraction other slices import. It
mirrors Phase 9 slice 9.5's server-side rules client-side (T1.14, hazard H13)
but emits no audit events itself — `auth.login-succeeded`/`auth.login-failed`
are daemon-side. Files in one feature folder, one logical area, no new
phase-wide convention.
**Affected seams:** none
**Planned contract additions:** none (frontend; registry is Rust-only)
**Confidence:** high

---

### 11.5: Dashboard with capability summary, drift banner, apply-in-flight banner
**Proposed tag:** [standard]
**Reasoning:** Self-contained feature: `DashboardPage` plus three presentational
components (`CapabilityCard`, `DriftBanner`, `ApplyInFlightBanner`) under
`web/src/features/dashboard/`. It issues three parallel GETs through the
existing 11.2/11.3 client (`/api/v1/capabilities`, `/api/v1/drift/current`,
`/api/v1/health`) but defines no shared surface. The `apply_in_flight` and drift
endpoints are resolved Phase 9 dependencies (see TODO open questions). No new
convention, no cross-module impact.
**Affected seams:** none
**Planned contract additions:** none (frontend; registry is Rust-only)
**Confidence:** high

---

### 11.6: Routes index with pagination
**Proposed tag:** [standard]
**Reasoning:** A single feature component `RoutesIndex` (with a `RouteRow`
subcomponent) under `web/src/features/routes/`, plus its test. It consumes the
`Route` type and the API client; pagination is client-side and local. The TODO
open questions confirm `GET /api/v1/routes` is a resolved Phase 9 dependency, so
no cross-phase ambiguity. One feature folder, no shared surface introduced.
**Affected seams:** none
**Planned contract additions:** none (frontend; registry is Rust-only)
**Confidence:** high

---

### 11.7: Route detail, history strip, delete with confirmation
**Proposed tag:** [standard]
**Reasoning:** `RouteDetail`, `DeleteRouteButton`, and `HistoryStrip` under
`web/src/features/routes/`. It consumes `submitMutation` and the `Mutation`
types from 11.3 and submits a `DeleteRoute` mutation with `expected_version`;
the `mutation.applied`/`mutation.conflicted` audit rows are daemon-side, not
emitted here. The `SnapshotSummary` type is local to the slice. Self-contained
feature work in one folder; extends consumption of the 11.3 surface without
modifying it.
**Affected seams:** none
**Planned contract additions:** none (frontend; registry is Rust-only)
**Confidence:** high

---

### 11.8: Route create and update forms with field-level validation
**Proposed tag:** [standard]
**Reasoning:** `CreateRouteForm`/`UpdateRouteForm` under
`web/src/features/routes/`, plus a small `web/src/lib/validation/` module
(`hostname.ts`, `port.ts`, `regex.ts`). It also adds an ESLint rule banning
`styled-components`/`@emotion/*`. The validation helpers are new but narrow,
local pure functions consumed only within this slice (no third use elsewhere in
the phase), so they do not rise to a shared abstraction. The ESLint rule is a
lint-config addition, not a phase-wide convention other slices must restructure
around (11.1 already established the no-CSS-in-JS posture; this codifies it). It
consumes `submitMutation` and `DiffPreview` (11.9) but introduces no shared
surface. Two tightly-related feature areas (forms + validation) within one
package — squarely standard.
**Affected seams:** none
**Planned contract additions:** none (frontend; registry is Rust-only)
**Confidence:** medium
**If low confidence, why:** The `web/src/lib/validation/` module sits in shared `lib/` rather than a feature folder; if a later phase reuses these validators broadly it would retroactively look like a shared surface, but within Phase 11 it is single-consumer, keeping it standard.

---

### 11.9: `DiffPreview` component with secret redaction enforcement
**Proposed tag:** [standard]
**Reasoning:** `DiffPreview` plus a `redact.ts` client-side redactor under
`web/src/components/diff/`. It is a single presentational component with one
supporting pure module; only slice 11.8 consumes it within the phase. It
enforces the H10 / ADR-0014 "UI never renders plaintext secrets" invariant, but
that enforcement is *local* to this component — it mirrors Phase 6 slice 6.3's
redactor rather than defining a new project-wide convention. No new shared
surface, no audit/tracing emission, one component folder. The secret-redaction
rule it upholds is load-bearing but already established by ADR-0014 and Phase 6;
this slice consumes the rule, it does not author it.
**Affected seams:** none
**Planned contract additions:** none (frontend; registry is Rust-only)
**Confidence:** medium
**If low confidence, why:** The secret-redaction guarantee is security-critical and tied to hazard H10; a reviewer could argue it deserves cross-cutting attention, but structurally the component is single-consumer and authors no convention, so standard is correct under the rubric.

---

## Summary
- 6 trivial → 0
- standard → 6 (11.4, 11.5, 11.6, 11.7, 11.8, 11.9)
- cross-cutting → 3 (11.1, 11.2, 11.3)
- low-confidence → 3 (11.2, 11.8, 11.9)

(0 trivial, 6 standard, 3 cross-cutting, 3 low-confidence)

## Notes

- **No trivial slices.** Every Phase 11 slice ships at least one full React
  component with tests and live data wiring. None is a pure enum addition or a
  single type-only change. The smallest (11.6, 4h) is still a paginated
  data-fetching feature component.
- **Why the foundation trio is cross-cutting.** 11.1, 11.2, and 11.3 are each
  consumed by 5+ downstream slices and each establishes a phase-wide convention
  (routing/layout shape; the generated typed client + CI sync gate; the shared
  type vocabulary and `expected_version` envelope invariant). If any of the
  three regresses, the whole phase breaks — this is the structural-dependency
  signal the cross-cutting tag exists to flag, even though no Rust layer is
  crossed.
- **PROPOSED seam.** Slice 11.2 should register a `web-openapi-client` seam
  capturing the Web UI ↔ Phase 9 daemon HTTP API boundary. The current
  `seams.md` models only Rust apply-path seams; `/phase-merge-review` should
  decide whether the seam registry is extended to cover the OpenAPI contract or
  whether that boundary is tracked by the existing `pnpm generate:api` CI diff
  gate alone. Flagged here so the decision is explicit rather than implicit.
- **Contracts.** No slice adds a registry contract: `contracts.md` is empty and
  `contract-roots.toml` enumerates Rust symbols only. The frontend's de-facto
  contract is the generated OpenAPI `schema.ts`, governed by the CI sync check,
  not by `cargo xtask registry-extract`.
- **Audit/tracing.** Every slice's TODO explicitly states audit kinds come
  "from the daemon" and tracing events are emitted daemon-side off the frontend's
  `X-Correlation-Id` header. No Phase 11 slice introduces or modifies an audit
  kind (§6.6) or tracing event (§12.1); none touches the Rust vocabulary tables.
- **Phase boundary correctness check.** All "Depends on" entries in the slice
  summary point within Phase 11 (11.1→11.9) or to completed Phases 9/10; the
  three resolved open questions confirm the Phase 9 endpoints (`apply_in_flight`,
  `GET /api/v1/routes`, `POST /api/v1/desired-state/validate` deferred to
  Phase 15) are settled, so no slice carries a hidden cross-phase contract risk
  beyond the 11.2 OpenAPI seam already flagged.

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Modifications (if any)
None.

### Notes from user
None.
