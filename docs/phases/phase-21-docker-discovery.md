# Phase 21 — Docker discovery, proposal queue, conflict surface

Source of truth: [`../phases/phased-plan.md#phase-21--docker-discovery-proposal-queue-conflict-surface`](../phases/phased-plan.md#phase-21--docker-discovery-proposal-queue-conflict-surface).

## Pre-flight checklist

- [ ] Phase 20 complete.

## Tasks

### Backend / adapters crate

- [ ] **Implement the `DockerWatcher` adapter.**
  - Acceptance: The watcher MUST use `bollard` (or equivalent) over the Docker Engine socket, MUST honour podman's Docker-compatible socket where present, and MUST emit typed `LabelChange` events for container start, stop, and label-change.
  - Done when: integration tests against a Docker-in-Docker fixture exercise all three transitions.
  - Feature: T2.1.
- [ ] **Reconnect on socket loss with bounded backoff.**
  - Acceptance: The watcher MUST reconnect on socket loss with bounded exponential backoff capped at 30 seconds.
  - Done when: an integration test that kills and restores the socket asserts the reconnect curve.
  - Feature: T2.1.

### Backend / core crate

- [ ] **Implement the `LabelParser`.**
  - Acceptance: A pure parser MUST parse `caddy.host`, `caddy.upstream.port`, `caddy.policy`, `caddy.tls`, and the documented label set into typed mutations.
  - Done when: unit tests cover every documented label and structured errors for malformed input.
  - Feature: T2.1.
- [ ] **Generate proposals with source `docker-discovery`.**
  - Acceptance: The proposal generator MUST take parsed labels and produce `propose_create_route`, `propose_update_route`, or `propose_delete_route` proposals with `source = docker-discovery` and the container identifier.
  - Done when: integration tests assert the proposal shape.
  - Feature: T2.1.
- [ ] **Conflict detector for hostname collisions.**
  - Acceptance: When two containers claim the same hostname, the generator MUST produce a single conflict proposal listing both candidates, never two competing proposals.
  - Done when: an integration test asserts the single-proposal invariant.
  - Feature: T2.1.

### Wildcard callout

- [ ] **Implement the `WildcardMatchSecurity` warning.**
  - Acceptance: At proposal-render time the proposal generator MUST check whether the proposed host matches an existing wildcard certificate and, if so, attach a typed `WildcardMatchSecurity` warning to the proposal, satisfying T2.11 and H3.
  - Done when: an integration test asserts the warning on a wildcard match.
  - Feature: T2.11 (mitigates H3).
- [ ] **Require explicit acknowledgement for wildcard proposals in UI.**
  - Acceptance: The wildcard banner MUST require explicit acknowledgement before the Approve button enables; the acknowledgement MUST be recorded in the audit log.
  - Done when: a Vitest test asserts the gate; an integration test asserts the audit row.
  - Feature: T2.11.

### Trust boundary

- [ ] **Print the H11 trust-grant warning at first run per data directory.**
  - Acceptance: The daemon's first-run output MUST display a stark warning explaining that mounting the Docker socket grants effective root, satisfying H11. The warning MUST appear once per data directory.
  - Done when: an integration test asserts the warning on first run and absence on second.
  - Feature: T2.1 (mitigates H11).

### HTTP endpoints

- [ ] **Implement `GET /api/v1/docker/status`.**
  - Acceptance: The endpoint MUST return connected, disconnected, or last-error status.
  - Done when: an integration test asserts each state.
  - Feature: T2.1.

### Frontend

- [ ] **Surface the Docker discovery status badge on the dashboard.**
  - Acceptance: The dashboard MUST render the discovery status with the H11 warning embedded.
  - Done when: a Vitest test asserts both elements.
  - Feature: T2.1.
- [ ] **Render Docker-sourced proposals with container metadata.**
  - Acceptance: The proposal queue UI from Phase 20 MUST render Docker-sourced proposals with container metadata; the wildcard banner MUST require acknowledgement before approval.
  - Done when: Vitest tests cover both elements.
  - Feature: T2.1, T2.11.

### Tests

- [ ] **Labelled container start produces a proposal within 5 seconds.**
  - Acceptance: A labelled container starting MUST produce a proposal within 5 seconds.
  - Done when: an integration test asserts the timing.
  - Feature: T2.1.
- [ ] **Labelled container stop produces a remove-route proposal.**
  - Acceptance: A labelled container stopping MUST produce a "remove route" proposal.
  - Done when: the integration test passes.
  - Feature: T2.1.

## Cross-references

- ADR-0007 (proposal-based Docker discovery).
- ADR-0008 (bounded typed tool gateway — proposal queue is shared).
- PRD T2.1 (Docker container discovery), T2.11 (wildcard callout).
- Architecture: "Docker discovery," "Wildcard callout," "Trust boundary — Docker socket."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] A container with valid Caddy labels produces a proposal within 5 seconds of starting.
- [ ] A container destruction produces a "remove route" proposal.
- [ ] A label conflict produces a single conflict proposal listing both candidates.
- [ ] Wildcard-certificate matches are highlighted with a security callout requiring explicit acknowledgement, satisfying T2.11.
- [ ] The daemon's first-run output displays the Docker socket trust warning, satisfying H11.
