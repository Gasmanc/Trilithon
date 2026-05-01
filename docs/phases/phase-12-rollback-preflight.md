# Phase 12 — Snapshot history and rollback with preflight

Source of truth: [`../phases/phased-plan.md#phase-12--snapshot-history-and-rollback-with-preflight`](../phases/phased-plan.md#phase-12--snapshot-history-and-rollback-with-preflight).

## Pre-flight checklist

- [ ] Phase 11 complete (web shell, route CRUD).

## Tasks

### Backend / core crate

- [ ] **Define the `RollbackRequest` mutation.**
  - Acceptance: The mutation MUST identify the target snapshot; the pre-condition MUST be that the target snapshot exists and is reachable from the current snapshot's history.
  - Done when: unit tests cover both pre-condition branches.
  - Feature: T1.3.
- [ ] **Implement the `Preflight` engine.**
  - Acceptance: The engine MUST produce a typed list of conditions, each with status (pass, fail, warn), a human-readable message, and a stable identifier suitable for per-condition override.
  - Done when: unit tests cover every condition class and every status.
  - Feature: T1.3.
- [ ] **Define the three Phase 12 preflight conditions.**
  - Acceptance: The engine MUST implement `upstream-tcp-reachable` (per upstream), `tls-issuance-valid` (per host with a managed certificate), and `module-available` (per referenced Caddy module).
  - Done when: unit tests cover each condition.
  - Feature: T1.3.

### Backend / adapters crate

- [ ] **Implement the upstream TCP reachability probe.**
  - Acceptance: The probe MUST carry a 2-second default timeout and produce a typed result.
  - Done when: integration tests cover happy path, timeout, and connection-refused branches.
  - Feature: T1.3 (substrate for T1.10).
- [ ] **Implement the TLS validity probe.**
  - Acceptance: The probe MUST verify the current certificate validity for managed hosts and produce a typed result.
  - Done when: integration tests cover valid, expired, and missing branches.
  - Feature: T1.3 (substrate for T1.9).

### HTTP endpoints

- [ ] **Implement `POST /api/v1/snapshots/{id}/preflight`.**
  - Acceptance: The endpoint MUST run preflight against the target snapshot and return the typed result.
  - Done when: integration tests cover passing and failing preflights.
  - Feature: T1.3.
- [ ] **Implement `POST /api/v1/snapshots/{id}/rollback`.**
  - Acceptance: The endpoint MUST accept an optional `overrides: [condition_id]` field, run preflight, apply if passing or overridden, and write audit rows for the rollback request, each override, and the apply outcome.
  - Done when: integration tests cover happy path, blocked path, and per-condition override path.
  - Feature: T1.3.

### Frontend / snapshot history

- [ ] **Implement the per-route history tab.**
  - Acceptance: The tab MUST show parent linkage, actor, intent, timestamps, and a one-click "Roll back to this point" button.
  - Done when: Vitest tests cover the rendering and the click handler.
  - Feature: T1.3.
- [ ] **Implement the rollback dialog with override toggles.**
  - Acceptance: The dialog MUST render the preflight result as a structured list; failing conditions MUST render with an "I understand" override toggle requiring a typed acknowledgement.
  - Done when: Vitest tests cover the toggles and the typed acknowledgement gate.
  - Feature: T1.3.
- [ ] **Capture an override reason bounded at 1024 characters.**
  - Acceptance: The override audit row MUST record the condition identifier, the actor, and a free-text reason with a 1024-character bound.
  - Done when: a Vitest test asserts the bound and an integration test asserts the persisted row.
  - Feature: T1.3.

### Tests

- [ ] **Stale-upstream rollback blocked by default.**
  - Acceptance: A rollback referencing a deleted upstream MUST fail preflight by default and MUST succeed only with explicit `upstream-tcp-reachable:override` and an audit row, satisfying H2.
  - Done when: an integration test exercises both branches.
  - Feature: T1.3 (mitigates H2).
- [ ] **Atomic rollback on full pass or full override.**
  - Acceptance: A rollback that passes preflight (or is fully overridden) MUST apply atomically.
  - Done when: an integration test asserts the apply lifecycle and the resulting running state.
  - Feature: T1.3.

## Cross-references

- ADR-0009 (immutable content-addressed snapshots and audit log).
- PRD T1.3 (one-click rollback with preflight).
- Architecture: "Rollback — preflight conditions," "Probe adapters," "Override audit."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] A rollback that fails preflight reports a structured error listing every failing condition.
- [ ] The user may override on a per-condition basis; each override is recorded in the audit log.
- [ ] A rollback that passes preflight (or is fully overridden) applies atomically.
- [ ] The snapshot history UI allows the user to browse parent linkage and trigger rollback.
