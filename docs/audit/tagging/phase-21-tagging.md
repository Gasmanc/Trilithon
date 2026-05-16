# Phase 21 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:** CLAUDE.md, architecture.md, trait-signatures.md, PRD.md, ADR-0007, ADR-0008, ADR-0010 (plus ADR directory listing), phase-21-docker-discovery.md, seams.md, seams-proposed.md, contract-roots.toml, contracts.md
**Slices analysed:** 9

## Proposed Tags

### 21.1: Docker socket adapter (bollard)
**Proposed tag:** [cross-cutting]
**Reasoning:** This slice introduces the `core::docker::DockerWatcher` trait and its `DockerError` type per trait-signatures.md §11, with the default implementation `BollardDockerWatcher` in the `adapters` crate — that is a new shared trait spanning the core/adapters layer boundary. It also adds a new `bollard` external dependency, a new `core::docker` module, and `[docker]` configuration keys consumed by later slices. A new trait that other slices implement against and a new external dep both push this beyond a self-contained `standard` slice.
**Affected seams:** none (no existing seam matches); PROPOSED: `docker-watcher-engine` — DockerWatcher ↔ Docker/Podman engine socket
**Planned contract additions:** `trilithon_core::docker::DockerWatcher`, `trilithon_core::docker::DockerError`, `trilithon_core::docker::ContainerInspect`, `trilithon_core::docker::ContainerReachability`, `trilithon_core::docker::LabelChange`, `trilithon_core::docker::ContainerId`, `trilithon_core::docker::DockerEventStream`
**Confidence:** high
**If low confidence, why:** n/a

### 21.2: Watcher loop with reconnect backoff
**Proposed tag:** [standard]
**Reasoning:** Self-contained supervisor in `adapters` (`docker_watcher.rs`) wrapping the trait from 21.1; it emits only the already-defined `docker.event.received` tracing event and introduces no new audit kind, trait, or schema. The one touch outside `adapters` is a spawn-site wiring line in `cli/src/runtime.rs`, which is mechanical glue rather than a layer-crossing contract change, so it stays `standard` rather than `cross-cutting`.
**Affected seams:** none
**Planned contract additions:** `trilithon_adapters::docker_watcher::SupervisedDockerStream`, `trilithon_adapters::docker_watcher::spawn_supervised`, `trilithon_adapters::docker_watcher::SupervisorConfig`
**Confidence:** medium
**If low confidence, why:** the `cli/src/runtime.rs` spawn wiring is a second crate touch; it is wiring-only, but a strict reading could elevate it.

### 21.3: Label parser (pure)
**Proposed tag:** [standard]
**Reasoning:** A single pure module in `crates/core` (`docker/label_parser.rs`) with no I/O, no async, no new trait, and no audit or tracing events. It introduces the `LabelSpec` / `LabelParseError` type set that slice 21.4 consumes, which keeps it from being `trivial` (its output is a typed surface other slices depend on), but it is otherwise fully one-crate, one-module, one-layer.
**Affected seams:** none
**Planned contract additions:** `trilithon_core::docker::label_parser::LabelSpec`, `trilithon_core::docker::label_parser::LabelParseError`, `trilithon_core::docker::label_parser::parse_labels`, `trilithon_core::docker::label_parser::PolicyLabel`, `trilithon_core::docker::label_parser::TlsLabel`
**Confidence:** high
**If low confidence, why:** n/a

### 21.4: Proposal generator from labels
**Proposed tag:** [cross-cutting]
**Reasoning:** This slice spans `core` (the pure `proposal_generator.rs`) and `adapters` (the `docker_proposal_pump.rs` task), wires together the watcher stream, `ProposalStore`, `AuditLogStore`, and `RouteStore`, and emits the `mutation.proposed` / `mutation.rejected` audit kinds plus the `proposal.received` tracing event — an audit convention later slices (21.5, 21.6) follow. It also carries the architecture §7.3 proposal-lifecycle SLO and depends on Phase 20's `ProposalStore`, so it crosses a layer boundary and establishes a shared audit/tracing pattern.
**Affected seams:** none; PROPOSED: `docker-proposal-pump` — Docker event stream ↔ proposal queue (ProposalStore) with audit emission
**Planned contract additions:** `trilithon_core::docker::proposal_generator::intent_from_change`, `trilithon_core::docker::proposal_generator::ProposalIntent`, `trilithon_core::docker::proposal_generator::ProposalGenerationError`, `trilithon_adapters::docker_proposal_pump::spawn_pump`, `trilithon_adapters::docker_proposal_pump::PumpConfig`
**Confidence:** high
**If low confidence, why:** n/a

### 21.5: Hostname-collision conflict detector
**Proposed tag:** [cross-cutting]
**Reasoning:** Although the detector itself is a pure `core` module, the slice explicitly extends the `proposals` table schema (a new `intent_kind` column distinguishing `single` from `conflict`, called out as a schema change in the slice text and in the phase Open Questions). A migration to a shared persisted table that the proposal store, the pump, and the web UI all read is a cross-cutting change other slices depend on, and it touches both `core` and `adapters`.
**Affected seams:** none
**Planned contract additions:** `trilithon_core::docker::conflict_detector::detect_conflicts`, `trilithon_core::docker::conflict_detector::HostnameClaim`, `trilithon_core::docker::conflict_detector::HostnameConflict`; plus a migration adding `proposals.intent_kind`
**Confidence:** medium
**If low confidence, why:** the schema-extension path is one of two options the phase leaves open; if the planner picks the `mutation_json` envelope path instead, no migration lands and this drops to `standard`.

### 21.6: Wildcard-match security warning
**Proposed tag:** [cross-cutting]
**Reasoning:** This slice spans `core` (`wildcard_callout.rs`), `adapters` (the proposal pump call site), and `cli` (the `http/proposals.rs` approve handler gains a required `acknowledged_wildcard` field) — three crates across the core→adapters→cli boundary. It also changes the approval audit-row convention (`notes.wildcard_acknowledged`) and consumes the Phase 14 certificate inventory, satisfying hazard H3 / T2.11 contracts that the web UI slice (21.9) then mirrors.
**Affected seams:** none
**Planned contract additions:** `trilithon_core::docker::wildcard_callout::check_wildcard_match`, `trilithon_core::docker::wildcard_callout::WildcardCert`
**Confidence:** high
**If low confidence, why:** n/a

### 21.7: Trust-grant first-run warning and GET /api/v1/docker/status
**Proposed tag:** [cross-cutting]
**Reasoning:** The slice lives in `cli` but writes the `docker.socket-trust-grant` audit row (architecture §6.6), adds a new HTTP endpoint to the router, may introduce a generic `adapters::kv_store::KvStore` adapter, and reads watcher-supervisor shared state — it crosses cli↔adapters and establishes the H11 trust-grant audit convention. It references hazard H11, ADR-0007, and ADR-0010, and the new KV adapter is reusable infrastructure other code can depend on.
**Affected seams:** none
**Planned contract additions:** `trilithon_cli::http::docker::DockerStatus`, `trilithon_cli::http::docker::get_status`, `trilithon_cli::startup::print_docker_trust_grant_if_first_run`; possibly `trilithon_adapters::kv_store::KvStore`
**Confidence:** medium
**If low confidence, why:** the `KvStore` adapter may already exist ("if not present"); if it does, the new-shared-infrastructure argument weakens, but the cross-layer audit-emission and routing changes still hold the tag.

### 21.8: Podman compatibility
**Proposed tag:** [standard]
**Reasoning:** A focused extension to the existing `docker_bollard.rs` adapter (a `discover_default_socket` helper) plus a config validator tweak in `core::config`. No new trait, no new audit kind, no new tracing event; the slice text states the Podman path works "without code changes — only configuration." The two-crate touch is the only reason it is not `trivial`, and both touches are small and tightly related.
**Affected seams:** none
**Planned contract additions:** `trilithon_adapters::docker_bollard::discover_default_socket`
**Confidence:** high
**If low confidence, why:** n/a

### 21.9: Web UI badge and proposal-row Docker metadata
**Proposed tag:** [standard]
**Reasoning:** Entirely within the `web/` frontend (Docker status badge, proposal-row metadata, wildcard banner). It consumes daemon HTTP endpoints defined in 21.7 and 21.6 but adds no Rust crate, no trait, no audit kind. It touches several files in one layer (the React app) with co-located tests, which is the textbook `standard` shape — self-contained, one layer, may add UI-side I/O (React Query polling).
**Affected seams:** none
**Planned contract additions:** none (frontend; not part of the Rust contract registry)
**Confidence:** high
**If low confidence, why:** n/a

## Summary
- 0 trivial / 4 standard / 5 cross-cutting / 3 low-confidence
- standard: 21.2, 21.3, 21.8, 21.9
- cross-cutting: 21.1, 21.4, 21.5, 21.6, 21.7
- low-confidence: 21.2, 21.5, 21.7

## Notes

- **No slice is `trivial`.** Every slice in Phase 21 either introduces a typed surface another slice consumes, crosses a layer boundary, or emits an audit/tracing event — none meets the strict `trivial` bar (one module, no shared types, no events others depend on).
- **New traits in this phase.** Slice 21.1 introduces `core::docker::DockerWatcher` and `DockerError`, already documented in trait-signatures.md §11. The trait-signatures convention requires a matching test double (`DockerWatcherDouble` in `crates/adapters/tests/doubles/`) shipped in the same commit — flag this for the 21.1 implementer.
- **Seam registry gap.** `seams.md` currently contains only Phase 7 apply-path seams. Phase 21 introduces two genuine cross-phase boundaries: the DockerWatcher↔engine socket boundary (21.1) and the Docker-event-stream↔proposal-queue boundary (21.4). Both should be staged in `seams-proposed.md` by `/tag-phase` for `/phase-merge-review` to ratify. They are marked `PROPOSED:` above per the rubric.
- **Contract registry is empty.** `contracts.md` reports `contract_count: 0` and `contract-roots.toml` only roots Phase 7 reconciler symbols. The "Planned contract additions" above are the symbols a future `contract-roots.toml` edit would root for Phase 21; they are not yet tracked. Adding them is itself a contract change for `/phase-merge-review`.
- **Schema-change watch (21.5).** The phase Open Questions explicitly leaves the conflict-proposal representation undecided between an `intent_kind` column and a `mutation_json` envelope. Slice 21.5 as written picks the column (a migration). The migration ordering matters: 21.5 must land its migration before 21.9 renders conflict rows. If the envelope path is chosen, 21.5 reduces to `standard`.
- **Audit-kind vocabulary.** All audit kinds emitted by Phase 21 (`mutation.proposed`, `mutation.rejected`, `proposal.approved`, `docker.socket-trust-grant`) already exist in architecture §6.6 — no §6.6 vocabulary edit is required, which keeps 21.4/21.6/21.7 from being elevated purely on audit-vocabulary grounds. They remain cross-cutting on layer-spanning and convention grounds.
- **Tracing-vocabulary check.** Slice 21.2 notes `docker.connected` is *not* in §12.1 and correctly falls back to the existing `docker.event.received`. No §12.1 edit is needed for Phase 21; if any implementer wants a new event name, §12.1 must be updated in the same commit.

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Notes from user
Auto-accepted.
