# Phase 27 — Tier 2 hardening and V1 release readiness

Source of truth: [`../phases/phased-plan.md#phase-27--tier-2-hardening-and-v1-release-readiness`](../phases/phased-plan.md#phase-27--tier-2-hardening-and-v1-release-readiness).

## Pre-flight checklist

- [ ] Phase 17 through Phase 26 complete.

## Tasks

### End-to-end flow tests

- [ ] **Concurrent mutation flow.**
  - Acceptance: A scripted CI flow MUST exercise concurrent mutation, conflict, rebase, and successful application.
  - Done when: the flow passes in CI.
  - Feature: T2.10.
- [ ] **Policy preset capability degradation flow.**
  - Acceptance: A scripted CI flow MUST attach `public-admin@1`, downgrade on stock Caddy, and upgrade on enhanced Caddy.
  - Done when: the flow passes in CI.
  - Feature: T2.2.
- [ ] **Language-model explain-then-propose flow.**
  - Acceptance: A scripted CI flow MUST exercise a language-model agent calling explain functions through the gateway, then proposing a route, the proposal being approved by a human, and the route serving traffic.
  - Done when: the flow passes in CI.
  - Feature: T2.3, T2.4.
- [ ] **Docker discovery wildcard-callout flow.**
  - Acceptance: A scripted CI flow MUST start a labelled container, observe the proposal within 5 seconds, surface the wildcard banner appropriately, and apply the route on approval.
  - Done when: the flow passes in CI.
  - Feature: T2.1, T2.11.
- [ ] **Access log viewer 10-million-line flow.**
  - Acceptance: A scripted CI flow MUST ingest a synthetic 10-million-line corpus, filter under 200 milliseconds, and trace a representative entry to its route.
  - Done when: the flow passes in CI.
  - Feature: T2.5, T2.6.
- [ ] **Deployment timing flow.**
  - Acceptance: Compose deployment MUST come up in under 30 seconds; systemd deployment MUST install in under 60 seconds.
  - Done when: both timings are asserted in CI.
  - Feature: T2.7, T2.8.
- [ ] **Native bundle round-trip flow.**
  - Acceptance: The native bundle MUST round-trip on the same machine and across machines.
  - Done when: both flows pass in CI.
  - Feature: T2.9, T2.12.

### Performance verification at 5,000 routes

- [ ] **Route list render under 1 second.**
  - Acceptance: With 5,000 routes loaded, the route list MUST render in under 1 second.
  - Done when: the CI performance harness asserts the timing.
  - Feature: T1.8.
- [ ] **Single mutation apply percentiles.**
  - Acceptance: With 5,000 routes loaded, single mutation apply MUST be under 1.5 seconds median and under 7 seconds at the 99th percentile.
  - Done when: the CI performance harness asserts the percentiles.
  - Feature: T1.1.
- [ ] **Drift-check tick under 5 seconds.**
  - Acceptance: With 5,000 routes loaded, a drift-check tick MUST complete in under 5 seconds.
  - Done when: the CI performance harness asserts the timing.
  - Feature: T1.4.
- [ ] **Memory ceiling at idle under 400 MiB resident.**
  - Acceptance: At idle with 5,000 routes loaded, resident memory MUST stay under 400 MiB.
  - Done when: the CI performance harness asserts the ceiling.
  - Feature: foundational.

### Security review

- [ ] **Update `docs/architecture/security-review.md` for Tier 2.**
  - Acceptance: Each of H1 through H17 MUST receive an updated written confirmation paragraph against the Tier 2 surface. The Docker socket trust boundary (H11) and the language-model boundary (H16) MUST receive dedicated re-review.
  - Done when: the document is updated and all paragraphs are present.
  - Feature: foundational.

### Documentation pass

- [ ] **Audit doc comments and component headers.**
  - Acceptance: Every public Rust item MUST have accurate doc and every web component MUST have an accurate header comment.
  - Done when: a `cargo doc -D rustdoc::missing_docs` build passes and the lint over component headers passes.
  - Feature: foundational.
- [ ] **User-facing documentation pass.**
  - Acceptance: User-facing documentation MUST cover installation (compose, systemd), bootstrap, first route, drift, rollback, secrets reveal, language-model setup, Docker discovery, backup, restore, and uninstall.
  - Done when: each section exists and is referenced from the index.
  - Feature: foundational.

### Install-and-upgrade matrix

- [ ] **Fresh install matrix.**
  - Acceptance: The matrix MUST cover fresh install on Ubuntu 24.04, Debian 12, Docker Compose on Linux, and Docker Compose on macOS.
  - Done when: each cell runs cleanly in CI and is recorded in `docs/release/v1-matrix.md`.
  - Feature: T2.7, T2.8.
- [ ] **Tier 1 to Tier 2 schema upgrade.**
  - Acceptance: An upgrade from a Phase 16 (Tier 1) database to Phase 27 (Tier 2 complete) database MUST proceed via migrations.
  - Done when: a CI test exercises the upgrade and asserts the resulting schema version.
  - Feature: T2.12 substrate.
- [ ] **Record the upgrade-only verdict for downgrade.**
  - Acceptance: Downgrade is OUT OF SCOPE FOR V1; the matrix MUST record a clean "upgrade-only" verdict.
  - Done when: the verdict is recorded in the matrix document.
  - Feature: foundational.

### Release notes

- [ ] **Publish V1 release notes.**
  - Acceptance: V1 release notes MUST list every T1.x and T2.x feature and its acceptance status.
  - Done when: `docs/release/v1-release-notes.md` exists and links every feature acceptance test.
  - Feature: foundational.

### Tier 1 regression guard

- [ ] **Re-run Phase 16 acceptance.**
  - Acceptance: Tier 1 acceptance MUST be re-run as part of this phase; any Tier 1 regression MUST block the phase.
  - Done when: the CI job re-runs every Phase 16 sign-off check.
  - Feature: foundational.

## Cross-references

- ADR-0001 through ADR-0014 (Tier 2 ADRs reviewed end-to-end).
- PRD T1.1 through T1.15, T2.1 through T2.12.
- Architecture: "End-to-end flows," "Performance budgets — 5,000 routes," "Install-and-upgrade matrix."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] Every Tier 2 end-to-end flow test passes in continuous integration.
- [ ] Every Tier 2 performance budget is met or recorded as a known regression with an open issue.
- [ ] Every hazard has an updated written confirmation paragraph.
- [ ] The install-and-upgrade matrix is exercised in CI for every supported target.
- [ ] V1 release notes are published, listing every T1.x and T2.x feature and its acceptance status.
