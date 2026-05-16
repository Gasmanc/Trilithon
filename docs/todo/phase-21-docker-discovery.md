# Phase 21 — Docker discovery, proposal queue, conflict surface — Implementation Slices

> Phase reference: [../phases/phase-21-docker-discovery.md](../phases/phase-21-docker-discovery.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference: `docs/phases/phase-21-docker-discovery.md`.
- Architecture §4 (component view), §6.6 (audit-kind vocabulary), §6.8 (`proposals`), §7.3 (proposal lifecycle), §8.2 (Docker engine API contract), §11 (security posture), §12.1 (tracing vocabulary).
- Trait signatures: `core::docker::DockerWatcher`, `DockerError`, `core::tool_gateway::ToolGateway`.
- ADRs: ADR-0007 (proposal-based Docker discovery), ADR-0008 (proposal queue is shared), ADR-0010 (two-container deployment).
- PRD: T2.1 (Docker container discovery), T2.11 (wildcard callout).
- Hazards: H3 (wildcard certificate match), H11 (Docker socket trust grant).

## Slice plan summary

| # | Slice | Primary files | Effort (h) | Depends on |
|---|-------|---------------|------------|------------|
| 21.1 | Docker socket adapter (bollard) | `crates/adapters/src/docker_bollard.rs` | 8 | — |
| 21.2 | Watcher loop with reconnect backoff | `crates/adapters/src/docker_watcher.rs` | 6 | 21.1 |
| 21.3 | Label parser (pure) | `crates/core/src/docker/label_parser.rs` | 6 | — |
| 21.4 | Proposal generator from labels | `crates/core/src/docker/proposal_generator.rs` | 6 | 21.3, Phase 20 slice 20.1 |
| 21.5 | Hostname-collision conflict detector | `crates/core/src/docker/conflict_detector.rs` | 4 | 21.4 |
| 21.6 | Wildcard-match security warning | `crates/core/src/docker/wildcard_callout.rs` | 4 | 21.4 |
| 21.7 | Trust-grant first-run warning and `GET /api/v1/docker/status` | `crates/cli/src/startup.rs`, `crates/cli/src/http/docker.rs` | 4 | 21.2 |
| 21.8 | Podman compatibility | `crates/adapters/src/docker_bollard.rs` | 3 | 21.1 |
| 21.9 | Web UI badge and proposal-row Docker metadata | `web/src/features/docker/*`, `web/src/features/proposals/ProposalRow.tsx` | 6 | 21.7 |

---

## Slice 21.1 [cross-cutting] — Docker socket adapter (bollard)

### Goal

Implement `DockerEngineClient` over `bollard` against the Docker engine Unix socket (default `/var/run/docker.sock`, configuration-overridable). Provide `events()` returning a typed event stream, `inspect_container(id)`, and `reachability_check(id)` per `trait-signatures.md` §11.

### Entry conditions

- Phase 20 complete.
- The `bollard` crate is on the dependency graph or added by this slice.

### Files to create or modify

- `core/crates/adapters/src/docker_bollard.rs` — adapter.
- `core/crates/adapters/src/lib.rs` — export `docker_bollard`.
- `core/crates/core/src/docker/mod.rs` — trait per `trait-signatures.md` §11.
- `core/crates/core/src/lib.rs` — `pub mod docker;`.
- `core/crates/core/src/config.rs` — add `[docker] socket_path: String` (default `/var/run/docker.sock`), `[docker] enabled: bool` (default false).

### Signatures and shapes

```rust
//! `core/crates/core/src/docker/mod.rs`

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContainerId(pub String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContainerInspect {
    pub id:           ContainerId,
    pub names:        Vec<String>,
    pub state:        ContainerState,
    pub labels:       std::collections::BTreeMap<String, String>,
    pub networks:     Vec<String>,
    pub ip_addresses: Vec<std::net::IpAddr>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContainerState { Running, Stopped, Restarting, Other }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LabelChange {
    Started      { id: ContainerId, labels: std::collections::BTreeMap<String, String> },
    Stopped      { id: ContainerId },
    LabelsUpdated{ id: ContainerId, labels: std::collections::BTreeMap<String, String> },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContainerReachability {
    pub on_trilithon_network: bool,
    pub network_names:        Vec<String>,
}

pub type DockerEventStream = futures::stream::BoxStream<'static, Result<LabelChange, DockerError>>;

#[async_trait::async_trait]
pub trait DockerWatcher: Send + Sync + 'static {
    async fn events(&self) -> Result<DockerEventStream, DockerError>;
    async fn inspect_container(&self, id: &ContainerId) -> Result<ContainerInspect, DockerError>;
    async fn reachability_check(&self, id: &ContainerId) -> Result<ContainerReachability, DockerError>;
}

#[derive(Debug, thiserror::Error)]
pub enum DockerError {
    #[error("docker socket unavailable: {detail}")]
    SocketUnavailable { detail: String },
    #[error("permission denied accessing docker socket")]
    Permission,
    #[error("docker engine error: {detail}")]
    EngineError { detail: String },
}
```

```rust
//! `core/crates/adapters/src/docker_bollard.rs`

use std::path::PathBuf;
use bollard::Docker;
use trilithon_core::docker::{
    ContainerId, ContainerInspect, ContainerReachability, DockerError, DockerEventStream, DockerWatcher, LabelChange,
};

pub struct BollardDockerWatcher {
    client:                  Docker,
    trilithon_network_names: Vec<String>,
}

impl BollardDockerWatcher {
    pub async fn connect(socket_path: &std::path::Path, trilithon_network_names: Vec<String>) -> Result<Self, DockerError>;
}

#[async_trait::async_trait]
impl DockerWatcher for BollardDockerWatcher { /* ... */ }
```

### Algorithm

`connect`:

1. `let client = Docker::connect_with_unix(socket_path.to_string_lossy().as_ref(), 30, bollard::API_DEFAULT_VERSION).map_err(|e| DockerError::SocketUnavailable { detail: e.to_string() })?`.
2. Issue `client.ping().await` to verify reachability. On `EACCES` map to `Permission`; on other errors map to `SocketUnavailable`.
3. Return `Self { client, trilithon_network_names }`.

`events`:

1. Subscribe via `client.events(...)` filtering on `type = container` and `event in (start, stop, die, kill, destroy, update)`.
2. For each event:
   - `start` → fetch labels via `inspect_container`; emit `LabelChange::Started { id, labels }` only if any label key starts with `caddy.`.
   - `stop|die|kill|destroy` → emit `LabelChange::Stopped { id }`.
   - `update` (label change) → fetch labels; emit `LabelChange::LabelsUpdated { id, labels }`.
3. Wrap as a `BoxStream`. Map errors to `DockerError::EngineError { detail }`.

`reachability_check`:

1. `inspect_container(id)`.
2. `let on_trilithon_network = inspect.networks.iter().any(|n| self.trilithon_network_names.contains(n))`.
3. Return `ContainerReachability { on_trilithon_network, network_names: inspect.networks }`.

### Tests

- `core/crates/adapters/tests/docker_bollard.rs` (gated behind `--features docker-integration` since these need a Docker daemon):
  - `connect_with_valid_socket_succeeds`.
  - `connect_with_missing_socket_returns_socket_unavailable`.
  - `inspect_returns_typed_record_for_running_container`.
  - `events_emits_started_for_caddy_labelled_container`.
  - `events_emits_stopped_for_destroyed_container`.
  - `events_filters_out_unlabelled_containers`.
  - `reachability_check_reports_trilithon_network_membership`.

The harness uses Docker-in-Docker per the phase reference; integration tests outside the feature flag stay green by default.

### Acceptance command

```
cargo test -p trilithon-adapters --features docker-integration --test docker_bollard
```

### Exit conditions

- All seven tests pass against a Docker-in-Docker fixture.
- The default build (without the feature flag) compiles.
- Permission errors map to `DockerError::Permission` distinctly from `SocketUnavailable`.

### Audit kinds emitted

None directly. The trust-grant audit row is written by slice 21.7.

### Tracing events emitted

Per §12.1: `docker.event.received` on every successfully decoded event.

### Cross-references

- ADR-0007.
- Trait signatures §11.

---

## Slice 21.2 [standard] — Watcher loop with reconnect backoff

### Goal

Wrap `BollardDockerWatcher::events` in a supervisor that reconnects on socket loss with bounded exponential backoff capped at 30 seconds. The supervisor exposes a `Stream<LabelChange>` that survives socket failures transparently.

### Entry conditions

- Slice 21.1 shipped.

### Files to create or modify

- `core/crates/adapters/src/docker_watcher.rs` — supervisor.
- `core/crates/cli/src/runtime.rs` — spawn the supervisor when `[docker] enabled = true`.

### Signatures and shapes

```rust
//! `core/crates/adapters/src/docker_watcher.rs`

use std::sync::Arc;
use std::time::Duration;
use trilithon_core::docker::{DockerWatcher, LabelChange};

pub struct SupervisedDockerStream {
    rx: tokio::sync::mpsc::Receiver<LabelChange>,
}

pub fn spawn_supervised(
    watcher:   Arc<dyn DockerWatcher>,
    shutdown:  tokio::sync::watch::Receiver<bool>,
    config:    SupervisorConfig,
) -> (SupervisedDockerStream, tokio::task::JoinHandle<()>);

#[derive(Clone, Debug)]
pub struct SupervisorConfig {
    pub initial_backoff: Duration, // default 250 ms
    pub max_backoff:     Duration, // default 30 s
    pub channel_size:    usize,    // default 256
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            initial_backoff: Duration::from_millis(250),
            max_backoff:     Duration::from_secs(30),
            channel_size:    256,
        }
    }
}
```

### Algorithm

Supervisor task body:

1. `let mut backoff = config.initial_backoff`.
2. Loop:
   - If shutdown signal: exit.
   - `match watcher.events().await`:
     - `Ok(stream)`:
       - On the first event, emit `tracing::info!(event = "caddy.connected")` analogue for Docker (`docker.connected` is not in §12.1; per the architecture `docker.event.received` covers it).
       - Reset `backoff = config.initial_backoff`.
       - For each item: forward to `tx`. If `tx.send` fails (receiver dropped), exit.
       - When the stream ends (socket loss): goto reconnect.
     - `Err(_)`:
       - `tokio::time::sleep(backoff).await`.
       - `backoff = (backoff * 2).min(config.max_backoff)`.

### Tests

- `core/crates/adapters/tests/docker_watcher_reconnect.rs`:
  - `reconnect_after_socket_loss_resumes_event_stream` — uses a fake `DockerWatcher` whose `events()` returns Err on the first call and Ok on the second.
  - `backoff_grows_exponentially_capped_at_30_seconds` — assert the sleep durations issued at each retry: 250ms, 500ms, 1s, 2s, 4s, 8s, 16s, 30s, 30s, 30s.
  - `shutdown_signal_terminates_supervisor`.

### Acceptance command

```
cargo test -p trilithon-adapters --test docker_watcher_reconnect
```

### Exit conditions

- All three tests pass.
- The supervisor uses a `tokio::time::Instant`-based clock fake in tests so the assertion is deterministic.
- The cap of 30 seconds is enforced.

### Audit kinds emitted

None.

### Tracing events emitted

Per §12.1: `docker.event.received` on each forwarded event. (No new event names introduced.)

### Cross-references

- ADR-0007.
- Phase 21 task: "Reconnect on socket loss with bounded backoff."

---

## Slice 21.3 [standard] — Label parser (pure)

### Goal

Parse the documented `caddy.*` label set into a typed `LabelSpec`. Pure function, no I/O. Malformed labels produce structured `LabelParseError` values.

### Entry conditions

- Slice 21.1 shipped (for `BTreeMap<String, String>` source shape).

### Files to create or modify

- `core/crates/core/src/docker/label_parser.rs`.
- `core/crates/core/src/docker/mod.rs` — re-export.

### Signatures and shapes

```rust
//! `core/crates/core/src/docker/label_parser.rs`

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LabelSpec {
    pub host:          Option<String>,        // caddy.host
    pub upstream_port: Option<u16>,           // caddy.upstream.port
    pub upstream_path: Option<String>,        // caddy.upstream.path
    pub policy:        Option<PolicyLabel>,   // caddy.policy
    pub tls:           Option<TlsLabel>,      // caddy.tls
    pub disabled:      bool,                  // caddy.disabled = "true"
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PolicyLabel { pub preset_id: String, pub version: u32 }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum TlsLabel {
    Auto,
    Internal,
    Disabled,
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum LabelParseError {
    #[error("caddy.host is required when any caddy.* label is present")]
    MissingHost,
    #[error("invalid hostname {host:?}: {detail}")]
    InvalidHost { host: String, detail: String },
    #[error("caddy.upstream.port must be a u16: {raw:?}")]
    InvalidPort { raw: String },
    #[error("caddy.policy must be of the form '<id>@<version>': {raw:?}")]
    InvalidPolicy { raw: String },
    #[error("unknown caddy label {key:?}")]
    UnknownLabel { key: String },
    #[error("caddy.tls must be one of: auto, internal, disabled: {raw:?}")]
    InvalidTls { raw: String },
}

pub fn parse_labels(
    labels: &std::collections::BTreeMap<String, String>,
) -> Result<LabelSpec, LabelParseError>;
```

### Algorithm

1. Filter `labels` to keys starting with `caddy.`.
2. If the filtered set is empty: return `Ok(LabelSpec::default())` (no Trilithon-relevant labels).
3. For each key:
   - `caddy.host` → validate as a hostname per RFC 1123; on failure `InvalidHost`.
   - `caddy.upstream.port` → parse as `u16`; on failure `InvalidPort`.
   - `caddy.upstream.path` → accept the raw string (Caddy validates).
   - `caddy.policy` → split on `@`; parse second half as `u32`; on failure `InvalidPolicy`.
   - `caddy.tls` → match against `auto`, `internal`, `disabled`; on failure `InvalidTls`.
   - `caddy.disabled` → treat any value other than `"true"` as `false`.
   - Any other `caddy.*` key → `UnknownLabel { key }`.
4. After parsing every key, if `host.is_none()` and any other label is present: return `MissingHost`.
5. Return `Ok(LabelSpec { ... })`.

### Tests

- `core/crates/core/src/docker/label_parser.rs` `mod tests`:
  - `parse_full_label_set`.
  - `parse_minimal_host_only`.
  - `missing_host_with_other_labels_errors`.
  - `invalid_host_errors`.
  - `invalid_port_errors`.
  - `invalid_policy_errors`.
  - `invalid_tls_errors`.
  - `unknown_label_errors`.
  - `empty_caddy_labels_returns_default`.
  - `disabled_true_recognised`.

### Acceptance command

```
cargo test -p trilithon-core docker::label_parser::tests
```

### Exit conditions

- All ten tests pass.
- The parser is pure (no `tokio`, no `std::fs`).

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- ADR-0007.
- Phase 21 task: "Implement the `LabelParser`."

---

## Slice 21.4 [cross-cutting] — Proposal generator from labels

### Goal

Translate `LabelChange` events into typed proposals (`propose_create_route`, `propose_update_route`, `propose_delete_route`) with `source = "docker-discovery"` and `source_identifier = container.id`. The generator persists proposals through `ProposalStore::insert`.

### Entry conditions

- Slices 21.2, 21.3 shipped.
- Phase 20 slice 20.1 (`ProposalStore`) shipped.

### Files to create or modify

- `core/crates/core/src/docker/proposal_generator.rs`.
- `core/crates/adapters/src/docker_proposal_pump.rs` — task that consumes the supervised stream and calls the generator.

### Signatures and shapes

```rust
//! `core/crates/core/src/docker/proposal_generator.rs`

use crate::docker::label_parser::LabelSpec;
use crate::docker::{ContainerId, LabelChange};
use crate::mutation::TypedMutation;

pub struct ProposalIntent {
    pub container_id: ContainerId,
    pub mutation:     TypedMutation,
}

pub fn intent_from_change(
    change:                  &LabelChange,
    existing_route_for_host: Option<&crate::desired_state::Route>,
) -> Result<Option<ProposalIntent>, ProposalGenerationError>;

#[derive(Debug, thiserror::Error)]
pub enum ProposalGenerationError {
    #[error("label parse: {0}")]
    LabelParse(#[from] crate::docker::label_parser::LabelParseError),
}
```

```rust
//! `core/crates/adapters/src/docker_proposal_pump.rs`

pub fn spawn_pump(
    rx:              crate::docker_watcher::SupervisedDockerStream,
    proposal_store:  std::sync::Arc<crate::proposal_store::ProposalStore>,
    audit:           std::sync::Arc<crate::audit_log_store::AuditLogStore>,
    route_store:     std::sync::Arc<crate::route_store::RouteStore>,
    config:          PumpConfig,
    shutdown:        tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()>;

pub struct PumpConfig {
    pub proposal_ttl_seconds: u64,        // from config.tool_gateway.proposal_ttl_seconds
    pub max_lag_seconds:      u64,        // 5 (per the phase reference's "within 5 seconds" SLO)
}
```

### Algorithm

`intent_from_change`:

1. Match on `LabelChange`:
   - `Started { id, labels }`: parse `LabelSpec`. If `disabled` → return `Ok(None)`. If `existing_route_for_host.is_some()`:
     - If existing `route.upstream` matches the spec → no proposal needed → `Ok(None)`.
     - Else build `TypedMutation::UpdateRoute(...)`, return `Ok(Some(...))`.
   - Else build `TypedMutation::CreateRoute(...)`, return `Ok(Some(...))`.
   - `Stopped { id }`: look up which routes were last sourced from this `id` (via `route.source = docker-discovery:<id>`). If none, `Ok(None)`. Else build `TypedMutation::DeleteRoute(...)`, return `Ok(Some(...))`.
   - `LabelsUpdated { id, labels }`: parse and treat like `Started` with `existing_route_for_host` populated.
2. Forward propagate label-parse errors as `ProposalGenerationError::LabelParse`.

`spawn_pump` task body:

1. For each `change` from `rx`:
   - `tracing::info!(event = "docker.event.received", container_id = %id)`.
   - Compute `intent_from_change`.
   - On `Err`: write one `mutation.rejected` audit row with the typed error in `notes`; do not insert a proposal.
   - On `Ok(None)`: skip.
   - On `Ok(Some(intent))`:
     - Compute `expires_at = now + config.proposal_ttl_seconds`.
     - `proposal_store.insert(conn, ProposalSource::DockerDiscovery, &intent.container_id.0, &intent.mutation, current_config_version, expires_at, now)?`.
     - Write one `mutation.proposed` audit row with `notes = { source: "docker-discovery", container_id }`.
2. Performance contract: from event receipt to proposal insertion MUST be under 5 seconds at the 95th percentile.

### Tests

- `core/crates/core/src/docker/proposal_generator.rs` `mod tests`:
  - `started_no_existing_route_yields_create_route_intent`.
  - `started_with_matching_route_yields_no_intent`.
  - `started_with_diverging_route_yields_update_route_intent`.
  - `stopped_with_existing_docker_route_yields_delete_route_intent`.
  - `stopped_with_no_known_route_yields_no_intent`.
  - `disabled_label_yields_no_intent`.
  - `label_parse_error_propagates`.
- `core/crates/adapters/tests/docker_proposal_pump.rs`:
  - `started_event_produces_proposal_within_five_seconds` — assert wall-clock latency under 5 s in a controlled fixture.
  - `stopped_event_produces_remove_route_proposal`.
  - `label_parse_error_writes_mutation_rejected_audit`.

### Acceptance command

```
cargo test -p trilithon-core docker::proposal_generator::tests && \
cargo test -p trilithon-adapters --test docker_proposal_pump
```

### Exit conditions

- All ten tests pass.
- The 5-second SLO holds in the controlled fixture.

### Audit kinds emitted

Per §6.6: `mutation.proposed`, `mutation.rejected`.

### Tracing events emitted

Per §12.1: `docker.event.received`, `proposal.received`.

### Cross-references

- ADR-0007.
- Phase 21 tasks: "Generate proposals with source `docker-discovery`," "Labelled container start produces a proposal within 5 seconds."

---

## Slice 21.5 [cross-cutting] — Hostname-collision conflict detector

### Goal

When two containers claim the same hostname, produce a single conflict proposal listing both candidates rather than two competing proposals.

### Entry conditions

- Slice 21.4 shipped.

### Files to create or modify

- `core/crates/core/src/docker/conflict_detector.rs`.
- `core/crates/adapters/src/docker_proposal_pump.rs` — wire conflict detection into the pump.

### Signatures and shapes

```rust
//! `core/crates/core/src/docker/conflict_detector.rs`

use crate::docker::ContainerId;
use crate::mutation::TypedMutation;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostnameClaim {
    pub container_id: ContainerId,
    pub host:         String,
    pub mutation:     TypedMutation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostnameConflict {
    pub host:       String,
    pub candidates: Vec<HostnameClaim>,
}

/// Group the active claims by hostname; return one conflict per hostname
/// where multiple containers claim it.
pub fn detect_conflicts(
    claims: &[HostnameClaim],
) -> Vec<HostnameConflict>;
```

### Algorithm

`detect_conflicts`:

1. Build a `BTreeMap<String, Vec<HostnameClaim>>` keyed on `host`.
2. For each entry where `claims.len() > 1`: emit a `HostnameConflict { host, candidates: claims }`.
3. Return the vector sorted by `host`.

Pump integration:

1. The pump maintains an in-memory map `host -> HostnameClaim` of the most recent claim per container.
2. On every event, recompute conflicts.
3. For each `HostnameConflict`:
   - Insert a single proposal with `source = docker-discovery`, `source_identifier = format!("conflict:{host}")`, and a `mutation` payload carrying both candidate intents (encoded via a typed `MutationOption::ConflictAmongCandidates(...)` variant; this slice extends the proposal's `mutation_json` to optionally hold a conflict envelope, OR a separate proposal-payload field captures the conflict — the simpler path: a dedicated `proposal.intent_kind` column on `proposals` distinguishing `single` from `conflict`. This slice extends the schema).
   - Trilithon MUST NOT insert two competing proposals for the same hostname.

### Tests

- `core/crates/core/src/docker/conflict_detector.rs` `mod tests`:
  - `single_claim_yields_no_conflict`.
  - `two_claims_same_host_yield_one_conflict_with_two_candidates`.
  - `three_claims_two_hosts_yield_one_conflict`.
  - `claims_sorted_by_host`.
- `core/crates/adapters/tests/docker_hostname_conflict.rs`:
  - `two_containers_same_host_yield_single_conflict_proposal`.
  - `proposal_table_contains_one_row_for_the_pair`.

### Acceptance command

```
cargo test -p trilithon-core docker::conflict_detector::tests && \
cargo test -p trilithon-adapters --test docker_hostname_conflict
```

### Exit conditions

- All six tests pass.
- The proposal table contains exactly one row per hostname conflict.

### Audit kinds emitted

Per §6.6: `mutation.proposed`.

### Tracing events emitted

Per §12.1: `proposal.received`.

### Cross-references

- ADR-0007.
- Phase 21 task: "Conflict detector for hostname collisions."

---

## Slice 21.6 [cross-cutting] — Wildcard-match security warning

### Goal

At proposal-render time, check whether the proposed host matches an existing wildcard certificate. If so, attach a typed `LossyWarning::WildcardMatchSecurity` warning to the proposal. The web UI requires explicit acknowledgement before approval; the acknowledgement is recorded in the audit log.

### Entry conditions

- Slice 21.4 shipped.
- The Phase 14 certificate inventory exposes `list_wildcard_certificates() -> Vec<WildcardCert>`.

### Files to create or modify

- `core/crates/core/src/docker/wildcard_callout.rs`.
- `core/crates/adapters/src/docker_proposal_pump.rs` — call the callout before insert.
- `core/crates/cli/src/http/proposals.rs` — extend approve handler to require an `acknowledged_wildcard: bool` field when the proposal carries the warning.
- `core/crates/core/src/audit.rs` — add `AuditEvent::ProposalWildcardAcknowledged` if not present (this is the §6.6 path: the table already includes `proposal.approved`; the wildcard acknowledgement is recorded in the approval row's `notes` field, not a new kind).

### Signatures and shapes

```rust
//! `core/crates/core/src/docker/wildcard_callout.rs`

use crate::policy::LossyWarning;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WildcardCert {
    pub certificate_id: String,
    pub pattern:        String,        // "*.example.com"
}

pub fn check_wildcard_match(
    host:      &str,
    wildcards: &[WildcardCert],
) -> Option<LossyWarning>;
```

### Algorithm

`check_wildcard_match`:

1. For each `wildcards` entry, convert `pattern` into a literal-prefix matcher:
   - `*.example.com` matches any host of the form `<label>.example.com` where `<label>` is one DNS label (no dots).
2. If `host` matches any wildcard: return `Some(LossyWarning::WildcardMatchSecurity { host: host.to_string(), certificate_id })`.
3. Else `None`.

Pump integration:

1. Before inserting a proposal whose mutation introduces a new host, call `check_wildcard_match`.
2. If `Some(warning)`: store it on the proposal's `notes` JSON as `wildcard_warning: { certificate_id, pattern }`.

Approval path:

1. The approval handler reads the proposal's `wildcard_warning` field. If present, the request body MUST carry `acknowledged_wildcard: true`. Otherwise return `(400, { kind: "wildcard-acknowledgement-required" })`.
2. On approval with acknowledgement: write the `proposal.approved` audit row with `notes.wildcard_acknowledged = true` and the matching `certificate_id`.

### Tests

- `core/crates/core/src/docker/wildcard_callout.rs` `mod tests`:
  - `match_single_label_subdomain_returns_warning`.
  - `match_multi_label_subdomain_does_not_match_single_label_wildcard`.
  - `match_apex_does_not_match`.
  - `multiple_wildcards_returns_first_match`.
- `core/crates/adapters/tests/docker_wildcard_callout.rs`:
  - `proposal_for_host_under_wildcard_carries_warning`.
  - `approval_without_acknowledgement_returns_400`.
  - `approval_with_acknowledgement_writes_audit_with_acknowledged_true`.

### Acceptance command

```
cargo test -p trilithon-core docker::wildcard_callout::tests && \
cargo test -p trilithon-adapters --test docker_wildcard_callout
```

### Exit conditions

- All seven tests pass.
- The approval body acknowledgement gate is enforced.

### Audit kinds emitted

Per §6.6: `proposal.approved` with `notes.wildcard_acknowledged = true`. No new audit kind.

### Tracing events emitted

Per §12.1: `proposal.approved`.

### Cross-references

- Hazard H3.
- Phase 21 tasks: "Implement the `WildcardMatchSecurity` warning," "Require explicit acknowledgement for wildcard proposals in UI."

---

## Slice 21.7 [cross-cutting] — Trust-grant first-run warning and `GET /api/v1/docker/status`

### Goal

On first daemon startup whose configuration mounts the Docker socket, print a stark warning to stderr and write one `docker.socket-trust-grant` audit row. The warning appears once per data directory. Implement `GET /api/v1/docker/status` returning connected, disconnected, or last-error.

### Entry conditions

- Slice 21.2 shipped.
- A `runtime_state` table or KV file exists for "first-run flags" (a `kv` table with key `docker.socket-trust-grant.acknowledged_at`).

### Files to create or modify

- `core/crates/cli/src/startup.rs` — first-run check and warning emission.
- `core/crates/cli/src/http/docker.rs` — status endpoint.
- `core/crates/cli/src/http/router.rs` — mount endpoint.
- `core/crates/adapters/src/kv_store.rs` — generic KV adapter (if not present).

### Signatures and shapes

```rust
//! `core/crates/cli/src/startup.rs` addition

pub fn print_docker_trust_grant_if_first_run(
    config: &trilithon_core::config::Config,
    kv:     &crate::adapters::kv_store::KvStore,
    audit:  &crate::adapters::audit_log_store::AuditLogStore,
    out:    &mut dyn std::io::Write,
) -> Result<(), StartupError>;
```

```rust
//! `core/crates/cli/src/http/docker.rs`

use axum::{Json, http::StatusCode};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum DockerStatus {
    Connected    { socket_path: String, since_unix_seconds: i64 },
    Disconnected { socket_path: String, retrying:           bool },
    LastError    { socket_path: String, detail: String, at_unix_seconds: i64 },
}

pub async fn get_status(
    state: axum::extract::State<crate::AppState>,
) -> (StatusCode, Json<DockerStatus>);
```

The warning text (verbatim, printed to stderr):

```
=================================================================
WARNING: Trilithon is configured with Docker socket access.

Mounting the Docker socket grants Trilithon effective root on this
host. Anything that compromises Trilithon can use the socket to
create privileged containers and escape isolation.

This warning is shown once per data directory. The grant is
recorded in the audit log under kind `docker.socket-trust-grant`.

Press Ctrl-C now to abort, or wait 10 seconds to continue.
=================================================================
```

### Algorithm

`print_docker_trust_grant_if_first_run`:

1. If `config.docker.enabled == false`: return early.
2. Read `kv.get("docker.socket-trust-grant.acknowledged_at")`. If `Some(_)`: return early (the warning has been shown already).
3. Write the warning text to `out`.
4. Pause for 10 seconds. On Ctrl-C during the pause, exit cleanly with the existing operator-abort exit code.
5. `audit.append(AuditEvent::DockerSocketTrustGrant, notes: { socket_path: config.docker.socket_path })`.
6. `kv.set("docker.socket-trust-grant.acknowledged_at", clock_now)`.

`get_status`:

1. Read shared state (an `Arc<RwLock<DockerStatusState>>` updated by the watcher supervisor on each connect/disconnect).
2. Return the current status.

### Tests

- `core/crates/cli/tests/docker_first_run.rs`:
  - `first_run_prints_warning_and_writes_audit_and_kv`.
  - `second_run_does_not_print_warning_and_does_not_write_audit`.
  - `disabled_docker_does_not_print_warning`.
  - `ctrl_c_during_pause_exits_with_operator_abort_code`.
- `core/crates/cli/tests/docker_status_endpoint.rs`:
  - `status_returns_connected_when_watcher_is_up`.
  - `status_returns_disconnected_with_retrying_true_during_backoff`.
  - `status_returns_last_error_after_failure`.

### Acceptance command

```
cargo test -p trilithon-cli --test docker_first_run --test docker_status_endpoint
```

### Exit conditions

- All seven tests pass.
- The warning appears on first run only; subsequent starts are silent.

### Audit kinds emitted

Per §6.6: `docker.socket-trust-grant`.

### Tracing events emitted

Per §12.1: `daemon.started`, `daemon.shutting-down`, `http.request.received`, `http.request.completed`.

### Cross-references

- Hazard H11.
- ADR-0007, ADR-0010.
- Phase 21 tasks: "Print the H11 trust-grant warning at first run per data directory," "Implement `GET /api/v1/docker/status`."

---

## Slice 21.8 [standard] — Podman compatibility

### Goal

Allow Trilithon to point at a Podman-provided Docker-compatible socket. The adapter MUST honour `XDG_RUNTIME_DIR/podman/podman.sock` when configured; the configuration knob `[docker] socket_path` accepts any absolute path.

### Entry conditions

- Slice 21.1 shipped.

### Files to create or modify

- `core/crates/adapters/src/docker_bollard.rs` — already accepts an arbitrary socket path; this slice adds documentation and a fallback discovery routine.
- `core/crates/core/src/config.rs` — extend the validator to verify the path exists and is a Unix socket.

### Signatures and shapes

```rust
//! Addition to `core/crates/adapters/src/docker_bollard.rs`

pub fn discover_default_socket() -> Option<std::path::PathBuf>;
// Tries (in order):
//   1. /var/run/docker.sock
//   2. $XDG_RUNTIME_DIR/podman/podman.sock
//   3. /run/user/<uid>/podman/podman.sock
```

### Algorithm

`discover_default_socket`:

1. For each candidate path, `tokio::fs::metadata(path).await` and check `file_type().is_socket()`.
2. Return the first hit, or `None` if none exist.

Configuration:

- If `[docker] socket_path` is unset and `[docker] enabled = true`, the daemon calls `discover_default_socket()`.
- If discovery returns `None` and `enabled = true`, the daemon refuses to start with `ConfigError::DockerSocketNotFound`.

### Tests

- `core/crates/adapters/tests/docker_socket_discovery.rs`:
  - `discover_returns_docker_socket_when_present`.
  - `discover_falls_back_to_podman_xdg_runtime`.
  - `discover_returns_none_when_no_socket_present`.
- `core/crates/cli/tests/docker_podman_socket.rs`:
  - `daemon_starts_with_explicit_podman_socket_path`.

### Acceptance command

```
cargo test -p trilithon-adapters --test docker_socket_discovery && \
cargo test -p trilithon-cli --test docker_podman_socket
```

### Exit conditions

- All four tests pass.
- The Podman path works without code changes — only configuration.

### Audit kinds emitted

None new. The existing `docker.socket-trust-grant` audit row carries the resolved socket path.

### Tracing events emitted

None new.

### Cross-references

- Phase 21 task: the watcher MUST honour podman's Docker-compatible socket where present.

---

## Slice 21.9 [standard] — Web UI badge and proposal-row Docker metadata

### Goal

Render the Docker discovery status badge on the dashboard with the H11 warning embedded. Render Docker-sourced proposals in the proposal queue with container metadata. The wildcard banner requires acknowledgement before the Approve button enables.

### Entry conditions

- Slice 21.7 shipped.
- Phase 20 slice 20.6 (`ProposalsPage`) shipped.

### Files to create or modify

- `web/src/features/docker/types.ts`.
- `web/src/features/docker/DockerStatusBadge.tsx` and `.test.tsx`.
- `web/src/features/docker/useDockerStatus.ts`.
- `web/src/features/proposals/ProposalRow.tsx` — extend with container metadata and wildcard acknowledgement gate.
- `web/src/features/proposals/WildcardBanner.tsx` and `.test.tsx`.

### Signatures and shapes

```typescript
// web/src/features/docker/types.ts

export type DockerStatus =
  | { kind: 'connected'; socket_path: string; since_unix_seconds: number }
  | { kind: 'disconnected'; socket_path: string; retrying: boolean }
  | { kind: 'last-error'; socket_path: string; detail: string; at_unix_seconds: number };

// web/src/features/docker/DockerStatusBadge.tsx
export function DockerStatusBadge(): JSX.Element;

// web/src/features/proposals/WildcardBanner.tsx
export function WildcardBanner(props: {
  host: string;
  certificate_id: string;
  acknowledged: boolean;
  onAcknowledge: () => void;
}): JSX.Element;
```

### Algorithm

`DockerStatusBadge`:

1. `useQuery` `GET /api/v1/docker/status` with a 10-second polling interval.
2. Render a badge: green for `connected`, yellow for `disconnected` with `retrying`, red for `last-error`.
3. Embed the literal H11 warning text in a tooltip and inside an info popover.

`ProposalRow` extension:

1. When `proposal.source == 'docker-discovery'`, render a metadata strip: `Container: <source_identifier[..12]>...`, `Host: <mutation.host>`.
2. When `proposal.notes.wildcard_warning` is present, render `WildcardBanner` above the Approve button; the Approve button is disabled until `acknowledged === true`.
3. On approve, send `acknowledged_wildcard: true` in the request body.

### Tests

- `web/src/features/docker/DockerStatusBadge.test.tsx`:
  - `renders_green_when_connected`.
  - `renders_yellow_when_disconnected_with_retrying_true`.
  - `renders_red_when_last_error`.
  - `tooltip_contains_h11_warning_text`.
- `web/src/features/proposals/ProposalRow.test.tsx` (extend):
  - `docker_sourced_row_renders_container_id_and_host`.
  - `wildcard_warning_disables_approve_until_acknowledged`.
  - `acknowledgement_sends_acknowledged_wildcard_true_in_approve_body`.

### Acceptance command

```
cd web && pnpm typecheck && pnpm lint && pnpm test --run
```

### Exit conditions

- All seven Vitest tests pass.
- The badge is visible on the dashboard.
- Wildcard acknowledgement is required before approval.

### Audit kinds emitted

None directly from the web tier.

### Tracing events emitted

None directly.

### Cross-references

- Hazard H11, H3.
- Phase 21 tasks: "Surface the Docker discovery status badge on the dashboard," "Render Docker-sourced proposals with container metadata."

---

## Phase exit checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] A container with valid Caddy labels produces a proposal within 5 seconds of starting.
- [ ] A container destruction produces a "remove route" proposal.
- [ ] A label conflict produces a single conflict proposal listing both candidates.
- [ ] Wildcard-certificate matches are highlighted with a security callout requiring explicit acknowledgement, satisfying T2.11.
- [ ] The daemon's first-run output displays the Docker socket trust warning, satisfying H11.

## Open questions

- The label-conflict representation extends the `proposals` table either with a typed `intent_kind` column or via a special-shape `mutation_json` envelope. The phase reference does not pin one. Slice 21.5 picks the `intent_kind` column path; if the project prefers the envelope path, the planner should flag the divergence before slice 21.5 lands.
