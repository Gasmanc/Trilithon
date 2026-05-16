# Phase 23 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:** CLAUDE.md; docs/architecture/architecture.md; docs/architecture/trait-signatures.md; docs/planning/PRD.md (T2.8); docs/adr/ (ADR-0010, ADR-0011, ADR-0014 referenced); docs/todo/phase-23-compose-deployment.md; docs/architecture/seams.md; docs/architecture/contract-roots.toml; docs/architecture/contracts.md
**Slices analysed:** 9

## Proposed Tags

### 23.1: Multi-stage Trilithon Dockerfile and `healthcheck` subcommand
**Proposed tag:** [standard]
**Reasoning:** The work touches a single crate (`core/crates/cli`): it adds a new `healthcheck` subcommand module, wires one variant into the existing clap `Command` enum, and adds a `reqwest` blocking dependency. It adds new outbound I/O (an HTTP GET) but confined to one adapter-free CLI command using a blocking client. The Dockerfile is a new build artefact, not code that crosses a layer boundary. The TODO explicitly states no audit rows and no tracing events are emitted, so it introduces no shared convention others depend on.
**Affected seams:** none
**Planned contract additions:** none — `HealthcheckArgs` and `healthcheck::run` are CLI-internal; the CLI crate has no contract roots in contract-roots.toml.
**Confidence:** high
**If low confidence, why:** —

### 23.2: Base `docker-compose.yml` (default profile)
**Proposed tag:** [trivial]
**Reasoning:** The slice creates only declarative deployment artefacts under `deploy/compose/` (a compose file, an `.env` example, a pinned-digest text file) plus a few shell test scripts. No Rust crate is touched, no trait is implemented, no layer boundary is crossed, and no audit/tracing convention originates here (the `daemon.started` event it references is emitted by pre-existing daemon code). It is self-contained config authoring.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** —

### 23.3: Opt-in Docker-discovery overlay and socket-trust enforcement test
**Proposed tag:** [trivial]
**Reasoning:** Adds one declarative compose overlay file and one bash lint script (plus test scripts) under `deploy/compose/`. No Rust code, no trait, no layer crossing. The `docker.socket-trust-grant` audit kind it references is emitted by slice 23.4's daemon code, not here; this slice only enforces a deployment invariant via static parsing with `yq`.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** —

### 23.4: First-run Docker socket trust-grant warning emission
**Proposed tag:** [cross-cutting]
**Reasoning:** This slice spans two crates — it adds a startup hook in `core/crates/cli/src/startup.rs` and references the `AuditEvent::DockerSocketTrustGrant` variant in `core/crates/core/src/audit`. It introduces a daemon-startup side effect that writes an audit row through the `docker.socket-trust-grant` kind (an architecture §6.6 convention) and crosses the cli↔core boundary plus consumes the adapters audit-log writer. It also performs new filesystem I/O (a write-mode `open(2)` probe) at startup and emits to three sinks. It references ADR-0010, hazard H11, architecture §6.6 and §12.1 — a multi-crate, audit-emitting, layer-crossing change.
**Affected seams:** none directly registered; closest is `applier-audit-writer` (audit row provenance), but the trust-grant path does not flow through the applier. PROPOSED: `daemon-startup-audit` — daemon startup hook ↔ audit log writer (`core::audit::AuditEvent::DockerSocketTrustGrant`, `Storage::record_audit_event`).
**Planned contract additions:** PROPOSED `DOCKER_SOCKET_TRUST_GRANT_BLOCK` const and `emit_docker_socket_trust_grant_if_present` in `crates/cli/src/startup.rs` (cli-internal, not a contract root). The `AuditEvent::DockerSocketTrustGrant` variant already exists in `core/crates/core/src/audit/event.rs` (verified) — no new core symbol, the slice's "add if not already present" is a no-op.
**Confidence:** high
**If low confidence, why:** —

### 23.5: GHCR publish workflow with multi-arch build, cosign signing, and SBOM
**Proposed tag:** [trivial]
**Reasoning:** The slice creates a single GitHub Actions workflow YAML and appends a heading + verification command to a markdown README. No Rust crate is touched, no trait, no layer boundary, no audit/tracing events. It is CI/build-pipeline infrastructure authoring; the image-size budget gate runs in CI only. Self-contained within `.github/workflows/` and one doc file.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** —

### 23.6: Compose smoke-test script and 30-second bootstrap timing gate
**Proposed tag:** [standard]
**Reasoning:** The slice authors a bash smoke-test script and one CI workflow under `deploy/compose/` and `.github/workflows/`. It writes no Rust code and crosses no layer boundary, but it is more than trivial: it exercises a multi-endpoint integration path (login, route apply, route probe, cosign verify) against the live HTTP API and depends on slices 23.1–23.5 plus Phases 9 and 11. It does not introduce conventions others follow, so it stays below cross-cutting. Standard captures the integration-test scope without overstating reach.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** medium
**If low confidence, why:** Could reasonably be argued [trivial] since it is only a shell script and a workflow, but its dependency on five upstream slices plus two prior phases' API surfaces makes [standard] the safer call.

### 23.7: Upgrade-from-prior smoke test and `UPGRADING.md` rationale paragraph
**Proposed tag:** [standard]
**Reasoning:** The slice adds a bash upgrade-smoke script and an operator-facing `UPGRADING.md`. No Rust crate, no trait, no layer crossing. Like 23.6 it is an integration test that exercises a non-trivial path — boot prior image, apply route, upgrade, verify schema migration and route persistence — and the doc encodes the migration-failure/rollback contract (exit code 4, single-transaction migration). It references architecture §14 but does not itself introduce a convention; [standard] fits the integration-test scope.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** medium
**If low confidence, why:** Borderline [trivial] (shell script plus markdown), but the upgrade/migration verification path and the documented rollback contract push it to [standard].

### 23.8: Operator and end-user documentation
**Proposed tag:** [trivial]
**Reasoning:** The slice writes two markdown documentation files and one heading-lint bash script. No Rust, no trait, no layer boundary, no audit/tracing events, no I/O beyond reading files in a lint. Pure documentation plus a static lint — the lowest-risk category.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** —

### 23.9: Wire deployment lints into `just check` and image-size budget into CI
**Proposed tag:** [trivial]
**Reasoning:** The slice edits the `justfile` to add a `deployment-checks` recipe into the `check` target and appends a `deployment-lints` job to the existing publish workflow. No Rust code, no trait, no layer boundary, no audit/tracing events. It is build-orchestration glue wiring already-authored scripts into the gate — small and self-contained.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** —

## Summary
- 5 trivial / 3 standard / 1 cross-cutting / 0 low-confidence

## Notes

- Phase 23 is overwhelmingly deployment/CI artefact authoring (compose files, Dockerfile, GitHub Actions workflows, shell scripts, markdown). Only two slices touch Rust code: 23.1 (CLI subcommand, single crate) and 23.4 (startup hook crossing cli↔core and emitting an audit row).
- **23.4 is the sole cross-cutting slice.** It crosses the cli↔core layer boundary, performs startup-time filesystem I/O, and writes through the architecture §6.6 audit vocabulary. Verified during analysis: `AuditEvent::DockerSocketTrustGrant` already exists in `core/crates/core/src/audit/event.rs` (variant present, `Display` returns `docker.socket-trust-grant`). The slice's instruction to "add the variant if not already present" is therefore a no-op — the implementer should confirm presence and not duplicate it.
- Minor TODO drift: slice 23.1 says wire the `Healthcheck` variant into the clap enum "in `core/crates/cli/src/main.rs`", but the actual `Command` enum lives in `core/crates/cli/src/cli.rs`. The TODO also assumes a `core/crates/cli/src/commands/` directory which does not yet exist (current CLI command modules live flat under `src/`, e.g. `run.rs`, `config_show.rs`). The implementer should follow the existing flat layout or the TODO's `commands/` subdir consistently; this does not change the [standard] tag.
- No seams in `seams.md` are exercised by Phase 23. The only candidate is a PROPOSED `daemon-startup-audit` seam for slice 23.4 (daemon startup hook ↔ audit log writer); if `/phase` decides the trust-grant emission warrants a cross-phase integration test, that seam should be staged in `seams-proposed.md` for `/phase-merge-review` ratification. The existing `applier-audit-writer` seam does not cover it because the trust-grant path bypasses the applier.
- No new contract roots: the `cli` crate has no entries in `contract-roots.toml`, and slice 23.4's new symbols are CLI-internal. No additions to `contracts.md` are expected from this phase.

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Notes from user
Auto-accepted.
