# Phase 20 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:** CLAUDE.md; docs/architecture/architecture.md; docs/architecture/trait-signatures.md; docs/planning/PRD.md (T2.4, H8, H16); docs/adr/ (0007, 0008, 0012 in depth; 0001–0016 indexed); docs/todo/phase-20-gateway-propose-mode.md; docs/architecture/seams.md; docs/architecture/contract-roots.toml; docs/architecture/contracts.md
**Slices analysed:** 7

## Proposed Tags

### 20.1: Proposals migration and store
**Proposed tag:** [standard]
**Reasoning:** Adds one migration and one adapter module (`proposal_store.rs`) plus three small enums in `core::proposals` — two crates but tightly related and self-contained. It introduces a new persistent row shape and a new adapter write boundary, but no trait, no cross-layer wiring, and emits no audit/tracing events. Note a discrepancy the implementer must resolve before coding: the plan names the file `0007_proposals.sql` with columns `proposal_id`/`basis_config_version`/`expires_at_unix_seconds`, while architecture §6.8 already defines a `proposals` table (columns `id`, `source IN ('docker','llm','import')`, `expires_at`, `resulting_mutation`, `wildcard_callout`) — the plan and the architecture data model are not aligned, so the migration number and column names need reconciliation against §6.8 (and the workspace already has a `0012_*` migration, so `0007` is almost certainly taken).
**Affected seams:** none
**Planned contract additions:** `trilithon_core::proposals::ProposalId`, `ProposalSource`, `ProposalStatus`; `trilithon_adapters::proposal_store::ProposalStore`, `ProposalRecord`, `ProposalStoreError` (currently absent from contracts.md and contract-roots.toml)
**Confidence:** medium
**If low confidence, why:** Plan-vs-architecture §6.8 schema divergence (table name reuse, column names, migration number) means the slice may be reclassified cross-cutting if it forces a §6.8 amendment.

### 20.2: Propose scopes and propose-function catalogue
**Proposed tag:** [standard]
**Reasoning:** Touches only `core`: extends the `Scope` enum and adds a new `propose_functions` module plus JSON schema files. It grows a shared closed set (eight to eleven scopes) that gateway tokens issued in Phase 19 already serialise, but it adds no trait, no I/O, no audit/tracing convention, and no cross-layer dependency. The closed-set extension is additive and compile-time asserted, keeping blast radius inside one crate.
**Affected seams:** none
**Planned contract additions:** `trilithon_core::tool_gateway::scopes::Scope` (extended variants `ProposeRoutes`, `ProposePolicies`, `ProposeUpstreams`); `trilithon_core::tool_gateway::propose_functions::ProposeFunction`
**Confidence:** high

### 20.3: Proposal validation and creation
**Proposed tag:** [cross-cutting]
**Reasoning:** Implements `ToolGateway::invoke_propose` on the shared trait, wiring `core` validation, the `core::proposals` types, the Phase 19 rate limiter, and the `ProposalStore` adapter — it crosses the core↔adapters boundary on a shared trait. It also introduces the fifth `ToolGatewayError::ValidationFailed` variant, which the plan's own Open Questions flag as requiring a same-commit update to trait-signatures.md §7 (the closed-set declaration is shared by Phase 19 and downstream phases). It emits new-to-this-phase audit kinds (`mutation.proposed`, `tool-gateway.tool-invoked`) and tracing events (`proposal.received`, `tool-gateway.invocation.*`) that other slices and phases follow, and it references ADR-0007, ADR-0008, PRD T2.4, and hazard H16.
**Affected seams:** PROPOSED: tool-gateway-propose-pipeline (ToolGateway::invoke_propose ↔ core validation ↔ ProposalStore)
**Planned contract additions:** `trilithon_core::tool_gateway::ToolGatewayError::ValidationFailed`; `trilithon_core::proposals::ProposalCreated`, `ValidationReport`; `trilithon_adapters::tool_gateway::DefaultToolGateway::invoke_propose`
**Confidence:** high

### 20.4: Proposals HTTP endpoints (list, approve, reject)
**Proposed tag:** [cross-cutting]
**Reasoning:** Spans `cli` HTTP handlers, the `core` apply pipeline, the `ProposalStore` adapter, and the Phase 17 conflict path — three layers. It introduces a new `RequireUserSession` auth extractor enforcing the security-critical rule that a gateway token MUST NOT approve (PRD T2.4, ADR-0008, §11), a convention other endpoints will reuse. It emits multiple audit kinds (`proposal.approved/rejected/expired`, `mutation.applied/conflicted`) and tracing events, and routes stale-basis approvals through the Phase 17 rebase contract (ADR-0012, hazard H8).
**Affected seams:** PROPOSED: proposal-approval-apply (HTTP approve handler ↔ standard apply pipeline ↔ Phase 17 conflict/rebase path)
**Planned contract additions:** `trilithon_cli::http::proposals` handlers (`list_proposals`, `approve_proposal`, `reject_proposal`); `crate::http::auth::RequireUserSession`; `ApproveResponse` (Ok/Conflict/Gone)
**Confidence:** high

### 20.5: Expiry sweeper and queue cap
**Proposed tag:** [cross-cutting]
**Reasoning:** Adds a background sweeper task in `adapters`, registers it in `cli::runtime` as a long-lived supervised task (architecture §9 names "Proposal expiry" as a daemon task), and adds a `[tool_gateway] proposal_ttl_seconds` key to `core::config` — so it crosses adapters↔cli and touches core config. It writes `proposal.expired` audit rows on a schedule, a convention consumed by the audit viewer and slice 20.7. The queue-cap eviction-to-`superseded` behaviour is a shared invariant relied on by 20.1's `insert` and the 20.3 propose path.
**Affected seams:** none (extends the proposal pipeline; sweeper participates in the §9 task graph but no enumerated seam covers it)
**Planned contract additions:** `trilithon_adapters::proposal_expiry::ProposalExpirySweeper`, `ProposalSweepError`; `core::config` `proposal_ttl_seconds` field
**Confidence:** medium
**If low confidence, why:** The cli `runtime.rs` task-graph wiring is the only cross-layer touch; if eviction logic already lives entirely in 20.1's `insert`, this slice is close to a two-crate [standard], but the config + daemon-task wiring tips it cross-cutting.

### 20.6: Proposals web UI
**Proposed tag:** [standard]
**Reasoning:** Entirely within `web/src/features/proposals/` — one frontend feature module with its page, row, diff, and hook components plus tests. It consumes the existing `/api/v1/proposals` HTTP contract from 20.4 but adds no trait, no backend code, no audit/tracing events, and no cross-layer dependency. The web tier is a single layer and this is a self-contained feature within it.
**Affected seams:** none
**Planned contract additions:** none (frontend module; no Rust contract surface)
**Confidence:** high

### 20.7: End-to-end propose-approve-conflict scenarios
**Proposed tag:** [standard]
**Reasoning:** Three integration-test files under `crates/adapters/tests/`; no new public surface, no new traits, no production code. Each test drives the daemon through a scenario but the slice itself touches only the test tree. Although the scenarios exercise behaviour spanning multiple crates, the deliverable is test code in one crate's `tests/` directory with no contract or seam additions of its own.
**Affected seams:** exercises PROPOSED: tool-gateway-propose-pipeline and proposal-approval-apply (defined by 20.3/20.4); adds none
**Planned contract additions:** none
**Confidence:** high

## Summary
- 4 trivial / — / 4 standard / 3 cross-cutting / 2 low-confidence (none trivial)

Correction: 0 trivial / 4 standard / 3 cross-cutting / 2 low-confidence.

## Notes

- **No trivial slices.** Every slice either spans two crates, extends a shared closed set, implements a trait, or wires a daemon task. Phase 20 is integration-heavy by nature.
- **Schema divergence (action required before 20.1).** The plan's `0007_proposals.sql` and its column set (`proposal_id`, `basis_config_version`, `expires_at_unix_seconds`, `status`) do not match architecture §6.8's already-specified `proposals` table (`id`, `source` enum `docker|llm|import`, `expires_at`, `state`, `resulting_mutation`, `wildcard_callout`, `wildcard_ack_*`). The migration number `0007` also conflicts with the existing `0012_tokens_user_id.sql` lineage. The planner should reconcile the plan against §6.8 (or amend §6.8 via the contract-change path) before slice 20.1 lands. This is the single biggest risk in the phase.
- **Open Question carried from the plan (20.3).** The plan introduces `ToolGatewayError::ValidationFailed { errors: ValidationErrorSet }`. trait-signatures.md §7 *already* lists this variant and `ValidationErrorSet` as part of the closed set — so the trait surface is consistent and the plan's "introduces a fifth variant" wording is stale. The implementer should treat §7 as authoritative (the variant exists) rather than adding it; no divergence flag is needed.
- **Seam staging.** No existing seam in seams.md covers the propose pipeline (all five current seams are Phase 7 apply-path seams). Two new seams are proposed (`tool-gateway-propose-pipeline` for 20.3, `proposal-approval-apply` for 20.4); these go to `seams-proposed.md` for `/phase-merge-review` ratification, with cross-phase test files under `tests/cross_phase/`.
- **Contract surface.** contracts.md and contract-roots.toml currently carry only Phase 7 apply-path roots — nothing for the tool gateway (Phase 19) or proposals. Phase 20 will add a substantial new contract surface; `audit-contract-roots` will need new `[roots]` entries for `trilithon_core::proposals::*` and `trilithon_core::tool_gateway::*` once the symbols land.
- **Security-critical slice.** 20.4's `RequireUserSession` extractor enforces PRD T2.4's apply-authority boundary (model cannot approve its own proposal). It warrants the most adversarial review; 20.7's `proposals_model_cannot_approve.rs` is its regression guard.

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Notes from user
Auto-accepted.
