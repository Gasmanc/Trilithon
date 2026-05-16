# Phase 14 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:** CLAUDE.md, docs/architecture/architecture.md, docs/architecture/trait-signatures.md, docs/planning/PRD.md, docs/adr/ (ADR-0001 … ADR-0016, focus ADR-0002 and ADR-0013), docs/todo/phase-14-tls-and-upstream-health.md, docs/architecture/seams.md, docs/architecture/contract-roots.toml, docs/architecture/contracts.md
**Slices analysed:** 8

## Proposed Tags

### 14.1: Migration `0005_tls_and_health.sql` and storage extension
**Proposed tag:** [cross-cutting]
**Reasoning:** The slice extends the shared `core::storage::Storage` trait with four new methods (`upsert_tls_certificates`, `list_tls_certificates`, `upsert_upstream_health`, `list_upstream_health`), which forces an edit to `crates/core/src/storage.rs` AND its adapter implementation in `crates/adapters/src/storage_sqlite.rs` — a core↔adapters trait modification. It also adds a forward-only SQL migration that every downstream slice (14.2, 14.3, 14.5, 14.8) depends on, and the trait extension MUST land in `trait-signatures.md` in the same commit per the document's stability rule. Modifying a shared, object-safe trait that adapters are stored behind is the canonical cross-cutting trigger.
**Affected seams:** none active; PROPOSED: tls-inventory-storage (Storage TLS/upstream-health methods become the seam between the Phase 14 refresher adapters and the SQLite store)
**Planned contract additions:** `trilithon_core::storage::Storage::upsert_tls_certificates`, `::list_tls_certificates`, `::upsert_upstream_health`, `::list_upstream_health`, plus row types `trilithon_core::storage::TlsCertificate` and `trilithon_core::storage::UpstreamHealthRow` (trait surface already drafted in trait-signatures.md §1; contract-roots.toml currently lists only Phase 7 apply-path roots, so these are net-new contract roots)
**Confidence:** high
**If low confidence, why:** n/a

### 14.2: `TlsInventory` adapter with 5-minute Tokio interval
**Proposed tag:** [standard]
**Reasoning:** The work is self-contained in a single new file `crates/adapters/src/tls_inventory.rs` plus a one-line task registration in `crates/cli/src/services.rs`, which is wiring, not policy. It consumes the existing `CaddyClient::get_certificates` and the already-extended `Storage` trait — it adds no new trait and no new shared vocabulary, reusing the existing `caddy.capability-probe.completed` tracing event by design. It introduces I/O (a Caddy poll plus a periodic Tokio task) confined to the one adapter, which is exactly the standard envelope.
**Affected seams:** PROPOSED (from 14.1): tls-inventory-storage
**Planned contract additions:** none (`refresh_tls_inventory`, `run_inventory_loop`, `TlsInventoryReport` are adapter-internal; not contract roots unless a later phase consumes them)
**Confidence:** high
**If low confidence, why:** n/a

### 14.3: `UpstreamHealth` adapter with 30-second interval and Caddy long-poll
**Proposed tag:** [cross-cutting]
**Reasoning:** The slice introduces a brand-new tracing event `upstream.probe.completed` into the architecture §12.1 closed vocabulary, and §12.1 is explicit that the table is authoritative and must be amended in the same commit — this is a tracing convention other code follows. The implementation also spans `crates/adapters/src/upstream_health.rs` plus the long-poll subscriber registration in `crates/cli/src/services.rs`, consumes two traits (`CaddyClient`, `ProbeAdapter`) and the `Storage` trait, and defines a merge rule between two reachability sources that downstream slices (14.4, 14.5, 14.6) all depend on. Introducing a shared observability convention plus multi-crate reach clears the cross-cutting bar.
**Affected seams:** PROPOSED (from 14.1): tls-inventory-storage; PROPOSED: upstream-health-caddy-probe (the merge boundary between `CaddyClient::get_upstream_health`, `ProbeAdapter::tcp_reachable`, and the persisted health rows)
**Planned contract additions:** none new as contract roots; the new tracing event `upstream.probe.completed` and span fields (`route.id`, `correlation_id`, `latency_ms`) are vocabulary additions to architecture §12.1, not Rust contract symbols
**Confidence:** high
**If low confidence, why:** n/a

### 14.4: Route-level probe opt-out
**Proposed tag:** [standard]
**Reasoning:** The slice branches `run_health_loop` on an existing `Route::disable_trilithon_probes` field (the TODO entry conditions assume Phase 4/11 already expose it) and surfaces that flag in `crates/cli/src/http/routes.rs`. The behaviour change is localized to the upstream-health adapter introduced in 14.3 plus a field pass-through in an existing HTTP handler; it adds no trait, no migration, and no new shared convention. The touch of `core/src/route.rs` is a confirm-only step ("confirm the field exists"), keeping this within the one-or-two-tightly-related-crate standard envelope rather than a genuine cross-layer change.
**Affected seams:** PROPOSED (from 14.3): upstream-health-caddy-probe
**Planned contract additions:** none
**Confidence:** medium
**If low confidence, why:** If `disable_trilithon_probes` does not already exist on `Route` from a prior phase, this slice would add a core field plus a migration and become cross-cutting; the TODO states the field is expected to exist but flags "add migration if not."

### 14.5: HTTP endpoints `GET /api/v1/tls/certificates` and `/upstreams/health`
**Proposed tag:** [standard]
**Reasoning:** Two read-only Axum handlers added in `crates/cli/src/http/tls.rs` and `crates/cli/src/http/upstreams.rs` plus route registration in `http/mod.rs` — all within the `cli` crate. The handlers only call existing `Storage::list_*` methods and reuse the existing `http.request.received` / `http.request.completed` tracing events; they emit no audit rows and add no new vocabulary. This is single-crate, single-layer endpoint work over an already-established storage contract, the standard profile.
**Affected seams:** none
**Planned contract additions:** none (response DTOs `TlsCertificatesResponse`, `UpstreamHealthResponse` are HTTP-layer wire types, not workspace contract roots)
**Confidence:** high
**If low confidence, why:** n/a

### 14.6: Web UI per-route TLS and upstream health badges
**Proposed tag:** [standard]
**Reasoning:** Frontend-only work confined to the `web/src/features/routes/` module: extending `RouteCard.tsx`, adding badge tests, and extending `api.ts`/`types.ts` with the new fetchers and types. It adds two pure colour/state computation functions and consumes the 14.5 HTTP endpoints — no new shared abstraction, no cross-cutting convention, one feature module. This is a self-contained component slice.
**Affected seams:** none
**Planned contract additions:** none (the Rust contract registry does not track TypeScript types)
**Confidence:** high
**If low confidence, why:** n/a

### 14.7: Dashboard "TLS expiring soon" widget
**Proposed tag:** [trivial]
**Reasoning:** A single new presentational component `TlsExpiringWidget.tsx` and its test file in `web/src/features/dashboard/`, with one filter-sort-render function over data already fetched by 14.5. It is one module, no new trait, no I/O of its own (props-driven), no audit/tracing events, and no shared convention. It is the smallest possible unit and clears the trivial bar.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** n/a

### 14.8: "Issuing" vs "applied" state with ACME error surfacing
**Proposed tag:** [cross-cutting]
**Reasoning:** The slice spans three crates/layers at once: it extends the `tls_inventory` adapter merge logic to capture ACME failure detail, extends the `cli` HTTP `tls.rs` response shape with `last_error`, and adds a fourth badge state plus an error banner with a re-apply trigger in the `web` `RouteCard`. It also extends the `TlsCertificate` row shape with a new `last_error` field, which propagates through the storage row, the adapter, the HTTP DTO, and the TypeScript type in lockstep. Touching adapters + cli + web together with a coordinated data-shape change to mitigate hazard H17 places it firmly in cross-cutting.
**Affected seams:** PROPOSED (from 14.1): tls-inventory-storage
**Planned contract additions:** field addition `last_error: Option<String>` on `trilithon_core::storage::TlsCertificate` (extends the 14.1 row contract; note architecture §6.14 names the column `renewal_detail` — see Notes)
**Confidence:** medium
**If low confidence, why:** The TODO adds a `last_error` field while architecture §6.14 already specifies a `renewal_detail` column for the same purpose; the naming must be reconciled, which may shift the exact contract symbol.

## Summary
- 1 trivial / 4 standard / 3 cross-cutting / 2 low-confidence (14.4, 14.8 are medium)

## Notes

- **Contract registry is near-empty.** `contracts.md` reports `contract_count: 0` and `contract-roots.toml` lists only Phase 7 apply-path roots. The `Storage` / `CaddyClient` / `ProbeAdapter` trait surfaces are documented in `trait-signatures.md` but are not yet contract roots. Slice 14.1 should add the new `Storage` TLS/health methods to `contract-roots.toml` (a contract change reviewed by `/phase-merge-review`).

- **No matching seam exists.** `seams.md` currently holds only Phase 7 apply-path seams. Phase 14 needs two new seams — `tls-inventory-storage` (refresher adapters ↔ SQLite store) and `upstream-health-caddy-probe` (Caddy/Trilithon probe merge ↔ persisted health) — which `/tag-phase` cannot invent in `seams.md` directly; they go to `seams-proposed.md` for `/phase-merge-review` ratification.

- **Schema-shape discrepancies the implementer must reconcile (not tagging-affecting but flagged):**
  - The TODO's slice 14.1 DDL keys `tls_certificates` on `host` alone, while architecture §6.14 keys it on `(host, issuer)`. Architecture §6.14 and `trait-signatures.md` §1 are authoritative.
  - The TODO's `upstream_health` table uses `(route_id, upstream)` as PK with a mutable `state` column; architecture §6.15 specifies an append-only table keyed `(route_id, upstream_id, observed_at)`. `trait-signatures.md` §1 describes `upsert_upstream_health` as append-only with triple-key dedup — the TODO's "advance last_transition only on state flip" algorithm conflicts with this and must be raised against the TODO.
  - The TODO references migration file `0005_tls_and_health.sql`, but the migrations directory already contains files through `0012`; the actual new migration number will be higher (likely `0013`).
  - Slice 14.8 adds a `last_error` field while §6.14 already names the column `renewal_detail`; reconcile before implementing.

- **Vocabulary authority.** Slice 14.3's `upstream.probe.completed` is already pre-listed in architecture §12.1, but the §12.1 rule still requires the row land in the same commit that ships the emitting code. Slice 14.2 correctly reuses the existing `caddy.capability-probe.completed` event rather than inventing a new one — no vocabulary change there.

- **Cross-cutting invariants** (TODO §"Cross-cutting invariants") — read-only HTTP surfaces, UTC-storage/local-display, no silent overwrite of Caddy-owned fields, and uniform probe opt-out — apply to every slice and should be checked at review regardless of per-slice tag.

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Modifications (if any)
None.

### Notes from user
Auto-accepted.
