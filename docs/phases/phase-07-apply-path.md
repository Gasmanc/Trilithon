# Phase 7 — Configuration ownership reconciler (apply path)

Source of truth: [`../phases/phased-plan.md#phase-7--configuration-ownership-reconciler-apply-path`](../phases/phased-plan.md#phase-7--configuration-ownership-reconciler-apply-path).

## Pre-flight checklist

- [ ] Phase 3 complete (Caddy adapter).
- [ ] Phase 4 complete (mutation algebra).
- [ ] Phase 5 complete (snapshots).
- [ ] Phase 6 complete (audit log; apply outcomes write audit rows).

## Tasks

### Backend / core crate

- [ ] **Implement the `CaddyJsonRenderer`.**
  - Acceptance: A pure-core renderer MUST convert a `DesiredState` into a Caddy 2.x configuration JSON document and produce byte-identical output for byte-identical inputs.
  - Done when: `cargo test -p trilithon-core renderer::tests::deterministic` passes for the fixture corpus.
  - Feature: T1.1.
- [ ] **Encode typed apply outcomes.**
  - Acceptance: `ApplyOutcome` MUST cover `Succeeded`, `Failed { kind }`, and `Conflicted { stale_version, current_version }`.
  - Done when: every variant is exercised by a unit test.
  - Feature: T1.1.

### Backend / adapters crate

- [ ] **Implement the `Applier`.**
  - Acceptance: The `Applier` MUST serialise the current desired state, call `POST /load`, fetch `GET /config/` to confirm equivalence, and write either `ApplySucceeded` or `ApplyFailed` audit rows.
  - Done when: integration tests cover happy path and Caddy validation rejection.
  - Feature: T1.1.
- [ ] **Optimistic concurrency on `config_version`.**
  - Acceptance: Every apply MUST carry the `config_version` of the snapshot it is realising; a stale version MUST surface a typed conflict error.
  - Done when: a concurrent-applies integration test observes exactly one winner and a typed conflict for the loser.
  - Feature: T1.1 (substrate for T2.10).
- [ ] **In-process mutex plus SQLite advisory lock per `caddy_instance_id`.**
  - Acceptance: Only one apply MUST be in flight per `caddy_instance_id`. The applier MUST hold both an in-process mutex and a SQLite advisory lock.
  - Done when: a stress test launching 32 concurrent applies asserts exactly one in-flight at any moment.
  - Feature: T1.1.
- [ ] **Failure handling leaves the desired-state pointer untouched.**
  - Acceptance: An apply that fails at Caddy validation MUST NOT advance any pointer; the failure MUST be reported via audit and surfaced to the caller as a typed error.
  - Done when: an integration test with a deliberately invalid desired state asserts the unchanged pointer and the `ApplyFailed` audit row.
  - Feature: T1.1.
- [ ] **Record Caddy reload semantics in the audit row.**
  - Acceptance: Every `ApplyStarted` and terminal audit row MUST record Caddy's reload semantics (graceful, abrupt) so downstream phases surface drain behaviour, satisfying H4.
  - Done when: the audit row schema carries the field and a unit test asserts population.
  - Feature: T1.1 (mitigates H4).
- [ ] **Re-check capability cache at apply time.**
  - Acceptance: The applier MUST re-check the live capability cache before submitting `POST /load`, satisfying H5.
  - Done when: an integration test with a hot-swapped capability cache rejects the apply with a typed error.
  - Feature: T1.1 (mitigates H5).
- [ ] **Distinguish "applied" from "TLS issuing" in audit metadata.**
  - Acceptance: The applier MUST NOT block on certificate issuance; the audit row MUST record `applied_state = applied` separately from any TLS issuance state, satisfying H17.
  - Done when: an integration test with a freshly added managed host observes `applied` immediately and a separate later observation of TLS state.
  - Feature: T1.1 (mitigates H17).

### Tests

- [ ] **Idempotent apply on identical state.**
  - Acceptance: Given desired state X and running state X, no apply MUST be performed.
  - Done when: an integration test asserts zero `ApplyStarted` audit rows under that condition.
  - Feature: T1.1.
- [ ] **Single apply on changed state.**
  - Acceptance: Given desired state Y and running state X, exactly one apply MUST be performed and the resulting running state MUST equal Y.
  - Done when: an integration test asserts both invariants.
  - Feature: T1.1.
- [ ] **Exactly one terminal audit row per apply.**
  - Acceptance: Every apply MUST produce exactly one `ApplyStarted` and exactly one terminal audit row (`ApplySucceeded`, `ApplyFailed`, or `ApplyConflicted`).
  - Done when: a property test over random apply sequences asserts the invariant.
  - Feature: T1.1.

### Documentation

- [ ] **Document the apply lifecycle.**
  - Acceptance: `core/README.md` MUST add an "Apply lifecycle" section covering optimistic concurrency, the audit invariants, and the H4/H5/H17 mitigations.
  - Done when: the section exists and references ADR-0012.
  - Feature: T1.1.

## Cross-references

- ADR-0002 (Caddy JSON admin API as source of truth).
- ADR-0009 (immutable content-addressed snapshots and audit log).
- ADR-0012 (optimistic concurrency on monotonic `config_version`).
- ADR-0013 (capability probe gates optional Caddy features).
- PRD T1.1 (configuration ownership loop).
- Architecture: "Apply pipeline," "Concurrency control," "Failure modes — Caddy unreachable mid-flight."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] Given desired state X and running state X, no apply is performed.
- [ ] Given desired state Y and running state X, exactly one apply is performed and the resulting running state equals Y.
- [ ] An apply that fails at Caddy validation does not advance the desired-state pointer.
- [ ] All applies are wrapped in optimistic concurrency control on `config_version`; a stale apply is rejected with a typed conflict error.
- [ ] Every apply produces exactly one `ApplyStarted` and exactly one terminal audit row.
