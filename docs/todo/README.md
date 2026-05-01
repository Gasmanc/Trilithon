# Trilithon Per-Phase Implementation Slices

This directory contains one slice-by-slice implementation breakdown per phase of the Trilithon V1 build. Each file maps one-to-one to a phase document under [`../phases/`](../phases/) and decomposes that phase into independently shippable slices a less-capable coding agent can execute end-to-end.

Voice and procedure rules come from [`../prompts/PROMPT-spec-generation.md`](../prompts/PROMPT-spec-generation.md) sections 8.5 and 9.

## How these documents relate

- **`docs/phases/phased-plan.md`** — the overall V1 roadmap, sequencing, entry/exit criteria, and effort estimates.
- **`docs/phases/phase-NN-<title>.md`** — the phase reference. Defines what the phase delivers, its acceptance bar, the audit kinds and tracing events it emits, the trait surfaces it touches, the test files it produces. Authoritative for the WHAT.
- **`docs/todo/phase-NN-<title>.md`** (this directory) — the slice breakdown. Decomposes the phase into 4–10 vertical slices. Each slice is an independently testable, independently shippable increment of work with exact files, full Rust/TypeScript signatures, numbered algorithm pseudocode, named tests, and a single acceptance command. Authoritative for the HOW.

A coding agent picks one slice, implements it, runs the slice's named tests, and ships. Slices are sequenced within a phase so that each builds on prior slices in the same phase.

## Tier 1 — Foundational phases

| Phase | Slice TODO | Phase reference | Summary |
|-------|------------|------------------|---------|
| 1 | [phase-01-daemon-skeleton.md](phase-01-daemon-skeleton.md) | [../phases/phase-01-daemon-skeleton.md](../phases/phase-01-daemon-skeleton.md) | Daemon binary, configuration loader, tracing, signals, exit codes. |
| 2 | [phase-02-sqlite-persistence.md](phase-02-sqlite-persistence.md) | [../phases/phase-02-sqlite-persistence.md](../phases/phase-02-sqlite-persistence.md) | SQLite + WAL, embedded migrations, Tier 1 schema. |
| 3 | [phase-03-caddy-adapter.md](phase-03-caddy-adapter.md) | [../phases/phase-03-caddy-adapter.md](../phases/phase-03-caddy-adapter.md) | Caddy admin client, capability probe, ownership sentinel. |
| 4 | [phase-04-mutation-algebra.md](phase-04-mutation-algebra.md) | [../phases/phase-04-mutation-algebra.md](../phases/phase-04-mutation-algebra.md) | Closed typed mutation set, capability gating, pure core. |
| 5 | [phase-05-snapshot-writer.md](phase-05-snapshot-writer.md) | [../phases/phase-05-snapshot-writer.md](../phases/phase-05-snapshot-writer.md) | Content-addressed snapshots with parent linkage. |
| 6 | [phase-06-audit-log.md](phase-06-audit-log.md) | [../phases/phase-06-audit-log.md](../phases/phase-06-audit-log.md) | Immutable audit log, secrets-aware redactor. |
| 7 | [phase-07-apply-path.md](phase-07-apply-path.md) | [../phases/phase-07-apply-path.md](../phases/phase-07-apply-path.md) | Desired-state-to-Caddy reconciler with optimistic concurrency. |
| 8 | [phase-08-drift-detection.md](phase-08-drift-detection.md) | [../phases/phase-08-drift-detection.md](../phases/phase-08-drift-detection.md) | Drift detection and the three resolution paths. |
| 9 | [phase-09-http-api.md](phase-09-http-api.md) | [../phases/phase-09-http-api.md](../phases/phase-09-http-api.md) | Authenticated loopback HTTP API for mutations and reads. |
| 10 | [phase-10-secrets-vault.md](phase-10-secrets-vault.md) | [../phases/phase-10-secrets-vault.md](../phases/phase-10-secrets-vault.md) | Encrypt-at-rest with keychain-backed master key. |
| 11 | [phase-11-web-ui-shell.md](phase-11-web-ui-shell.md) | [../phases/phase-11-web-ui-shell.md](../phases/phase-11-web-ui-shell.md) | Web shell, login, route CRUD. |
| 12 | [phase-12-rollback-preflight.md](phase-12-rollback-preflight.md) | [../phases/phase-12-rollback-preflight.md](../phases/phase-12-rollback-preflight.md) | Snapshot history and one-click rollback with preflight. |
| 13 | [phase-13-caddyfile-import.md](phase-13-caddyfile-import.md) | [../phases/phase-13-caddyfile-import.md](../phases/phase-13-caddyfile-import.md) | Caddyfile lexer, parser, transformer, round-trip equivalence. |
| 14 | [phase-14-tls-and-upstream-health.md](phase-14-tls-and-upstream-health.md) | [../phases/phase-14-tls-and-upstream-health.md](../phases/phase-14-tls-and-upstream-health.md) | Certificate inventory and per-route upstream reachability. |
| 15 | [phase-15-dual-pane-editor.md](phase-15-dual-pane-editor.md) | [../phases/phase-15-dual-pane-editor.md](../phases/phase-15-dual-pane-editor.md) | Dual-pane editor with live cross-validation. |
| 16 | [phase-16-tier-1-hardening.md](phase-16-tier-1-hardening.md) | [../phases/phase-16-tier-1-hardening.md](../phases/phase-16-tier-1-hardening.md) | Failure-mode tests, performance budgets, security review. |

## Tier 2 — V1, after Tier 1 is solid

| Phase | Slice TODO | Phase reference | Summary |
|-------|------------|------------------|---------|
| 17 | [phase-17-concurrency-control.md](phase-17-concurrency-control.md) | [../phases/phase-17-concurrency-control.md](../phases/phase-17-concurrency-control.md) | Conflict surface and guided rebase workflow. |
| 18 | [phase-18-policy-presets.md](phase-18-policy-presets.md) | [../phases/phase-18-policy-presets.md](../phases/phase-18-policy-presets.md) | Seven V1 policy presets with versioning and capability degradation. |
| 19 | [phase-19-gateway-explain-mode.md](phase-19-gateway-explain-mode.md) | [../phases/phase-19-gateway-explain-mode.md](../phases/phase-19-gateway-explain-mode.md) | Bounded read-only language-model tool gateway. |
| 20 | [phase-20-gateway-propose-mode.md](phase-20-gateway-propose-mode.md) | [../phases/phase-20-gateway-propose-mode.md](../phases/phase-20-gateway-propose-mode.md) | Language-model proposal generation into the human-approval queue. |
| 21 | [phase-21-docker-discovery.md](phase-21-docker-discovery.md) | [../phases/phase-21-docker-discovery.md](../phases/phase-21-docker-discovery.md) | Docker label discovery, conflict detection, wildcard callouts. |
| 22 | [phase-22-access-log-viewer.md](phase-22-access-log-viewer.md) | [../phases/phase-22-access-log-viewer.md](../phases/phase-22-access-log-viewer.md) | Rolling access log store, filters, live tail, per-entry explanation. |
| 23 | [phase-23-compose-deployment.md](phase-23-compose-deployment.md) | [../phases/phase-23-compose-deployment.md](../phases/phase-23-compose-deployment.md) | Two-container Docker Compose with isolated socket trust. |
| 24 | [phase-24-systemd-deployment.md](phase-24-systemd-deployment.md) | [../phases/phase-24-systemd-deployment.md](../phases/phase-24-systemd-deployment.md) | Bare-metal systemd install for Ubuntu 24.04 LTS and Debian 12. |
| 25 | [phase-25-config-export.md](phase-25-config-export.md) | [../phases/phase-25-config-export.md](../phases/phase-25-config-export.md) | Caddy JSON, Caddyfile, and native bundle export. |
| 26 | [phase-26-backup-and-restore.md](phase-26-backup-and-restore.md) | [../phases/phase-26-backup-and-restore.md](../phases/phase-26-backup-and-restore.md) | Encrypted backup and validated cross-machine restore. |
| 27 | [phase-27-tier-2-hardening.md](phase-27-tier-2-hardening.md) | [../phases/phase-27-tier-2-hardening.md](../phases/phase-27-tier-2-hardening.md) | Tier 2 end-to-end flows and install/upgrade matrix. |

## Slice document format

Every slice TODO file follows this structure:

1. **Header.** Phase identifier, title, link to the phase reference document, link back to `phased-plan.md`, list of inputs the agent needs in context (which architecture sections, which trait signatures, which ADRs).
2. **Slice plan summary.** A table listing every slice in this phase with title, primary files touched, expected effort in ideal-engineering-hours, and dependencies on prior slices.
3. **One section per slice.** Each slice section MUST contain, in order:
   - **Goal.** One paragraph stating the increment this slice ships.
   - **Entry conditions.** Bulleted list of what must be true before starting the slice (prior slice numbers, environment ready, fixtures present).
   - **Files to create or modify.** Bulleted list with exact workspace-relative paths and a one-line purpose for each.
   - **Signatures and shapes.** Verbatim Rust or TypeScript code blocks for every public type, function, trait method, JSON envelope, or SQL DDL the slice introduces.
   - **Algorithm.** Numbered pseudocode for any non-trivial procedure.
   - **Tests.** Named tests with input fixtures and expected outcomes. Names match the test-file naming convention below.
   - **Acceptance command.** A single concrete command (`cargo test -p <crate> <test_path>`, `vitest <pattern>`, `curl <invocation>`, observable database row) that confirms the slice is correct.
   - **Exit conditions.** Bulleted list of post-conditions; the slice is complete when every one is true.
   - **Audit kinds emitted.** Reference to architecture §6.6 with the dotted strings the slice writes.
   - **Tracing events emitted.** Reference to architecture §12.1 with the event names the slice fires.
   - **Cross-references.** ADR numbers, PRD T-numbers, architecture sections, trait-signatures.md entries.
4. **Phase exit checklist.** Reproduces the phase reference's exit criteria as a checklist; ticked when every slice is shipped.

## Phase 28+ — Post-V1 sketch (Tier 3)

Tier 3 work is OUT OF SCOPE FOR V1. Hooks are noted in the architecture and the phased plan but no per-phase slice breakdowns are produced here; detailed phase plans are deferred until V2 planning.

## Path conventions

Every `crates/<name>/...` reference in any slice file is workspace-relative, anchored at `core/` in the repo. So `crates/core/src/foo.rs` resolves to `core/crates/core/src/foo.rs` on disk. Web frontend paths are anchored at `web/`, for example `web/src/features/routes/RoutesIndex.tsx`. Documentation paths are anchored at the repo root (`docs/...`). Bare deployment paths (`deploy/compose/...`, `deploy/systemd/...`) are anchored at the repo root.

## Test-file naming convention

Integration tests live at `core/crates/<crate>/tests/<area>_<scenario>.rs` where `<area>` is a noun phrase matching the feature area (`caddyfile_round_trip`, `concurrency_two_actor`, `policy_public_admin`, `export_bundle_determinism`) and `<scenario>` describes the situation. Unit tests live inline in `mod tests` blocks within the implementation file. Vitest tests live next to the implementation as `<Component>.test.tsx`.

## Authoritative vocabulary references

The following sources are authoritative; slice TODOs cross-reference them rather than restating their contents:

- Audit `kind` vocabulary: architecture §6.6 in [`../architecture/architecture.md`](../architecture/architecture.md).
- Tracing event names and span field keys: architecture §12.1.
- Rust trait signatures: [`../architecture/trait-signatures.md`](../architecture/trait-signatures.md).
- Native bundle format: [`../architecture/bundle-format-v1.md`](../architecture/bundle-format-v1.md).

Phases that emit a new audit kind, new tracing event, or define a new trait MUST update the authoritative source in the same commit.
