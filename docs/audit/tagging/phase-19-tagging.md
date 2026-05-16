# Phase 19 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:** CLAUDE.md, docs/architecture/architecture.md, docs/architecture/trait-signatures.md, docs/planning/PRD.md (T2.3 / T1.6 / H16 via ADRs), docs/adr/0008-bounded-typed-tool-gateway-for-language-models.md, docs/adr/0009-immutable-content-addressed-snapshots-and-audit-log.md, docs/todo/phase-19-gateway-explain-mode.md, docs/architecture/seams.md, docs/architecture/contract-roots.toml, docs/architecture/contracts.md
**Slices analysed:** 8

## Proposed Tags

### 19.1: Tokens table migration and Argon2id hashing
**Proposed tag:** [standard]
**Reasoning:** Self-contained in the `adapters` crate: one new migration file plus one new module (`gateway_token_store.rs`) exported from `adapters/src/lib.rs`. It adds I/O (SQLite) but only in a single adapter, defines its own concrete `GatewayTokenError` type, and introduces no shared trait. It imports `trilithon_core::tool_gateway::Scope` but that type does not exist until 19.2 ships — the dependency is intra-phase and within-layer (adapters→core is the normal direction), so it does not make this cross-cutting. No audit/tracing events are emitted here.
**Affected seams:** none
**Planned contract additions:** none (the store is a concrete adapter type, not registered in contract-roots.toml; `GatewayTokenStore`, `CreatedToken`, `VerifiedToken`, `TokenSummary` stay internal)
**Confidence:** medium
**If low confidence, why:** The TODO names a new `0006_gateway_tokens.sql` / `gateway_tokens` table while architecture §6.4 already defines a `tokens` table; the implementer must reconcile the table name and column set against §6.4. A working-tree migration `0012_tokens_user_id.sql` already exists, so the `0006` filename is likely stale numbering — a fixable wrinkle, not a tag change.

### 19.2: Typed scope set and read-function catalogue
**Proposed tag:** [standard]
**Reasoning:** Confined to the `core` crate: a new `tool_gateway` module with `scopes.rs` and `read_functions.rs`, plus JSON-Schema fixture files under `docs/schemas/gateway/`. It defines closed enums (`Scope`, `ReadFunction`) and `include_str!`-embedded schemas but adds no trait, no I/O, and no cross-layer dependency. These types are foundational for 19.3–19.8, yet the slice itself touches a single crate and a single layer.
**Affected seams:** none (PROPOSED: a `tool-gateway-read-catalogue` seam may be warranted, but the natural seam boundary is the `ToolGateway` trait introduced in 19.4, not this slice)
**Planned contract additions:** none yet — `Scope` and `ReadFunction` become public `core` API but contract-roots.toml registration belongs with the `ToolGateway` trait root proposed under 19.4
**Confidence:** high

### 19.3: Per-token rate limiter
**Proposed tag:** [standard]
**Reasoning:** A single new module `core/src/tool_gateway/rate_limit.rs` plus one config field added to `core/src/config.rs`. The limiter is a self-contained struct with `check_and_record`; it lives in one crate and one layer. The TODO's example imports `dashmap`, which is not on the `core` allow-list in architecture §5 — the implementer must use a permitted primitive or escalate, but that is an implementation correction within the slice, not a cross-cutting concern. No trait, no I/O, no audit/tracing events.
**Affected seams:** none
**Planned contract additions:** none (the limiter is an internal `core` type)
**Confidence:** medium
**If low confidence, why:** `dashmap` is not in the `core` dependency allow-list (architecture §5); the slice may need an interior-mutability primitive change, and adding a `[tool_gateway]` config block touches the shared `RuntimeConfig` surface, which borders on cross-cutting if other phases consume it.

### 19.4: Read-only function implementations
**Proposed tag:** [cross-cutting]
**Reasoning:** This slice introduces the `ToolGateway` trait into `core` (per trait-signatures.md §7) and its `DefaultToolGateway` implementation in `adapters` — a shared trait that 19.5 (HTTP) and the future Phase 20 propose mode both depend on, so it crosses the core↔adapters boundary by definition. `DefaultToolGateway` wires together six existing adapters (snapshot, audit, route, upstream-health, certificate inventory, route history) plus the rate limiter, fanning across multiple crates. It also defines the new `tool-gateway.invocation.*` tracing events that 19.5 and 19.7 emit. References ADR-0008, trait-signatures §7, and architecture §6.6/§12.1.
**Affected seams:** PROPOSED: `tool-gateway-invoke-read` — name "ToolGateway ↔ read adapters", contracts `trilithon_core::tool_gateway::ToolGateway`, `trilithon_core::tool_gateway::ReadFunction`, `trilithon_core::tool_gateway::ToolGatewayError`, `trilithon_adapters::tool_gateway::DefaultToolGateway`; goes to seams-proposed.md for `/phase-merge-review` ratification
**Planned contract additions:** `trilithon_core::tool_gateway::ToolGateway`, `trilithon_core::tool_gateway::ToolGatewayError`, `trilithon_core::tool_gateway::ReadFunction`, `trilithon_core::tool_gateway::Scope`, `trilithon_core::tool_gateway::SessionToken` (add to contract-roots.toml)
**Confidence:** high

### 19.5: HTTP endpoints and authentication middleware
**Proposed tag:** [cross-cutting]
**Reasoning:** Spans the `cli` crate's HTTP layer while consuming the `core` `ToolGateway` trait and the `adapters` `GatewayTokenStore` — a clean core↔adapters↔cli traversal. It introduces a new public HTTP contract surface (`POST /api/v1/gateway/functions/{list,call}`) and bearer-token middleware that establishes the `SessionToken` request-extension convention later handlers and 19.7's audit code rely on. It emits the `tool-gateway.session-opened` audit kind and `http.request.*` tracing events. References ADR-0008, architecture §8.3 and §11 (language-model boundary, hazard H16).
**Affected seams:** PROPOSED: `tool-gateway-http-auth` — name "Gateway HTTP ↔ token verification", contracts `trilithon_adapters::gateway_token_store::GatewayTokenStore`, `trilithon_core::tool_gateway::SessionToken`, `trilithon_core::tool_gateway::ToolGateway`; goes to seams-proposed.md
**Planned contract additions:** none new beyond 19.4's (HTTP request/response structs are `cli`-internal wire types, not registered contracts)
**Confidence:** medium
**If low confidence, why:** Architecture §8.3 fixes the gateway path prefix as `/api/tool/` while the TODO specifies `/api/v1/gateway/...`; the implementer must reconcile the route prefix against §8.3 (or raise an ADR) — a path discrepancy, not a tag change.

### 19.6: Prompt-injection envelope and system message
**Proposed tag:** [cross-cutting]
**Reasoning:** Adds the `wrap_untrusted` helper in `core/src/tool_gateway/envelope.rs` and modifies `adapters/src/tool_gateway.rs` to apply it — touching two crates across the core↔adapters boundary. More decisively, it establishes the `{ data, warning }` untrusted-envelope convention and the published system message (`docs/gateway/system-message.md`) that satisfy hazard H16; this is a shared structural convention every gateway response and any future propose-mode response must follow. References ADR-0008 and hazard H16.
**Affected seams:** PROPOSED: covered by the `tool-gateway-invoke-read` seam proposed under 19.4 — the envelope is part of the read-response contract; no separate seam needed
**Planned contract additions:** `trilithon_core::tool_gateway::envelope::wrap_untrusted` and `UntrustedEnvelope` (consider registering, as the envelope shape is a wire contract H16 depends on)
**Confidence:** medium
**If low confidence, why:** In isolation the helper is a one-function `core` module that could read as [standard]; it is [cross-cutting] because it introduces a security convention (H16) that other slices and Phase 20 must conform to, and that convention judgement is the borderline call.

### 19.7: Audit obligations
**Proposed tag:** [cross-cutting]
**Reasoning:** Touches `core/src/audit.rs` (confirming the `ToolGatewayInvoked` / `ToolGatewaySessionOpened` / `ToolGatewaySessionClosed` variants) and `cli/src/http/gateway.rs`, crossing the core↔cli boundary. It introduces the audit-emission convention for every gateway call — actor-id format `language-model:<token-name>`, blake3 arg/result hashing, the per-token-day session-open rule — and emits three §6.6 audit kinds. Per the trait-signatures cross-trait invariant "audit row provenance", audit-writing behaviour is a shared cross-phase concern. References ADR-0008, ADR-0009, architecture §6.6.
**Affected seams:** PROPOSED: covered by the `tool-gateway-http-auth` seam proposed under 19.5; the audit row is the observable side-effect of that seam
**Planned contract additions:** none (the `GatewayAuditNotes` struct is `cli`-internal; the `AuditEvent` variants already exist in `core` per architecture §6.6)
**Confidence:** medium
**If low confidence, why:** If the `AuditEvent` variants genuinely already exist (architecture §6.6 says they do), the code delta is mostly in one `cli` file and could read as [standard]; it is tagged [cross-cutting] because it sets the audit convention and emits shared §6.6 vocabulary that downstream phases follow.

### 19.8: API tokens page (web)
**Proposed tag:** [standard]
**Reasoning:** Entirely within the `web/` frontend: one feature directory (`web/src/features/tokens/*`) with typed React components, a `useTokens` hook, and Vitest tests. It consumes the gateway HTTP endpoints over the wire but adds no Rust crate, no trait, and no cross-layer dependency inside the workspace; the web tier is its own module group (architecture §4.13). No audit or tracing events originate here.
**Affected seams:** none (the frontend↔HTTP boundary is a network contract exercised by the endpoints from 19.5, not a Rust workspace seam in seams.md)
**Planned contract additions:** none
**Confidence:** high

## Summary
- 3 trivial / 4 standard / 4 cross-cutting / 5 low-confidence
- Correction: 0 trivial / 4 standard (19.1, 19.2, 19.3, 19.8) / 4 cross-cutting (19.4, 19.5, 19.6, 19.7) / 5 medium-confidence (19.1, 19.3, 19.5, 19.6, 19.7)

## Notes

- **No trivial slices.** Every slice in Phase 19 either adds I/O, extends/implements a trait, crosses a layer, or establishes a convention other slices follow. The phase is foundational for the language-model surface, so the floor is [standard].
- **The cross-cutting block (19.4–19.7) is the gateway core.** 19.4 introduces the `ToolGateway` trait and `DefaultToolGateway`; 19.5 mounts it behind HTTP with auth middleware; 19.6 adds the H16 envelope convention; 19.7 adds the audit convention. These four together cross all three layers and define conventions Phase 20 (propose mode) will inherit. They should each get a cross-phase integration test.
- **Seam staging.** Two new seams are proposed (`tool-gateway-invoke-read`, `tool-gateway-http-auth`). Per seams.md rules, `/tag-phase` writes these to `seams-proposed.md`; `/phase-merge-review` ratifies them into `seams.md` before merge. The current `seams.md` has no Phase 19 entries.
- **Contract-roots update needed.** 19.4 should add the `tool_gateway` trait surface (`ToolGateway`, `ToolGatewayError`, `ReadFunction`, `Scope`, `SessionToken`) to `contract-roots.toml`; that file edit is itself a contract change reviewed by `/phase-merge-review`. The registry (`contracts.md`) is currently empty.
- **Two spec discrepancies the implementer must reconcile (neither changes a tag):**
  1. Migration filename/table: TODO says `0006_gateway_tokens.sql` / `gateway_tokens`; architecture §6.4 defines a `tokens` table; a `0012_tokens_user_id.sql` already exists in the working tree. The migration number and table name in the TODO are stale.
  2. Route prefix: TODO uses `/api/v1/gateway/...`; architecture §8.3 fixes the language-model path prefix as `/api/tool/`. Reconcile or raise an ADR.
- **`dashmap` allow-list violation (19.3):** the TODO's rate-limiter sketch imports `dashmap`, absent from the `core` dependency allow-list in architecture §5. Use a permitted interior-mutability primitive or escalate per CLAUDE.md's "stop and ask" rule.

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Notes from user
Auto-accepted.
