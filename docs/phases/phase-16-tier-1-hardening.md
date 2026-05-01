# Phase 16 — Tier 1 hardening and integration test sweep

Source of truth: [`../phases/phased-plan.md#phase-16--tier-1-hardening-and-integration-test-sweep`](../phases/phased-plan.md#phase-16--tier-1-hardening-and-integration-test-sweep).

## Pre-flight checklist

- [ ] Phase 1 through Phase 15 complete; every Tier 1 feature passes its own acceptance.

## Tasks

### Failure-mode integration tests

- [ ] **Caddy unreachable at startup.**
  - Acceptance: With Caddy unreachable, the daemon MUST retry, surface a banner, and attempt no apply.
  - Done when: an integration test asserts the retry curve and the banner.
  - Feature: T1.1.
- [ ] **Caddy unreachable mid-flight.**
  - Acceptance: An in-flight apply MUST return a typed error and the desired-state pointer MUST remain untouched.
  - Done when: an integration test asserts the typed error and the unchanged pointer.
  - Feature: T1.1.
- [ ] **SQLite locked beyond busy timeout.**
  - Acceptance: A mutation MUST return a typed retryable error; the user MUST see an actionable message.
  - Done when: an integration test induces lock contention and asserts the typed error.
  - Feature: T1.6.
- [ ] **Docker socket gone.**
  - Acceptance: With no Docker socket the daemon MUST emit "no Docker, no proposals" rather than panic; this is the Tier 2 substrate verifier.
  - Done when: an integration test running without the socket asserts no panic.
  - Feature: T2.1 substrate.
- [ ] **Capability probe failure.**
  - Acceptance: When the probe fails, modules MUST be listed as "unknown" and mutations referencing them MUST fail validation with a clear message.
  - Done when: an integration test asserts the probe-failure branch.
  - Feature: T1.11 (mitigates H5).
- [ ] **Bootstrap credentials file unwritable.**
  - Acceptance: An unwritable bootstrap path MUST cause exit `3` with a structured error.
  - Done when: an integration test asserts the exit code.
  - Feature: T1.14 (mitigates H13).
- [ ] **Master-key access denied.**
  - Acceptance: When the keychain is locked, the file fallback MUST engage and an audit row MUST record the choice.
  - Done when: an integration test asserts the fallback and the audit row.
  - Feature: T1.15.
- [ ] **SQLite corruption simulated via integrity-check failure.**
  - Acceptance: The daemon MUST emit a critical tracing event, surface a banner, and document a recovery path.
  - Done when: an integration test injects a non-`ok` integrity check result and asserts the response.
  - Feature: foundational (mitigates H14).

### Performance verification

- [ ] **Cold start to ready under 5 seconds.**
  - Acceptance: With 1,000 routes loaded, cold start to ready MUST take under 5 seconds on the reference hardware.
  - Done when: a CI performance harness records and asserts the timing.
  - Feature: T1.13.
- [ ] **Route list render under 500 milliseconds.**
  - Acceptance: With 1,000 routes loaded, the route list MUST render in under 500 milliseconds.
  - Done when: a Vitest performance harness records and asserts the timing.
  - Feature: T1.8.
- [ ] **Single mutation apply under 1 second median, 5 seconds 99th percentile.**
  - Acceptance: A single mutation MUST apply in under 1 second median and under 5 seconds at the 99th percentile.
  - Done when: a CI performance harness asserts the percentiles.
  - Feature: T1.1.
- [ ] **Drift-check tick under 2 seconds.**
  - Acceptance: A drift-check tick MUST complete in under 2 seconds.
  - Done when: the CI performance harness asserts the timing.
  - Feature: T1.4.
- [ ] **Memory ceiling at idle under 200 MiB resident.**
  - Acceptance: At idle with 1,000 routes loaded, resident memory MUST stay under 200 MiB.
  - Done when: the CI performance harness samples RSS and asserts the ceiling.
  - Feature: foundational.

### Security review

- [ ] **Author `docs/architecture/security-review.md`.**
  - Acceptance: Each of H1 through H17 MUST receive a written one-paragraph confirmation against the implementation, or an open question filed.
  - Done when: the document exists and references every hazard identifier.
  - Feature: foundational.

### Gate upgrade

- [ ] **Upgrade `just check` to include strict-mode tests.**
  - Acceptance: `just check` MUST run property tests for the mutation algebra, the round-trip Caddyfile corpus, the failure-mode tests, and the secrets-vault leak simulation.
  - Done when: `just check` runs all four suites and CI enforces them.
  - Feature: foundational.

### Demo

- [ ] **Author the end-to-end demo script.**
  - Acceptance: A scripted walkthrough MUST cover fresh install, bootstrap, first route, second route via Caddyfile import, drift detection (induced by manual `curl` to Caddy admin), adopt running state, rollback to first snapshot, and secrets reveal under step-up.
  - Done when: the script lives at `docs/demos/tier-1.md` and a CI job runs it cleanly against a fresh Caddy 2.8.
  - Feature: T1.1 through T1.15.

### Documentation pass

- [ ] **Doc-comment every public Rust item.**
  - Acceptance: Every public Rust item MUST carry a doc comment.
  - Done when: a `cargo doc --no-deps -D rustdoc::missing_docs` build passes.
  - Feature: foundational.
- [ ] **Header-comment every web component file.**
  - Acceptance: Every component file under `web/src/` MUST carry a header comment.
  - Done when: a lint rule asserts the presence on every file.
  - Feature: foundational.
- [ ] **Author the user-facing README.**
  - Acceptance: `docs/README.md` MUST cover installation, first-run, and recovery.
  - Done when: the document exists and references the demo script.
  - Feature: foundational.

## Cross-references

- ADR-0001 through ADR-0014 (Tier 1 ADRs reviewed end-to-end).
- PRD T1.1 through T1.15.
- Architecture: "Failure modes table," "Performance budgets," "Security review."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] Every failure-mode test passes.
- [ ] Every performance budget is met or documented as a known regression with an open issue.
- [ ] Every hazard from H1 through H17 has a written confirmation paragraph in `docs/architecture/security-review.md`.
- [ ] The end-to-end demo script runs cleanly in continuous integration against a fresh Caddy 2.8 instance.
