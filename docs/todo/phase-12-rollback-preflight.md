# Phase 12 — Snapshot history and rollback with preflight — Implementation Slices

> Phase reference: [../phases/phase-12-rollback-preflight.md](../phases/phase-12-rollback-preflight.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference [phase-12-rollback-preflight.md](../phases/phase-12-rollback-preflight.md).
- Architecture sections: §4.1 (`core` crate responsibilities), §4.2 (`adapters` crate), §6.5 (`snapshots`), §6.6 (`audit_log` and the V1 `kind` vocabulary), §7.1 (mutation lifecycle), §7.6 (rollback with preflight), §10 (failure model), §12.1 (tracing vocabulary), §13 (performance budget).
- Trait signatures: §1 `core::storage::Storage`, §2 `core::caddy::CaddyClient`, §6 `core::reconciler::Applier`, §8 `core::probe::ProbeAdapter`.
- ADRs: ADR-0009 (immutable content-addressed snapshots and audit log), ADR-0012 (optimistic concurrency on monotonic `config_version`), ADR-0013 (capability probe gates optional Caddy features), ADR-0015 (instance ownership sentinel).
- PRD T-numbers: T1.3 (one-click rollback with preflight), T1.7 (audit log), T1.9 (TLS visibility, substrate), T1.10 (basic upstream health, substrate), T1.11 (capability probe).
- Hazards: H2 (stale-upstream rollback), H9 (Caddy version skew across snapshots).

## Slice plan summary

| # | Slice title | Primary files | Effort (h) | Depends on |
|---|---|---|---|---|
| 12.1 | `RollbackRequest` mutation type and reachability check | `core/crates/core/src/mutation.rs`, `core/crates/core/src/snapshot/reachable.rs` | 4 | Phase 5, Phase 11 |
| 12.2 | `Preflight` engine and condition algebra in `core` | `core/crates/core/src/preflight/mod.rs`, `core/crates/core/src/preflight/conditions.rs` | 8 | 12.1 |
| 12.3 | TCP reachability and TLS validity probes in `adapters` | `core/crates/adapters/src/probe_tokio.rs`, `core/crates/adapters/src/tls_inventory_probe.rs` | 6 | 12.2 |
| 12.4 | `module-available` condition wired to capability cache | `core/crates/core/src/preflight/module.rs` | 3 | 12.2 |
| 12.5 | HTTP endpoints `POST /api/v1/snapshots/{id}/preflight` and `/rollback` with override surface | `core/crates/cli/src/http/snapshots.rs`, `core/crates/core/src/preflight/override.rs` | 6 | 12.3, 12.4 |
| 12.6 | Audit row authoring for rollback request, overrides, and apply outcome | `core/crates/core/src/audit.rs`, `core/crates/adapters/src/audit_log_store.rs` | 3 | 12.5 |
| 12.7 | Web UI snapshot history tab and rollback dialog with override toggles | `web/src/features/routes/HistoryTab.tsx`, `web/src/features/rollback/RollbackDialog.tsx` | 8 | 12.5 |

After every slice: `cargo build --workspace` succeeds; `pnpm typecheck` succeeds where the slice touches the web; the slice's named tests pass.

---

## Slice 12.1 [standard] — `RollbackRequest` mutation type and snapshot reachability check

### Goal

Introduce the typed `RollbackRequest` mutation in the `core` crate. The mutation MUST identify a target snapshot and MUST carry a pre-condition that the target snapshot exists in the store and is reachable from the current desired-state snapshot through the parent chain. The slice ships pure-core types and a reachability function with full unit-test coverage; no I/O, no audit emission yet.

### Entry conditions

- Phase 11 is complete: route create, read, update, delete are exercised end-to-end and the desired-state aggregate is integration-tested.
- The `core::snapshot` module exposes `Snapshot { id: SnapshotId, parent_id: Option<SnapshotId>, ... }` and `core::storage::Storage::parent_chain` is implemented per trait-signatures.md §1.
- The `core::mutation::TypedMutation` enum exists from Phase 4 and is open for new variants.

### Files to create or modify

- `core/crates/core/src/mutation.rs` — add the `RollbackRequest` variant to the `TypedMutation` enum.
- `core/crates/core/src/snapshot/reachable.rs` — new module hosting `is_reachable` and `RollbackPreconditionError`.
- `core/crates/core/src/snapshot/mod.rs` — re-export the new module.
- `core/crates/core/src/lib.rs` — register the `snapshot::reachable` submodule if not already exposed.

### Signatures and shapes

```rust
// core/crates/core/src/mutation.rs (additions)
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TypedMutation {
    // ... existing variants ...
    RollbackRequest(RollbackRequest),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RollbackRequest {
    pub target: SnapshotId,
    pub expected_version: i64,
    pub intent: String,
}
```

```rust
// core/crates/core/src/snapshot/reachable.rs
use crate::snapshot::{Snapshot, SnapshotId};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RollbackPreconditionError {
    #[error("target snapshot {target} not found in store")]
    TargetMissing { target: SnapshotId },
    #[error("target snapshot {target} is not reachable from current head {head} within {max_depth} ancestors")]
    NotReachable { target: SnapshotId, head: SnapshotId, max_depth: usize },
    #[error("target snapshot {target} is the current head; rollback is a no-op")]
    TargetIsHead { target: SnapshotId },
}

/// Return `Ok(distance)` where `distance` is the number of parent hops from
/// `head` to `target`. Returns `Err(NotReachable)` if `target` does not appear
/// in `head`'s ancestor chain within `max_depth`.
///
/// `chain` MUST be the result of `Storage::parent_chain(head, max_depth)` ordered
/// oldest-first per trait-signatures.md §1.
pub fn is_reachable(
    head: &SnapshotId,
    target: &SnapshotId,
    chain: &[SnapshotId],
    max_depth: usize,
) -> Result<usize, RollbackPreconditionError>;
```

### Algorithm

1. If `target == head`, return `Err(TargetIsHead)`.
2. Walk `chain` newest-first (reverse iteration since the trait returns oldest-first).
3. For each `ancestor` at index `i`, if `ancestor == target`, return `Ok(chain.len() - 1 - i)`.
4. If the loop completes without a match, return `Err(NotReachable { target, head, max_depth })`.
5. Caller is responsible for surfacing `Err(TargetMissing)` when `Storage::get_snapshot` returns `None`; `is_reachable` does not perform I/O.

### Tests

Unit tests inline in `core/crates/core/src/snapshot/reachable.rs` under `#[cfg(test)] mod tests`.

- `reachable_target_in_chain_returns_distance` — chain of three ancestors, target at index 1; assert `Ok(1)`.
- `reachable_target_equals_head_returns_target_is_head` — head and target identical; assert `Err(TargetIsHead)`.
- `reachable_target_absent_returns_not_reachable` — chain does not contain target; assert `Err(NotReachable)` carries supplied `max_depth`.
- `reachable_genesis_target_distance_is_chain_length_minus_one` — target is the oldest entry; assert `Ok(chain.len() - 1)`.

Mutation-enum tests inline in `core/crates/core/src/mutation.rs`:

- `rollback_request_serde_round_trip` — serialise and deserialise a `TypedMutation::RollbackRequest` value; assert byte equality after canonical re-serialisation.

### Acceptance command

`cargo test -p trilithon-core snapshot::reachable::tests`

### Exit conditions

- `cargo build -p trilithon-core` succeeds.
- All four reachability tests pass.
- The `RollbackRequest` variant is part of the `TypedMutation` enum and round-trips through `serde_json`.
- No audit row is written by this slice.

### Audit kinds emitted

None. This slice introduces only types and pure logic. Audit emission lands in slice 12.6.

### Tracing events emitted

None.

### Cross-references

- ADR-0009 (snapshot immutability and parent linkage).
- ADR-0012 (`expected_version` discipline).
- PRD T1.3.
- Architecture §6.5 (`snapshots` row shape), §7.6 step 1.
- Trait signatures §1 (`Storage::parent_chain`).

---

## Slice 12.2 [cross-cutting] — `Preflight` engine and condition algebra in `core`

### Goal

Introduce the pure-core `Preflight` engine. The engine produces a typed `PreflightReport` consisting of one `ConditionOutcome` per condition. Each outcome carries a stable identifier, status (`pass`, `fail`, `warn`), human-readable message, and optional structured data. The engine is composed from a `Vec<Box<dyn Condition>>`; condition implementations land in slices 12.3 and 12.4. This slice ships the `Condition` trait, the report types, and an in-memory test harness.

### Entry conditions

- Slice 12.1 complete.
- `core::snapshot::Snapshot` is available and serialisable.

### Files to create or modify

- `core/crates/core/src/preflight/mod.rs` — engine orchestration and re-exports.
- `core/crates/core/src/preflight/report.rs` — `PreflightReport`, `ConditionOutcome`, `ConditionStatus`, `ConditionId`.
- `core/crates/core/src/preflight/conditions.rs` — `Condition` trait and the reusable `ConditionContext` struct.
- `core/crates/core/src/lib.rs` — register `pub mod preflight`.

### Signatures and shapes

```rust
// core/crates/core/src/preflight/report.rs
use crate::snapshot::Snapshot;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ConditionId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConditionStatus { Pass, Fail, Warn }

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConditionOutcome {
    pub id: ConditionId,
    pub status: ConditionStatus,
    pub message: String,
    pub overridable: bool,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PreflightReport {
    pub target: crate::snapshot::SnapshotId,
    pub outcomes: Vec<ConditionOutcome>,
}

impl PreflightReport {
    pub fn is_blocking(&self) -> bool {
        self.outcomes.iter().any(|o| o.status == ConditionStatus::Fail)
    }
    pub fn failing_ids(&self) -> Vec<&ConditionId> {
        self.outcomes
            .iter()
            .filter(|o| o.status == ConditionStatus::Fail)
            .map(|o| &o.id)
            .collect()
    }
}
```

```rust
// core/crates/core/src/preflight/conditions.rs
use async_trait::async_trait;
use crate::preflight::report::ConditionOutcome;
use crate::snapshot::Snapshot;

#[async_trait]
pub trait Condition: Send + Sync + 'static {
    /// Stable kebab-case identifier matching the §6.6 override audit notes.
    fn id(&self) -> &'static str;

    /// Evaluate the condition against the target snapshot. The implementation
    /// MUST NOT mutate state; the engine is invoked from read-only call sites.
    async fn evaluate(&self, target: &Snapshot) -> ConditionOutcome;
}
```

```rust
// core/crates/core/src/preflight/mod.rs
pub mod report;
pub mod conditions;

use crate::snapshot::Snapshot;
use report::PreflightReport;
use conditions::Condition;

pub struct PreflightEngine {
    conditions: Vec<Box<dyn Condition>>,
}

impl PreflightEngine {
    pub fn new(conditions: Vec<Box<dyn Condition>>) -> Self { Self { conditions } }

    pub async fn run(&self, target: &Snapshot) -> PreflightReport {
        let mut outcomes = Vec::with_capacity(self.conditions.len());
        for c in &self.conditions {
            outcomes.push(c.evaluate(target).await);
        }
        PreflightReport { target: target.id.clone(), outcomes }
    }
}
```

### Algorithm

Engine evaluation is sequential by design (the condition set is small; concurrency is not a performance requirement at three conditions). Sequential evaluation also yields deterministic outcome order, which matters for golden-test stability.

1. Construct `PreflightEngine` with the desired condition vector.
2. `run(target)` iterates conditions in registration order.
3. For each condition, await `evaluate(target)` and push the resulting `ConditionOutcome`.
4. Build and return a `PreflightReport` with the target snapshot id and the outcome list.

### Tests

Unit tests inline in `core/crates/core/src/preflight/mod.rs`:

- `engine_runs_every_condition_in_order` — register three test doubles each returning `Pass` with distinct ids; assert the outcome list matches registration order.
- `report_is_blocking_iff_any_failure` — fixture report containing one `Fail` and two `Pass`; assert `is_blocking()` is `true`.
- `report_failing_ids_returns_only_failures` — mixed fixture; assert the ids match.

A test double `RecordingCondition` lives at `core/crates/core/src/preflight/test_doubles.rs` (gated `#[cfg(test)]`). The double records `evaluate` calls into a shared `Vec<ConditionId>`.

### Acceptance command

`cargo test -p trilithon-core preflight::tests`

### Exit conditions

- `PreflightEngine::run` is exercised by passing tests against in-memory doubles.
- No I/O is performed by any item in `core::preflight`.
- `serde` round-trip on `PreflightReport` is asserted.

### Audit kinds emitted

None.

### Tracing events emitted

None. The HTTP slice (12.5) opens and closes the spans that wrap engine invocation.

### Cross-references

- PRD T1.3.
- Architecture §7.6 step 2.
- Trait signatures §8 (`ProbeAdapter`) — consumed by slice 12.3.

---

## Slice 12.3 [cross-cutting] — TCP reachability and TLS validity probes in `adapters`

### Goal

Implement the `upstream-tcp-reachable` and `tls-issuance-valid` `Condition` implementations. Both are wired through `core::probe::ProbeAdapter` (trait-signatures.md §8). The TCP probe carries a 2-second default timeout and a typed result. The TLS probe verifies current certificate validity for managed hosts using `CaddyClient::get_certificates` and a `ProbeAdapter::tls_handshake` cross-check.

### Entry conditions

- Slice 12.2 complete.
- `core::probe::ProbeAdapter` exists with the methods listed in trait-signatures.md §8.
- `core::caddy::CaddyClient::get_certificates` is implemented in `adapters` and returns `Vec<TlsCertificate>`.

### Files to create or modify

- `core/crates/core/src/preflight/upstream_tcp.rs` — `UpstreamTcpReachableCondition` (lives in `core` because the condition-evaluation logic is pure; it accepts an `Arc<dyn ProbeAdapter>` injected from `cli`).
- `core/crates/core/src/preflight/tls_issuance.rs` — `TlsIssuanceValidCondition`, parameterised by an `Arc<dyn CaddyClient>` and an `Arc<dyn ProbeAdapter>`.
- `core/crates/adapters/src/probe_tokio.rs` — extend the existing `TokioProbeAdapter` with a 2-second default `tcp_reachable` timeout.
- `core/crates/adapters/tests/preflight_probes_smoke.rs` — integration tests against a loopback TCP listener and a self-signed TLS handshake.

### Signatures and shapes

```rust
// core/crates/core/src/preflight/upstream_tcp.rs
use std::sync::Arc;
use async_trait::async_trait;
use crate::preflight::conditions::Condition;
use crate::preflight::report::{ConditionId, ConditionOutcome, ConditionStatus};
use crate::probe::ProbeAdapter;
use crate::snapshot::Snapshot;

pub struct UpstreamTcpReachableCondition {
    pub probe: Arc<dyn ProbeAdapter>,
}

#[async_trait]
impl Condition for UpstreamTcpReachableCondition {
    fn id(&self) -> &'static str { "upstream-tcp-reachable" }

    async fn evaluate(&self, target: &Snapshot) -> ConditionOutcome {
        // See Algorithm.
    }
}
```

```rust
// core/crates/core/src/preflight/tls_issuance.rs
use std::sync::Arc;
use async_trait::async_trait;
use crate::caddy::client::CaddyClient;
use crate::preflight::conditions::Condition;
use crate::preflight::report::{ConditionId, ConditionOutcome, ConditionStatus};
use crate::snapshot::Snapshot;

pub struct TlsIssuanceValidCondition {
    pub caddy: Arc<dyn CaddyClient>,
}

#[async_trait]
impl Condition for TlsIssuanceValidCondition {
    fn id(&self) -> &'static str { "tls-issuance-valid" }

    async fn evaluate(&self, target: &Snapshot) -> ConditionOutcome { /* see Algorithm */ }
}
```

### Algorithm — `UpstreamTcpReachableCondition::evaluate`

1. Extract every `Upstream` referenced by `target.desired_state` into a deduplicated `Vec<UpstreamDestination>`.
2. For each destination, call `self.probe.tcp_reachable(&destination).await`.
3. Collect the failures into a `Vec<UnreachableDestination { upstream, host, port }>`.
4. If `failures.is_empty()`, return `ConditionOutcome { status: Pass, message: "all upstreams reachable", overridable: false, details: { "checked": <n> } }`.
5. Otherwise, return `ConditionOutcome { status: Fail, message: format!("{n} upstream(s) unreachable"), overridable: true, details: { "failures": failures } }`.

### Algorithm — `TlsIssuanceValidCondition::evaluate`

1. Fetch the certificate inventory via `self.caddy.get_certificates().await`. On error, return `ConditionOutcome { status: Warn, overridable: true, message: "TLS inventory fetch failed; proceeding without verification", details: { "error": <detail> } }`.
2. Extract every host requiring TLS from `target.desired_state` (skip hosts marked TLS internal or explicit-cert-and-key without managed issuance).
3. For each host, find the matching certificate by SAN; if missing, record `MissingCertificate { host }`.
4. For each found certificate, compare `not_after` with the current Unix seconds; if `not_after <= now`, record `Expired { host, not_after }`.
5. If both lists are empty, return `Pass`. Otherwise, return `Fail` with `overridable: true` and structured details.

### Tests

Integration tests at `core/crates/adapters/tests/preflight_probes_smoke.rs`:

- `tcp_probe_happy_path_against_loopback_listener` — bind a `tokio::net::TcpListener` on `127.0.0.1:0`, run the condition; assert `Pass`.
- `tcp_probe_connection_refused_returns_fail` — point at a closed loopback port; assert `Fail` with details listing the destination.
- `tcp_probe_timeout_returns_fail_within_2500_ms` — point at a non-routable address (`192.0.2.1:80`, RFC 5737 TEST-NET-1); assert `Fail` and that the wall-clock duration is between 2 seconds and 2500 milliseconds.
- `tls_probe_valid_certificate_returns_pass` — Caddy double returns a single not-yet-expired certificate; assert `Pass`.
- `tls_probe_expired_certificate_returns_fail` — Caddy double returns one with `not_after = now - 1`; assert `Fail` with `overridable: true`.
- `tls_probe_missing_host_returns_fail` — desired state references a host the inventory does not cover; assert `Fail`.

### Acceptance command

`cargo test -p trilithon-adapters --test preflight_probes_smoke`

### Exit conditions

- All six smoke tests pass.
- The 2-second TCP timeout is enforced.
- Both conditions are constructible with `Arc<dyn ProbeAdapter>` and `Arc<dyn CaddyClient>` doubles per trait-signatures.md §1's "Test doubles" convention.

### Audit kinds emitted

None.

### Tracing events emitted

None directly; spans are opened by the HTTP wrapper in slice 12.5.

### Cross-references

- PRD T1.3 (substrate), T1.9 (TLS inventory cross-reference), T1.10 (upstream health cross-reference).
- Hazards H2 (stale-upstream rollback).
- Trait signatures §2 (`CaddyClient::get_certificates`), §8 (`ProbeAdapter`).

---

## Slice 12.4 [standard] — `module-available` condition wired to capability cache

### Goal

Implement `ModuleAvailableCondition`. The condition consumes the current `CapabilitySet` (recorded in `capability_probe_results.is_current = 1`, architecture §6.13). For each Caddy module referenced by `target.desired_state`, the condition asserts the module is present in the capability set. Missing modules produce a `Fail` with `overridable: false` because applying a snapshot that depends on a missing module is guaranteed to fail at apply (hazard H5).

### Entry conditions

- Slice 12.3 complete.
- The capability probe writes rows to `capability_probe_results` per Phase 3 / architecture §6.13.
- A `core::caddy::CapabilitySet` type exists with method `pub fn has_module(&self, name: &str) -> bool`.

### Files to create or modify

- `core/crates/core/src/preflight/module.rs` — new module.
- `core/crates/core/src/snapshot/referenced_modules.rs` — pure function `referenced_modules(state: &DesiredState) -> BTreeSet<String>`.

### Signatures and shapes

```rust
// core/crates/core/src/preflight/module.rs
use std::sync::Arc;
use async_trait::async_trait;
use crate::caddy::CapabilitySet;
use crate::preflight::conditions::Condition;
use crate::preflight::report::{ConditionOutcome, ConditionStatus};
use crate::snapshot::Snapshot;

pub struct ModuleAvailableCondition {
    pub capabilities: Arc<CapabilitySet>,
}

#[async_trait]
impl Condition for ModuleAvailableCondition {
    fn id(&self) -> &'static str { "module-available" }
    async fn evaluate(&self, target: &Snapshot) -> ConditionOutcome { /* see Algorithm */ }
}
```

```rust
// core/crates/core/src/snapshot/referenced_modules.rs
use std::collections::BTreeSet;
use crate::desired_state::DesiredState;

/// Walk the desired-state tree and collect every Caddy module identifier
/// referenced by any handler, matcher, or transport. Identifiers are returned
/// in canonical dotted form (for example `http.handlers.reverse_proxy`).
pub fn referenced_modules(state: &DesiredState) -> BTreeSet<String>;
```

### Algorithm — `evaluate`

1. Compute `BTreeSet<String> = referenced_modules(&target.desired_state)`.
2. For each module name, call `self.capabilities.has_module(name)`.
3. Collect missing names into `missing: Vec<String>`.
4. If `missing.is_empty()`, return `Pass` with `details = { "checked": <n> }`.
5. Otherwise return `Fail { overridable: false, details: { "missing": missing } }`. Per architecture §7.4 step 5 and hazard H5, the override is intentionally disallowed: applying a snapshot referencing an absent module would fail at Caddy validation regardless.

### Tests

Unit tests inline in `core/crates/core/src/preflight/module.rs`:

- `module_condition_pass_when_all_present` — capability set lists the referenced modules; assert `Pass`.
- `module_condition_fail_when_one_missing` — one module absent; assert `Fail` with `overridable: false`.
- `module_condition_fail_lists_every_missing_in_details` — three missing; assert details contains all three sorted.

Unit tests at `core/crates/core/src/snapshot/referenced_modules.rs`:

- `referenced_modules_walks_handlers_and_matchers` — fixture desired-state with a reverse-proxy handler and a header matcher; assert both appear.
- `referenced_modules_deduplicates` — two routes referencing `http.handlers.reverse_proxy`; assert the set has one entry.

### Acceptance command

`cargo test -p trilithon-core preflight::module::tests snapshot::referenced_modules::tests`

### Exit conditions

- All five tests pass.
- `Fail` outcome is non-overridable, satisfying H5 at the preflight boundary.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.11.
- ADR-0013.
- Hazards H5.
- Architecture §6.13 (`capability_probe_results`), §7.4 (capability probe).

---

## Slice 12.5 [cross-cutting] — HTTP endpoints `POST /api/v1/snapshots/{id}/preflight` and `/rollback`

### Goal

Expose preflight and rollback through the authenticated HTTP API introduced in Phase 9. `POST /api/v1/snapshots/{id}/preflight` returns the `PreflightReport`. `POST /api/v1/snapshots/{id}/rollback` accepts an optional `overrides: [condition_id]` field and an optional `override_reason: string` (bounded at 1024 characters), runs preflight, and either applies the snapshot through `Applier::rollback` or returns a structured 422 listing every failing condition. The rollback path emits the audit rows authored in slice 12.6.

### Entry conditions

- Slices 12.1 through 12.4 complete.
- `core::reconciler::Applier::rollback` is available per trait-signatures.md §6.
- The HTTP server framework from Phase 9 supports authenticated handlers and the `correlation_id` middleware.

### Files to create or modify

- `core/crates/cli/src/http/snapshots.rs` — new module hosting the two handlers.
- `core/crates/cli/src/http/mod.rs` — register the routes.
- `core/crates/core/src/preflight/override.rs` — `OverrideSet` type and validation helpers.

### Signatures and shapes

```rust
// core/crates/core/src/preflight/override.rs
use std::collections::BTreeSet;
use crate::preflight::report::{ConditionId, PreflightReport};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OverrideSet {
    pub overrides: BTreeSet<ConditionId>,
    pub reason: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OverrideError {
    #[error("override reason exceeds 1024 characters")]
    ReasonTooLong,
    #[error("override targets non-overridable condition {id}")]
    NotOverridable { id: ConditionId },
    #[error("override targets unknown condition {id} not in preflight report")]
    UnknownCondition { id: ConditionId },
    #[error("preflight has unresolved failures: {ids:?}")]
    UnresolvedFailures { ids: Vec<ConditionId> },
}

/// Validate that every supplied override targets an actually-failing,
/// actually-overridable condition in the report and that reason length is
/// within bounds.
pub fn validate_overrides(
    set: &OverrideSet,
    report: &PreflightReport,
) -> Result<(), OverrideError>;
```

```rust
// core/crates/cli/src/http/snapshots.rs (handler signatures)
pub async fn post_preflight(
    State(ctx): State<HttpContext>,
    Path(id):   Path<SnapshotId>,
    auth:       AuthenticatedActor,
) -> Result<Json<PreflightReport>, ApiError>;

#[derive(serde::Deserialize)]
pub struct RollbackBody {
    pub overrides: Option<Vec<ConditionId>>,
    pub override_reason: Option<String>,
    pub expected_version: i64,
}

#[derive(serde::Serialize)]
pub struct RollbackResponse {
    pub snapshot_id: SnapshotId,
    pub config_version: i64,
    pub applied_at: i64,
}

pub async fn post_rollback(
    State(ctx): State<HttpContext>,
    Path(id):   Path<SnapshotId>,
    auth:       AuthenticatedActor,
    Json(body): Json<RollbackBody>,
) -> Result<Json<RollbackResponse>, ApiError>;
```

### Algorithm — `post_preflight`

1. Authenticate session (middleware).
2. Open span `http.request.received` (architecture §12.1) with `http.method = "POST"`, `http.path = "/api/v1/snapshots/{id}/preflight"`, `correlation_id`, `actor.id`, `actor.kind`.
3. Fetch target snapshot via `Storage::get_snapshot(&id)`. If `None`, return `404` with `RollbackPreconditionError::TargetMissing`.
4. Fetch current head via `Storage::latest_desired_state()`. Verify reachability via `is_reachable`; surface `Err(NotReachable)` as `409`.
5. Run `PreflightEngine::run(&target)` to produce the report.
6. Emit span event `http.request.completed` with `http.status = 200`, `latency_ms`.
7. Return `200` with the report.

### Algorithm — `post_rollback`

1. Authenticate session.
2. Open the same `http.request.received` span as above.
3. Validate `body.expected_version` matches the current `config_version`. Mismatch returns `409` with the conflict envelope from Phase 9.
4. Fetch and reachability-check the target snapshot exactly as `post_preflight`.
5. Run `PreflightEngine::run(&target)`.
6. If `report.is_blocking()`:
   1. Build `OverrideSet { overrides: body.overrides.unwrap_or_default().into_iter().collect(), reason: body.override_reason.clone() }`.
   2. Call `validate_overrides(&set, &report)`. On `Err`, return `422` with the structured error.
   3. Compute `unresolved = report.failing_ids() \\ set.overrides`. If non-empty, return `422` with `OverrideError::UnresolvedFailures`.
7. Write audit rows per slice 12.6 (`mutation.submitted` for the rollback request, one row per accepted override, then `config.rolled-back` plus `config.applied` from the apply outcome).
8. Invoke `Applier::rollback(&target.id).await`.
9. On `Ok(outcome)`, return `200` with `RollbackResponse { snapshot_id, config_version, applied_at }`.
10. On `Err(ApplyError::OptimisticConflict)`, return `409` with the conflict envelope.
11. On any other `Err`, write a `config.apply-failed` audit row and return `502`.

### Tests

Integration tests at `core/crates/cli/tests/rollback_http.rs`:

- `preflight_endpoint_returns_report_for_reachable_target`.
- `preflight_endpoint_returns_404_for_missing_target`.
- `preflight_endpoint_returns_409_when_target_not_reachable`.
- `rollback_blocked_when_unresolved_failures` — fail without overrides; assert `422` and the failing condition list.
- `rollback_succeeds_with_full_override_set` — every failing condition is overridable and overridden; assert `200`, the apply outcome, and the audit rows from slice 12.6.
- `rollback_rejects_override_for_non_overridable_condition` — `module-available` failure with override attempted; assert `422` and `OverrideError::NotOverridable`.
- `rollback_rejects_reason_longer_than_1024_chars` — assert `422` with `ReasonTooLong`.
- `rollback_returns_409_on_optimistic_conflict`.

### Acceptance command

`cargo test -p trilithon-cli --test rollback_http`

### Exit conditions

- All eight integration tests pass against a real `axum` server bound to a random loopback port.
- The handlers participate in the `correlation_id` propagation middleware unchanged.
- The override-reason length bound is enforced at the API boundary, not only in the UI.

### Audit kinds emitted

This slice emits, via the audit writer wired in slice 12.6:

- `mutation.submitted` — one row when the rollback request enters the queue.
- `config.rolled-back` — one row when the apply succeeds.
- `config.applied` — one row from the standard apply path.
- `config.apply-failed` — one row on apply failure.
- `mutation.rejected` — one row on `422` due to unresolved failures or invalid overrides.

All five strings are quoted verbatim from architecture §6.6.

### Tracing events emitted

- `http.request.received` (architecture §12.1).
- `http.request.completed`.
- `apply.started`, `apply.succeeded`, `apply.failed` from the underlying `Applier::rollback`.

### Cross-references

- ADR-0009, ADR-0012.
- PRD T1.3.
- Architecture §7.6, §10 (failure model rows for Caddy unreachable, validation rejection), §12.1.
- Trait signatures §6 (`Applier::rollback`).

---

## Slice 12.6 [cross-cutting] — Audit row authoring for rollback request, overrides, and apply outcome

### Goal

Author the audit rows produced during a rollback. Each accepted override produces one row with `kind = mutation.rejected.missing-expected-version` is NOT used here; the override audit row reuses `kind = mutation.submitted` with structured `notes` capturing `condition_id`, `actor_id`, and `reason`. The successful apply produces a `config.rolled-back` row and a `config.applied` row. This slice ships the writer functions in `core::audit` and the call-site wiring inside the rollback handler from slice 12.5.

### Entry conditions

- Slice 12.5 complete.
- The Phase 6 audit writer (`adapters::audit_log_store::append`) accepts an `AuditEventRow` per trait-signatures.md §1.

### Files to create or modify

- `core/crates/core/src/audit.rs` — extend the `AuditEvent` enum with `RollbackRequested`, `RollbackOverrideAccepted`, `ConfigRolledBack` variants and their `Display` implementations matching the §6.6 dotted strings.
- `core/crates/adapters/src/audit_log_store.rs` — extend the §6.6 vocabulary table consumed by `record_audit_event` if `config.rolled-back` is not yet present.
- `core/crates/cli/src/http/snapshots.rs` — wire the writer at the appropriate steps in `post_rollback`.

### Signatures and shapes

```rust
// core/crates/core/src/audit.rs (additions)
#[derive(Debug, Clone, PartialEq)]
pub enum AuditEvent {
    // ... existing variants ...
    RollbackRequested {
        target: SnapshotId,
        actor_id: String,
        actor_kind: ActorKind,
        intent: String,
    },
    RollbackOverrideAccepted {
        target: SnapshotId,
        condition_id: String,
        actor_id: String,
        reason: Option<String>,
    },
    ConfigRolledBack {
        target: SnapshotId,
        new_config_version: i64,
    },
}

impl std::fmt::Display for AuditEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            // ... existing arms ...
            Self::RollbackRequested { .. } => "mutation.submitted",
            Self::RollbackOverrideAccepted { .. } => "mutation.submitted",
            Self::ConfigRolledBack { .. } => "config.rolled-back",
        };
        f.write_str(s)
    }
}
```

The §6.6 vocabulary already lists `config.rolled-back`, `mutation.submitted`, `mutation.applied`, `config.applied`, `config.apply-failed`, and `mutation.rejected`. No new audit kinds are introduced; only new `AuditEvent` Rust variants whose `Display` returns existing dotted strings.

### Algorithm

For a successful rollback with overrides, audit rows are written in this order:

1. `RollbackRequested { target, actor_id, actor_kind, intent }` → `mutation.submitted`. `notes` carries `{ "rollback_target": <id>, "intent": <text> }`.
2. For each `condition_id` in the validated `OverrideSet.overrides`, write `RollbackOverrideAccepted { target, condition_id, actor_id, reason }` → `mutation.submitted`. `notes` carries `{ "override": condition_id, "reason": reason }`.
3. After `Applier::rollback` returns `Ok(outcome)`:
   1. Write `ConfigRolledBack { target, new_config_version }` → `config.rolled-back`. `notes` carries `{ "previous_version": <prev>, "new_version": <new> }`.
   2. Write `ApplySucceeded { snapshot_id: outcome.snapshot_id }` → `config.applied`.
4. On apply failure, write `ApplyFailed` → `config.apply-failed` with `error_kind` from the `ApplyError` discriminator.

Every row carries the request's `correlation_id`.

### Tests

Integration tests in `core/crates/cli/tests/rollback_http.rs` (extend the slice 12.5 file):

- `audit_rows_written_for_successful_rollback_with_one_override` — assert four rows in order: `mutation.submitted` (request), `mutation.submitted` (override), `config.rolled-back`, `config.applied`. All four share the correlation id.
- `audit_rows_written_for_failed_apply_after_passing_preflight` — induce a Caddy 5xx via the test double; assert rows `mutation.submitted`, `config.apply-failed`.
- `audit_row_correlation_ids_match_request` — assert every row's `correlation_id` equals the value of the `X-Correlation-Id` request header echoed in the response.
- `audit_row_notes_contains_override_reason` — assert the override row's `notes_json` contains the supplied reason.

### Acceptance command

`cargo test -p trilithon-cli --test rollback_http audit_rows`

### Exit conditions

- The four audit-row tests pass.
- `kind` strings emitted match the §6.6 vocabulary exactly.
- Every rollback path writes at least one audit row; no path is silent.

### Audit kinds emitted

All from architecture §6.6:

- `mutation.submitted`
- `config.rolled-back`
- `config.applied`
- `config.apply-failed`
- `mutation.rejected` (on validation failures inside `post_rollback` before the queue).

### Tracing events emitted

None new. The applier emits `apply.started`, `apply.succeeded`, `apply.failed` per trait-signatures.md §6 invariants.

### Cross-references

- Architecture §6.6 (`audit_log` and the `kind` vocabulary), §7.6 step 4–5.
- PRD T1.3, T1.7.
- Trait signatures §1 (`Storage::record_audit_event`).

---

## Slice 12.7 [standard] — Web UI snapshot history tab and rollback dialog

### Goal

Ship the user-facing surface: a per-route history tab listing parent linkage, actor, intent, timestamps, and a "Roll back to this point" button; a rollback dialog rendering the preflight report as a structured list, with an "I understand" toggle plus typed acknowledgement gate per failing overridable condition; an override-reason text area bounded at 1024 characters in the client and the server. Every rendered timestamp is in the viewer's local time zone (architecture H6); storage remains UTC Unix seconds.

### Entry conditions

- Slice 12.5 complete; the HTTP endpoints respond.
- The Phase 11 web shell provides authenticated routing, fetch helpers (`apiFetch`), the toast surface, and the route-detail layout.
- Vitest, React Testing Library, and `@axe-core/react` are available per the project conventions.

### Files to create or modify

- `web/src/features/routes/HistoryTab.tsx` — history list with rollback trigger.
- `web/src/features/routes/HistoryTab.test.tsx` — tests.
- `web/src/features/rollback/RollbackDialog.tsx` — modal dialog hosting the preflight result, override toggles, reason field, and submit button.
- `web/src/features/rollback/RollbackDialog.test.tsx` — tests.
- `web/src/features/rollback/api.ts` — typed `runPreflight`, `runRollback` helpers.
- `web/src/features/rollback/types.ts` — TypeScript shapes mirroring the Rust types.

### Signatures and shapes

```ts
// web/src/features/rollback/types.ts
export type ConditionStatus = 'pass' | 'fail' | 'warn';

export interface ConditionOutcome {
  readonly id: string;
  readonly status: ConditionStatus;
  readonly message: string;
  readonly overridable: boolean;
  readonly details: unknown;
}

export interface PreflightReport {
  readonly target: string;
  readonly outcomes: readonly ConditionOutcome[];
}

export interface RollbackRequestBody {
  readonly overrides?: readonly string[];
  readonly override_reason?: string;
  readonly expected_version: number;
}

export interface RollbackResponse {
  readonly snapshot_id: string;
  readonly config_version: number;
  readonly applied_at: number;
}
```

```ts
// web/src/features/rollback/api.ts
export async function runPreflight(snapshotId: string): Promise<PreflightReport>;
export async function runRollback(
  snapshotId: string,
  body: RollbackRequestBody,
): Promise<RollbackResponse>;
```

```tsx
// web/src/features/routes/HistoryTab.tsx
export interface HistoryTabProps {
  readonly routeId: string;
}

export function HistoryTab(props: HistoryTabProps): JSX.Element;
```

```tsx
// web/src/features/rollback/RollbackDialog.tsx
export interface RollbackDialogProps {
  readonly snapshotId: string;
  readonly currentConfigVersion: number;
  readonly open: boolean;
  readonly onClose: () => void;
  readonly onApplied: (response: RollbackResponse) => void;
}

export function RollbackDialog(props: RollbackDialogProps): JSX.Element;
```

### Algorithm — `RollbackDialog` state machine

States: `Loading`, `Reviewing`, `Submitting`, `Submitted`, `Errored`.

1. On mount with `open === true`, transition `Loading` and call `runPreflight(snapshotId)`.
2. On success, transition `Reviewing` with the report.
3. While `Reviewing`, render every outcome in a structured list. For each `Fail` with `overridable === true`, render a checkbox labelled with the outcome message. The submit button is disabled while any failing-overridable outcome is unchecked OR while `report.is_blocking()` is true and the typed acknowledgement field does not contain the literal string `I understand`.
4. The override-reason `<textarea>` is `maxLength={1024}` with a live counter.
5. On submit, transition `Submitting`; call `runRollback` with `overrides` set to the ids of every checked outcome plus the typed reason.
6. On `200`, transition `Submitted`, fire `onApplied(response)`, and close the dialog after a one-second toast.
7. On `4xx`/`5xx`, transition `Errored` with the structured error rendered next to the offending condition where the response carries an `id`.

### Algorithm — `HistoryTab`

1. Fetch parent chain through a `GET /api/v1/routes/{routeId}/history` endpoint (introduced by Phase 11; if absent, this slice ships the read endpoint as a thin wrapper around `Storage::parent_chain`).
2. Render each entry: snapshot id (truncated), parent id, actor display name, intent, displayed timestamp via `Intl.DateTimeFormat` in the viewer's locale.
3. Each row has a "Roll back to this point" button that opens `RollbackDialog` for that snapshot id.

### Tests

Vitest tests at `web/src/features/rollback/RollbackDialog.test.tsx`:

- `dialog_loads_preflight_on_open` — assert the loading state renders, then the `Reviewing` state with the fixture outcomes.
- `dialog_disables_submit_until_every_failure_overridden` — fixture with two failing-overridable outcomes; assert submit disabled with one checked and enabled with both checked plus typed acknowledgement.
- `dialog_blocks_submit_when_typed_acknowledgement_missing` — assert clicking submit without the literal acknowledgement string is a no-op.
- `dialog_enforces_1024_character_reason_bound` — type 1025 characters; assert the field truncates at 1024 and the live counter shows `1024 / 1024`.
- `dialog_shows_non_overridable_condition_as_blocking` — fixture has a `module-available` failure (`overridable: false`); assert no checkbox is rendered and submit stays disabled.
- `dialog_renders_response_error_inline` — server returns `422` with `OverrideError::NotOverridable`; assert the relevant outcome row renders an error.

Vitest tests at `web/src/features/routes/HistoryTab.test.tsx`:

- `history_tab_renders_parent_chain_in_reverse_chronological_order`.
- `history_tab_rollback_button_opens_dialog_with_correct_snapshot_id`.
- `history_tab_renders_local_time_for_utc_timestamps` — fixture supplies UTC Unix seconds; assert the rendered string matches `Intl.DateTimeFormat('en-US', { timeZone: 'UTC', ... })` invariants under a fixed locale.

### Acceptance command

`pnpm vitest run web/src/features/rollback web/src/features/routes/HistoryTab.test.tsx`

### Exit conditions

- All nine Vitest tests pass.
- `pnpm typecheck` passes.
- `pnpm lint` passes; no `any`, no non-null assertions, no `console.log`.
- `axe` reports zero serious accessibility violations on the dialog.

### Audit kinds emitted

None directly from the web UI; emission is server-side per slice 12.6.

### Tracing events emitted

None directly. The browser issues `POST` requests that produce server-side `http.request.received` and `http.request.completed` events.

### Cross-references

- PRD T1.3.
- Architecture §7.6, hazard H6 (time-zone display).
- Slice 12.5 (HTTP contract this UI consumes).

---

## Phase 12 exit checklist

- [ ] Every slice from 12.1 through 12.7 has shipped and its acceptance command passes.
- [ ] `just check` passes locally and in continuous integration.
- [ ] A rollback that fails preflight reports a structured error listing every failing condition.
- [ ] The user MAY override on a per-condition basis; each override is recorded in the audit log.
- [ ] A rollback that passes preflight (or whose every failing condition has been overridden) applies atomically.
- [ ] The snapshot history UI allows the user to browse parent linkage and trigger rollback.
- [ ] Every rendered timestamp displays in the viewer's local time zone; storage remains UTC Unix seconds (hazard H6).
- [ ] The override-reason length bound (1024 characters) is enforced both in the web UI and at the HTTP boundary.
- [ ] Every audit kind written by Phase 12 (`mutation.submitted`, `config.rolled-back`, `config.applied`, `config.apply-failed`, `mutation.rejected`) appears verbatim in architecture §6.6.

## Open questions

1. The preflight engine evaluates conditions sequentially. If a future phase introduces a slow condition class, parallel evaluation may become necessary; this is not planned for V1 and is filed here rather than silently designed-in.
2. The override-reason field is free text. Whether to add a structured taxonomy (for example, `stale-upstream`, `intentional-takeover`) is unresolved.
