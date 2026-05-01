# Phase 20 — Language-model propose mode

Source of truth: [`../phases/phased-plan.md#phase-20--language-model-propose-mode`](../phases/phased-plan.md#phase-20--language-model-propose-mode).

## Pre-flight checklist

- [ ] Phase 19 complete.

## Tasks

### Database migrations

- [ ] **Author migration `0007_proposals.sql`.**
  - Acceptance: A migration MUST add `proposals` with `proposal_id`, `source`, `source_identifier`, `mutation_json`, `expires_at_unix_seconds`, `status`, `created_at`, `decided_at`, and `decided_by`.
  - Done when: a schema-introspection test asserts the columns.
  - Feature: T2.4.

### Backend / core crate

- [ ] **Add propose scopes to the gateway.**
  - Acceptance: The scope set MUST gain `propose.routes`, `propose.policies`, `propose.upstreams`. Existing read scopes MUST remain unchanged.
  - Done when: a unit test enumerates the closed scope set.
  - Feature: T2.4.
- [ ] **Implement proposal validation in the standard pipeline.**
  - Acceptance: A proposed mutation MUST pass through the standard validation pipeline (capability gating and policy enforcement). A proposed mutation that violates an attached policy MUST be rejected at validation, not at apply.
  - Done when: integration tests cover happy and rejection paths.
  - Feature: T2.4.

### HTTP endpoints

- [ ] **Implement gateway propose functions.**
  - Acceptance: The gateway MUST implement `propose_create_route`, `propose_update_route`, `propose_delete_route`, `propose_attach_policy`. Each call MUST validate, create a `pending` proposal, and return the proposal identifier and the validation result.
  - Done when: integration tests cover each function.
  - Feature: T2.4.
- [ ] **Implement `GET /api/v1/proposals`.**
  - Acceptance: The endpoint MUST return pending proposals filterable by source.
  - Done when: an integration test asserts the response shape.
  - Feature: T2.4.
- [ ] **Implement `POST /api/v1/proposals/{id}/approve` and `POST /api/v1/proposals/{id}/reject`.**
  - Acceptance: Approval MUST require an authenticated human session; a tool-gateway token MUST NOT be sufficient. Approval MUST run the mutation through the standard apply pipeline.
  - Done when: integration tests cover approve, reject, and the gateway-token-rejection branch.
  - Feature: T2.4.

### Backend / adapters crate

- [ ] **Implement proposal expiry.**
  - Acceptance: The daemon MUST periodically transition expired proposals to `expired` and write a `ProposalExpired` audit row. The default expiry MUST be 24 hours and MUST be configuration-overridable.
  - Done when: an integration test with a short configured expiry asserts the transition.
  - Feature: T2.4.
- [ ] **Per-token proposal rate limit and queue cap.**
  - Acceptance: A per-token rate limit MUST apply, and the proposal queue MUST be capped at 200 pending with the oldest expiring first.
  - Done when: integration tests assert both bounds.
  - Feature: T2.4.

### Frontend

- [ ] **Implement the Proposals page.**
  - Acceptance: The page MUST list pending proposals with source attribution, intent, and a diff preview. Approve and Reject buttons MUST require explicit confirmation.
  - Done when: Vitest tests cover both confirmations.
  - Feature: T2.4.

### Tests

- [ ] **Policy violation rejected at proposal time.**
  - Acceptance: A proposal that violates an attached policy MUST be rejected with a typed error and never applied.
  - Done when: the integration test passes.
  - Feature: T2.4.
- [ ] **Model cannot approve its own proposal.**
  - Acceptance: A model MUST NOT be able to approve its own proposal.
  - Done when: an integration test attempting model approval observes 401 / 403.
  - Feature: T2.4.
- [ ] **Stale proposal flows through the conflict path.**
  - Acceptance: A proposal whose basis `config_version` is stale at approval MUST flow through the Phase 17 conflict path.
  - Done when: an integration test asserts the conflict-and-rebase flow.
  - Feature: T2.4.

## Cross-references

- ADR-0007 (proposal-based Docker discovery — same queue).
- ADR-0008 (bounded typed tool gateway for language models).
- ADR-0012 (optimistic concurrency on monotonic `config_version`).
- PRD T2.4 (language-model "propose" mode).
- Architecture: "Proposals queue," "Tool gateway — propose."
- Trait signatures: `core::tool_gateway::ToolGateway` in `docs/architecture/trait-signatures.md` (full async signature for `invoke_propose`; rate-limit and queue-cap behaviour governed by `ToolGatewayError::RateLimited`).

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] The model cannot apply a proposal directly. Approval requires an authenticated user action.
- [ ] Proposals expire after a configurable window (default 24 hours).
- [ ] The model cannot bypass policy presets: a proposal that would violate an attached policy is rejected at validation.
