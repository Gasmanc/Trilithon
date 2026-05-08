# Phase 07 — Configuration ownership reconciler (apply path) — Implementation Slices

> Phase reference: [../phases/phase-07-apply-path.md](../phases/phase-07-apply-path.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference [../phases/phase-07-apply-path.md](../phases/phase-07-apply-path.md).
- Architecture §6.5 (`snapshots`), §6.6 (audit kinds `config.applied`, `config.apply-failed`, `mutation.conflicted`), §6.7 (`mutations`), §7.1 (mutation lifecycle, eleven-step apply procedure), §8.1 (Caddy admin contract), §9 (concurrency model), §10 (failure model), §12.1 (tracing events `apply.started`, `apply.succeeded`, `apply.failed`).
- Trait signatures: `core::reconciler::Applier`, `core::caddy::CaddyClient`, `core::storage::Storage`, `core::diff::DiffEngine` (consumed for equivalence check).
- ADRs: ADR-0002 (Caddy JSON admin API as source of truth), ADR-0009 (immutable snapshots and audit log), ADR-0012 (optimistic concurrency on monotonic `config_version`), ADR-0013 (capability probe gates optional Caddy features).

## Slice plan summary

| # | Title | Primary files | Effort (h) | Depends on |
|---|-------|---------------|-----------:|-----------|
| 7.1 | `CaddyJsonRenderer` deterministic serialisation in pure core | `core/crates/core/src/reconciler/render.rs` | 6 | Phase 4, Phase 5 |
| 7.2 | `ApplyOutcome`, `ApplyError`, and apply-state types | `core/crates/core/src/reconciler/applier.rs` | 3 | 7.1 |
| 7.3 | Capability re-check at apply time | `core/crates/core/src/reconciler/capability_check.rs` | 4 | 7.2, Phase 3 |
| 7.4 | `Applier` adapter — happy path with audit `ApplyStarted`/`ApplySucceeded` | `core/crates/adapters/src/applier_caddy.rs` | 8 | 7.2, 7.3, Phase 6 |
| 7.5 | Optimistic concurrency on `config_version` | `core/crates/adapters/src/applier_caddy.rs` (extension), `core/crates/adapters/src/storage_sqlite/snapshots.rs` | 6 | 7.4 |
| 7.6 | In-process mutex plus SQLite advisory lock per `caddy_instance_id` | `core/crates/adapters/src/applier_caddy.rs` (extension), `core/crates/adapters/src/storage_sqlite/locks.rs` | 5 | 7.4 |
| 7.7 | Failure handling and reload-semantics audit metadata | `core/crates/adapters/src/applier_caddy.rs` (extension) | 5 | 7.4, 7.5 |
| 7.8 | TLS-issuance state separation in audit metadata | `core/crates/adapters/src/applier_caddy.rs` (extension) | 4 | 7.7 |

---

## Slice 7.1 [standard] — `CaddyJsonRenderer` deterministic serialisation in pure core

### Goal

Convert a `core::DesiredState` into a Caddy 2.x JSON document with byte-identical output for byte-identical inputs. The renderer is pure-core: no I/O, no async, no Caddy reachability. Phases 4, 5, and 6 depend on identical bytes-for-identical-state for content addressing; the renderer is the canonical serialiser.

### Entry conditions

- Phase 4 ships `core::DesiredState` with `unknown_extensions: BTreeMap<JsonPointer, serde_json::Value>` (per Phase 8 phase reference).
- Phase 5 ships `core::snapshot::SnapshotId` and the canonical-JSON serialiser.

### Files to create or modify

- `core/crates/core/src/reconciler/mod.rs` — module root.
- `core/crates/core/src/reconciler/render.rs` — the renderer.
- `core/crates/core/src/lib.rs` — `pub mod reconciler;`.

### Signatures and shapes

```rust
use serde_json::Value;
use crate::desired_state::DesiredState;

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RenderError {
    #[error("invalid hostname {host} at {path}")]
    InvalidHostname { host: String, path: String },
    #[error("upstream {target} at {path} does not parse as host:port")]
    InvalidUpstream { target: String, path: String },
    #[error("preset attachment references unknown preset {preset}@{version}")]
    UnknownPreset { preset: String, version: u32 },
}

pub trait CaddyJsonRenderer: Send + Sync + 'static {
    fn render(&self, state: &DesiredState) -> Result<Value, RenderError>;
}

pub struct DefaultCaddyJsonRenderer;
impl CaddyJsonRenderer for DefaultCaddyJsonRenderer {
    fn render(&self, state: &DesiredState) -> Result<Value, RenderError>;
}

/// Canonical-JSON byte serialisation.
/// Sort keys lexicographically. Numbers in shortest round-trip form. No
/// trailing whitespace. UTF-8.
pub fn canonical_json_bytes(value: &Value) -> Vec<u8>;
```

### Algorithm

1. Build the top-level Caddy config skeleton: `{"apps":{"http":{"servers":{...}},"tls":{"automation":{...}}},"@id":"trilithon-owner-<instance>"}`. The ownership sentinel `@id` is set per ADR-0015.
2. For each route in `state.routes` (iterate in `BTreeMap` order):
   - Compute the matcher block from `route.hostnames` and `route.path_matchers`.
   - Compute the handler block from `route.upstreams`, `route.headers`, and any attached policy preset (resolved via `state.policy_attachments`).
3. Merge `state.unknown_extensions` last; the merge MUST NOT overwrite a Trilithon-owned key. A collision is a programmer error and returns `RenderError`.
4. Serialise via `canonical_json_bytes` (sorted keys, normalised numbers).
5. Validate hostnames and upstreams as the renderer walks them; emit typed `RenderError` on the first violation. The validation is a fast double-check; Phase 4 already validated, but the renderer is the last line.

### Tests

- `core::reconciler::render::tests::deterministic_byte_identical_outputs` — render the same `DesiredState` twice; assert identical bytes.
- `core::reconciler::render::tests::sorted_keys_under_random_insert_order` — build a `DesiredState` whose routes are inserted in two different orders that produce identical sets; assert identical bytes.
- `core::reconciler::render::tests::unknown_extension_round_trip` — set `unknown_extensions["/apps/foo"] = {"bar":1}`; render; assert the path is present in the output.
- `core::reconciler::render::tests::ownership_sentinel_present` — assert the rendered top-level object contains `"@id": "trilithon-owner-local"`.
- `core::reconciler::render::tests::corpus_fixtures` — render every fixture under `core/crates/core/tests/fixtures/desired_state/` and compare to a checked-in `.caddy.json` snapshot via `insta`.

### Acceptance command

`cargo test -p trilithon-core reconciler::render::tests`

### Exit conditions

- Identical inputs MUST yield byte-identical outputs.
- The output MUST be valid JSON parseable by `serde_json::from_slice`.
- The ownership sentinel MUST be present in every render.
- `unknown_extensions` MUST round-trip.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- ADR-0002.
- PRD T1.1, T1.6.
- Architecture §6.5 (snapshots reference these bytes), §8.1 (Caddy admin contract), ADR-0015 (ownership sentinel).

---

## Slice 7.2 [standard] — `ApplyOutcome`, `ApplyError`, and apply-state types

### Goal

Land the typed apply outcomes consumed by the HTTP layer (Phase 9) and produced by the applier. Pure-core types; no behaviour. The phase reference enumerates `Succeeded`, `Failed { kind }`, `Conflicted { stale_version, current_version }`; this slice supplies them plus the `AppliedState` discriminator that separates "applied" from "TLS issuing" (slice 7.8 populates the latter).

### Entry conditions

- Slice 7.1 done.
- `core::snapshot::SnapshotId` is in scope.

### Files to create or modify

- `core/crates/core/src/reconciler/applier.rs` — types only at this slice.

### Signatures and shapes

```rust
use crate::snapshot::SnapshotId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplyOutcome {
    Succeeded {
        snapshot_id:     SnapshotId,
        config_version:  i64,
        applied_state:   AppliedState,
        reload_kind:     ReloadKind,
        latency_ms:      u32,
    },
    Failed {
        snapshot_id: SnapshotId,
        kind:        ApplyFailureKind,
        detail:      String,
    },
    Conflicted {
        stale_version:    i64,
        current_version:  i64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppliedState {
    /// The configuration is loaded; certificates are not necessarily issued.
    Applied,
    /// Reserved: a follow-up observation MAY upgrade the audit metadata.
    TlsIssuing { hostnames: Vec<String> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReloadKind { Graceful, Abrupt }

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplyFailureKind {
    CaddyValidation,        // Caddy 400
    CaddyServerError,       // Caddy 5xx
    CaddyUnreachable,
    CapabilityMismatch { missing_module: String },
    OwnershipSentinelConflict { observed: Option<String> },
    Renderer,               // RenderError surfaced upward
}

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("caddy rejected the load: {detail}")]
    CaddyRejected { detail: String },
    #[error("optimistic conflict: observed {observed_version}, expected {expected_version}")]
    OptimisticConflict { observed_version: i64, expected_version: i64 },
    #[error("capability mismatch: module {module} not loaded at apply time")]
    CapabilityMismatch { module: String },
    #[error("caddy unreachable: {detail}")]
    Unreachable { detail: String },
    #[error("ownership sentinel conflict (expected {expected}, observed {observed:?})")]
    OwnershipSentinelConflict { expected: String, observed: Option<String> },
    #[error("renderer: {0}")]
    Renderer(#[from] super::render::RenderError),
    #[error("storage: {0}")]
    Storage(String),
}
```

### Tests

- `core::reconciler::applier::tests::apply_outcome_serde_round_trip` — exercise every variant.
- `core::reconciler::applier::tests::apply_failure_kind_exhaustive` — match arm coverage assertion via a const list.

### Acceptance command

`cargo test -p trilithon-core reconciler::applier::tests`

### Exit conditions

- Every variant compiles and round-trips through `serde`.
- `cargo build -p trilithon-core` succeeds.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- trait-signatures.md §6 `Applier`.
- PRD T1.1.
- Architecture §7.1.

---

## Slice 7.3 [standard] — Capability re-check at apply time

### Goal

Hazard H5 forbids a configuration that references a module Caddy did not load. Phase 4 validates against the cached capability set; this slice re-checks against the live cache immediately before `POST /load`, satisfying the "at apply time" clause of the phase reference.

### Entry conditions

- Slice 7.2 done.
- Phase 3 ships `core::caddy::CapabilitySet` and a cache adapter `adapters::CapabilityCache`.

### Files to create or modify

- `core/crates/core/src/reconciler/capability_check.rs` — the re-check.

### Signatures and shapes

```rust
use crate::desired_state::DesiredState;
use crate::caddy::CapabilitySet;

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CapabilityCheckError {
    #[error("module {module} required by {site} is not loaded by the running Caddy")]
    Missing { module: String, site: String },
}

pub fn check_against_capability_set(
    state:        &DesiredState,
    capabilities: &CapabilitySet,
) -> Result<(), CapabilityCheckError>;
```

### Algorithm

1. Walk `state.routes`; for each route extract the set of required Caddy modules. Routes attaching `rate_limit`, `forward_auth`, `coraza`, or any future optional preset declare the module they need via `RoutePolicyAttachment::required_modules`.
2. For every required module, assert `capabilities.has_module(&module)`. The first miss returns `CapabilityCheckError::Missing` with `site` set to the JSON pointer of the offending route segment.
3. The function is pure: no async, no I/O. The applier (slice 7.4) calls it after fetching the live cache.

### Tests

- `core::reconciler::capability_check::tests::passes_with_full_capabilities` — synthetic state requiring `http.handlers.rate_limit` against a cap set that lists it.
- `core::reconciler::capability_check::tests::fails_when_module_missing` — same state, cap set without it; assert `Missing { module: "http.handlers.rate_limit", site: ... }`.
- `core::reconciler::capability_check::tests::stock_caddy_admits_basic_route` — a desired state with only reverse-proxy routes against the stock cap set passes.

### Acceptance command

`cargo test -p trilithon-core reconciler::capability_check::tests`

### Exit conditions

- A desired state requiring an absent module MUST fail the check before the applier issues `POST /load`.
- The check is pure; no I/O.

### Audit kinds emitted

None directly. The applier (slice 7.4) maps `CapabilityCheckError` to `ApplyError::CapabilityMismatch` and writes `config.apply-failed`.

### Tracing events emitted

None directly.

### Cross-references

- ADR-0013.
- PRD T1.1, T1.11.
- Hazard H5.
- Architecture §7.4.

---

## Slice 7.4 [cross-cutting] — `Applier` adapter — happy path with audit `ApplyStarted` and `ApplySucceeded`

### Goal

Stand up the `adapters::CaddyApplier` that implements `core::reconciler::Applier::apply`. This slice covers the happy path: render, capability re-check, `POST /load`, fetch `GET /config/`, equivalence check, and the two audit rows (`config.applied` on success; `config.apply-failed` on Caddy validation failure). Optimistic concurrency, advisory locks, and TLS-state separation land in subsequent slices.

### Entry conditions

- Slices 7.1, 7.2, 7.3 done.
- Phase 3 ships `CaddyClient` adapter.
- Phase 6 ships `AuditWriter`.

### Files to create or modify

- `core/crates/adapters/src/applier_caddy.rs` — the adapter.
- `core/crates/adapters/src/lib.rs` — re-export.

### Signatures and shapes

```rust
use std::sync::Arc;
use async_trait::async_trait;
use trilithon_core::reconciler::{
    Applier, ApplyOutcome, ApplyError, AppliedState, ReloadKind, ApplyFailureKind,
    render::{CaddyJsonRenderer, canonical_json_bytes},
    capability_check::check_against_capability_set,
};
use trilithon_core::caddy::CaddyClient;
use trilithon_core::diff::DiffEngine;
use trilithon_core::storage::Storage;
use trilithon_core::audit::AuditEvent;
use trilithon_core::snapshot::{Snapshot, SnapshotId};
use crate::audit_writer::{AuditWriter, AuditAppend};
use crate::capability_cache::CapabilityCache;

pub struct CaddyApplier {
    pub client:       Arc<dyn CaddyClient>,
    pub renderer:     Arc<dyn CaddyJsonRenderer>,
    pub diff_engine:  Arc<dyn DiffEngine>,
    pub capabilities: Arc<CapabilityCache>,
    pub audit:        Arc<AuditWriter>,
    pub storage:      Arc<dyn Storage>,
    pub instance_id:  String,                     // 'local' for V1
    pub clock:        Arc<dyn trilithon_core::clock::Clock>,
}

#[async_trait]
impl Applier for CaddyApplier {
    async fn apply(
        &self,
        snapshot: &Snapshot,
        expected_version: i64,
    ) -> Result<ApplyOutcome, ApplyError>;

    async fn validate(
        &self,
        snapshot: &Snapshot,
    ) -> Result<trilithon_core::reconciler::ValidationReport, ApplyError>;

    async fn rollback(
        &self,
        target: &SnapshotId,
    ) -> Result<ApplyOutcome, ApplyError>;
}
```

### Algorithm

1. Open a span `tracing::info_span!("apply.started", correlation_id, snapshot.id = %snapshot.id, snapshot.config_version = snapshot.config_version)`. Emit the `apply.started` event.
2. Write `mutation.submitted` audit row (Phase 9 also emits this; the applier MUST not double-write — this row belongs to the HTTP path; the applier's first audit row is the apply-specific one). Construct an `AuditAppend` with `event = AuditEvent::ApplyStarted` (a reserved kind not in the V1 vocabulary as listed; this slice flags it as an open question — see below). For now, the applier writes `mutation.applied` only on success and `config.apply-failed` on failure; an `apply.started` audit row is NOT emitted in V1.
3. Render: `let body = self.renderer.render(&snapshot.desired_state)?;`. Convert to canonical bytes.
4. Capability re-check: `let caps = self.capabilities.current(&self.instance_id).await; check_against_capability_set(&snapshot.desired_state, &caps).map_err(|e| ApplyError::CapabilityMismatch { module: e.module })?`.
5. Issue `self.client.load_config(body.clone()).await`. On non-2xx, `ApplyError::CaddyRejected { detail }`.
6. Fetch `let observed = self.client.get_running_config().await?;`. Run `self.diff_engine.structural_diff(&snapshot.desired_state, &observed)?` against the architecture §7.2 ignore list. A non-empty diff (excluding ignored paths) is a protocol violation and surfaces as `ApplyError::CaddyRejected { detail: "post-load equivalence failed" }`.
7. On success, write one `config.applied` audit row via `AuditWriter::record` with `outcome = Ok`, `snapshot_id = Some(...)`, `notes = Some(serde_json::json!({ "reload_kind": reload_kind, "applied_state": "applied" }).to_string())`.
8. Emit `apply.succeeded` tracing event.
9. Return `ApplyOutcome::Succeeded { snapshot_id, config_version, applied_state: AppliedState::Applied, reload_kind: ReloadKind::Graceful, latency_ms }`.

On Caddy 4xx:

10. Write one `config.apply-failed` audit row with `outcome = Error`, `error_kind = "CaddyValidation"`, `notes` carrying the bounded body excerpt.
11. Emit `apply.failed` tracing event.
12. Return `ApplyOutcome::Failed { snapshot_id, kind: ApplyFailureKind::CaddyValidation, detail }` (NOT an `Err`; failures are a typed outcome at the trait surface — but the Phase 7 phase ref shows the trait returning `Result<ApplyOutcome, ApplyError>` so map: a Caddy 4xx is a successful invocation that returned a `Failed` outcome, not an `Err`. An `Err` is reserved for transport-level or programmer-error conditions).

### Tests

- `core/crates/adapters/tests/apply_happy_path.rs` — fake `CaddyClient` returning 200; assert `Succeeded` outcome and one `config.applied` audit row.
- `core/crates/adapters/tests/apply_caddy_400.rs` — fake returning 400; assert `Failed { kind: CaddyValidation, .. }`, one `config.apply-failed` audit row, no `config.applied` row, snapshot pointer unchanged.
- `core/crates/adapters/tests/apply_caddy_unreachable.rs` — fake transport error; assert `Err(ApplyError::Unreachable { .. })` and one `caddy.unreachable` audit row.
- `core/crates/adapters/tests/apply_post_load_equivalence_check.rs` — fake returns 200 but `GET /config/` differs in a non-ignored path; assert `Failed`.

### Acceptance command

`cargo test -p trilithon-adapters apply_`

### Exit conditions

- A 200 from Caddy MUST produce exactly one `config.applied` audit row.
- A 4xx from Caddy MUST produce exactly one `config.apply-failed` audit row and MUST NOT advance any pointer.
- The applier MUST emit `apply.started`, `apply.succeeded` or `apply.failed` tracing events per architecture §12.1.

### Audit kinds emitted

`config.applied`, `config.apply-failed`, `caddy.unreachable` (architecture §6.6).

### Tracing events emitted

`apply.started`, `apply.succeeded`, `apply.failed` (architecture §12.1).

### Cross-references

- ADR-0002, ADR-0009, ADR-0013.
- PRD T1.1, T1.6, T1.7.
- Architecture §7.1, §8.1.
- trait-signatures.md §6 `Applier`.

---

## Slice 7.5 [cross-cutting] — Optimistic concurrency on `config_version`

### Goal

Reject any apply whose `expected_version` does not match the database's current `config_version` for the instance. This is the substrate for T2.10 (the user-visible conflict UX lands in Phase 17). The check is wrapped in a transaction that reads the latest `config_version`, compares to `expected_version`, and either advances the pointer or aborts with `ApplyError::OptimisticConflict`.

### Entry conditions

- Slice 7.4 done.

### Files to create or modify

- `core/crates/adapters/src/applier_caddy.rs` — extend `apply` to wrap the pointer advance.
- `core/crates/adapters/src/storage_sqlite/snapshots.rs` — add `current_config_version` and `advance_config_version_if_eq` helpers.

### Signatures and shapes

```rust
// core/crates/adapters/src/storage_sqlite/snapshots.rs

pub async fn current_config_version(
    conn:        &mut rusqlite::Connection,
    instance_id: &str,
) -> Result<i64, StorageError>;

/// CAS-style advance. Returns `Ok(new_version)` if the previous value matched.
/// Returns `Err(StorageError::OptimisticConflict { observed, expected })`
/// otherwise.
pub async fn advance_config_version_if_eq(
    conn:             &mut rusqlite::Connection,
    instance_id:      &str,
    expected_version: i64,
    new_snapshot_id:  &SnapshotId,
) -> Result<i64, StorageError>;
```

A new variant on `StorageError`:

```rust
#[error("optimistic conflict: observed {observed}, expected {expected}")]
OptimisticConflict { observed: i64, expected: i64 },
```

### Algorithm

1. The applier opens an immediate-mode transaction before issuing `POST /load`.
2. Within the transaction, read the unique row in `snapshots` for this `caddy_instance_id` whose `config_version` is the maximum: `SELECT config_version FROM snapshots WHERE caddy_instance_id = ?1 ORDER BY config_version DESC LIMIT 1`.
3. If the observed version differs from `expected_version`, abort. Translate to `ApplyError::OptimisticConflict { observed_version, expected_version }`. Write one `mutation.conflicted` audit row.
4. If equal, commit the transaction after Caddy returns 200; the snapshot row inserted by Phase 5 already carries `config_version = expected_version + 1` because the Phase 4 mutation pipeline assigned it.
5. If Caddy returns non-2xx, roll back the transaction; the pointer is untouched.

### Tests

- `core/crates/adapters/tests/apply_concurrency_two_actors.rs` — launch two `apply()` calls in parallel, both targeting `expected_version = 5`; assert exactly one returns `Succeeded` and the other returns `Err(ApplyError::OptimisticConflict { .. })`. The losing call MUST produce a `mutation.conflicted` audit row.
- `core/crates/adapters/tests/apply_stale_version_rejected.rs` — fixture with `current = 10`, `expected = 9`; assert conflict.
- `core/crates/adapters/tests/apply_pointer_unchanged_on_conflict.rs` — assert the database's `config_version` is unchanged after a conflict.

### Acceptance command

`cargo test -p trilithon-adapters apply_concurrency_ apply_stale_ apply_pointer_unchanged_`

### Exit conditions

- A stale `expected_version` MUST surface `ApplyError::OptimisticConflict`.
- The conflict MUST produce exactly one `mutation.conflicted` audit row.
- A successful apply advances `config_version` by 1.
- The pointer is untouched on any failed transaction.

### Audit kinds emitted

`mutation.conflicted` (architecture §6.6).

### Tracing events emitted

None new; the `apply.failed` event from slice 7.4 covers the conflict path with `error.kind = OptimisticConflict`.

### Cross-references

- ADR-0012.
- PRD T1.1, T1.8 substrate, T2.10.
- Hazard H8.
- Architecture §6.5 unique index on `(caddy_instance_id, config_version)`.

---

## Slice 7.6 [cross-cutting] — In-process mutex plus SQLite advisory lock per `caddy_instance_id`

### Goal

Guarantee that at most one apply is in flight per `caddy_instance_id`. The slice combines a `tokio::sync::Mutex` (one per instance, held for the duration of the apply) with a SQLite advisory lock (a row in a `locks` table acquired via `INSERT OR IGNORE` plus `BEGIN IMMEDIATE`) so that a second daemon process pointed at the same database also serialises.

### Entry conditions

- Slice 7.4 done.

### Files to create or modify

- `core/crates/adapters/src/applier_caddy.rs` — wrap `apply` body in lock acquisition.
- `core/crates/adapters/src/storage_sqlite/locks.rs` — advisory lock helpers and the migration extension.
- `core/crates/adapters/migrations/0004_apply_locks.sql` — `CREATE TABLE apply_locks (instance_id TEXT PRIMARY KEY, holder_pid INTEGER NOT NULL, acquired_at INTEGER NOT NULL);`.

### Signatures and shapes

```rust
// core/crates/adapters/src/storage_sqlite/locks.rs

#[derive(Clone, Debug, thiserror::Error)]
pub enum LockError {
    #[error("apply lock already held by pid {pid}")]
    AlreadyHeld { pid: i32 },
    #[error("storage: {0}")]
    Storage(#[from] StorageError),
}

pub struct AcquiredLock { instance_id: String, holder_pid: i32 }

pub async fn acquire_apply_lock(
    pool:        &SqlitePool,
    instance_id: &str,
    holder_pid:  i32,
) -> Result<AcquiredLock, LockError>;

impl Drop for AcquiredLock {
    fn drop(&mut self) {
        // best-effort DELETE FROM apply_locks WHERE instance_id = ?1 AND holder_pid = ?2
    }
}
```

```rust
// core/crates/adapters/src/applier_caddy.rs

pub struct CaddyApplier { /* … existing fields … */
    pub instance_mutex: Arc<tokio::sync::Mutex<()>>,
}
```

### Algorithm

1. On `apply()` entry, acquire `let _guard = self.instance_mutex.lock().await;` (in-process serialisation).
2. While holding the mutex, call `acquire_apply_lock(&pool, &self.instance_id, std::process::id() as i32).await?`. The advisory lock is row-level: `INSERT INTO apply_locks (instance_id, holder_pid, acquired_at) VALUES (?, ?, ?)` returns a uniqueness-violation error if another process holds the lock. Translate to `LockError::AlreadyHeld`.
3. Run the apply body (slices 7.4 + 7.5).
4. The advisory-lock guard's `Drop` deletes the row regardless of outcome. The mutex guard releases on scope exit.
5. A stale `apply_locks` row from a crashed process is reaped on the next acquire by checking `holder_pid` against `proc/<pid>/status` or, on platforms without procfs, by stamping a TTL of 5 minutes and considering the row expired.

### Tests

- `core/crates/adapters/tests/apply_serial_under_32_concurrent_callers.rs` — spawn 32 concurrent `apply()` invocations against the same instance; instrument the start/end of the critical section; assert at most one is "in-flight" at any tick (sample at 1 ms intervals).
- `core/crates/adapters/tests/apply_lock_released_on_panic.rs` — run a deliberately panicking apply; assert the next apply succeeds, proving the lock was released by the guard's `Drop`.
- `core/crates/adapters/tests/apply_advisory_lock_blocks_second_process.rs` — open two `SqlitePool`s pointing at the same database; the second's lock acquisition MUST return `LockError::AlreadyHeld`.

### Acceptance command

`cargo test -p trilithon-adapters apply_serial_ apply_lock_released_ apply_advisory_lock_`

### Exit conditions

- 32 concurrent `apply()` calls MUST observe at most one in-flight at any sampled instant.
- A panic in the apply body MUST release the lock.
- A second process MUST be blocked while the first holds the lock.

### Audit kinds emitted

None new.

### Tracing events emitted

None new.

### Cross-references

- PRD T1.1.
- Architecture §9 (concurrency model).
- ADR-0012.

---

## Slice 7.7 [cross-cutting] — Failure handling and reload-semantics audit metadata

### Goal

Strengthen the failure path so that every apply produces exactly one terminal audit row with reload semantics recorded for hazard H4. The slice extends the audit row's `notes` payload with a structured JSON document `{ reload_kind, applied_state, drain_window_ms?, error_kind?, error_detail? }` and asserts the property "exactly one `ApplyStarted` (Phase 9 emits this) and exactly one terminal" via a property test.

### Entry conditions

- Slices 7.4, 7.5, 7.6 done.

### Files to create or modify

- `core/crates/adapters/src/applier_caddy.rs` — extend the audit `notes` builder.
- `core/crates/core/src/reconciler/applier.rs` — add `ReloadKind::Graceful { drain_window_ms: u32 }` if the prior shape was unitary; align as needed.

### Signatures and shapes

```rust
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ApplyAuditNotes {
    pub reload_kind:     ReloadKind,
    pub applied_state:   AppliedStateTag,           // "applied" | "tls-issuing"
    pub drain_window_ms: Option<u32>,               // populated when reload_kind = Graceful
    pub error_kind:      Option<&'static str>,
    pub error_detail:    Option<String>,
    pub caddy_status:    Option<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppliedStateTag { Applied, TlsIssuing }
```

### Algorithm

1. On every audit append from the applier, construct `ApplyAuditNotes` with the appropriate fields populated.
2. Serialise to canonical JSON; place in `AuditAppend.notes`.
3. The default `reload_kind` is `Graceful` with `drain_window_ms = config.caddy.drain_window_ms.unwrap_or(5_000)`. The configuration knob lands in `core::config::CaddyConfig` (Phase 1).
4. The "exactly one terminal" property test runs N=200 randomised apply scenarios and counts audit rows per apply via `correlation_id`.

### Tests

- `core/crates/adapters/tests/apply_audit_notes_present.rs` — every successful apply has parseable `ApplyAuditNotes` in `notes`.
- `core/crates/adapters/tests/apply_audit_notes_failure_carries_error_kind.rs` — Caddy 400 produces notes with `error_kind = Some("CaddyValidation")`.
- `core/crates/adapters/tests/apply_exactly_one_terminal_row.rs` — property test asserting exactly one of `{config.applied, config.apply-failed, mutation.conflicted}` per `correlation_id`.

### Acceptance command

`cargo test -p trilithon-adapters apply_audit_notes_ apply_exactly_one_terminal_row`

### Exit conditions

- Every apply MUST produce a parseable `ApplyAuditNotes` JSON in `audit_log.notes`.
- Every `correlation_id` from an apply MUST have exactly one terminal audit row.
- `reload_kind` MUST be recorded for hazard H4.

### Audit kinds emitted

`config.applied`, `config.apply-failed`, `mutation.conflicted`.

### Tracing events emitted

None new.

### Cross-references

- Hazard H4.
- PRD T1.1, T1.7.
- Architecture §6.6 `notes` column.

---

## Slice 7.8 [standard] — TLS-issuance state separation in audit metadata

### Goal

Hazard H17: `POST /load` returns quickly, but ACME issuance may take seconds-to-minutes. The applier MUST NOT block on issuance. This slice adds a follow-up observer task that, after a successful apply that introduces a new managed hostname, polls Caddy's `/pki` endpoints for up to 120 seconds and emits a separate audit row when issuance completes (or times out). The original `config.applied` row records `applied_state = "applied"` immediately.

### Entry conditions

- Slice 7.7 done.

### Files to create or modify

- `core/crates/adapters/src/applier_caddy.rs` — spawn the follow-up observer.
- `core/crates/adapters/src/tls_observer.rs` — the bounded observer task.

### Signatures and shapes

```rust
pub struct TlsIssuanceObserver {
    pub client:  Arc<dyn CaddyClient>,
    pub audit:   Arc<AuditWriter>,
    pub timeout: std::time::Duration,   // default 120 s
}

impl TlsIssuanceObserver {
    pub async fn observe(
        &self,
        correlation_id: ulid::Ulid,
        hostnames:      Vec<String>,
    ) -> ();   // emits audit rows; never returns an error to the caller
}
```

### Algorithm

1. After `apply` writes the success row, the applier compares the new desired-state hostnames against the previous snapshot's hostnames.
2. For every `added` hostname whose configuration enables managed TLS, spawn `TlsIssuanceObserver::observe(correlation_id, added)`.
3. The observer polls `client.get_certificates()` every 5 seconds for up to `self.timeout`. When every hostname appears with a non-expired certificate, emit one `config.applied` audit row with `notes.applied_state = "tls-issuing"` transitioning to `"applied"`. On timeout, emit one `config.apply-failed` audit row with `notes.error_kind = Some("TlsIssuanceTimeout")` and `notes.error_detail = "ACME provisioning did not complete within 120s"`.
4. The observer never blocks the original `apply()` return.

### Tests

- `core/crates/adapters/tests/apply_does_not_block_on_tls_issuance.rs` — fake `CaddyClient` whose `get_certificates` returns the cert only after 30 seconds; assert `apply()` returns within 2 seconds.
- `core/crates/adapters/tests/apply_emits_tls_issuance_followup_row.rs` — assert that 30 seconds after the apply, a follow-up `config.applied` audit row appears with `applied_state = "applied"`.
- `core/crates/adapters/tests/apply_emits_tls_issuance_timeout_row.rs` — fake never returns the cert; after 120 seconds, assert one `config.apply-failed` audit row with `TlsIssuanceTimeout`.

### Acceptance command

`cargo test -p trilithon-adapters apply_does_not_block_on_tls apply_emits_tls_`

### Exit conditions

- `apply()` MUST return immediately once Caddy responds 200, regardless of certificate state.
- A follow-up audit row MUST be emitted within `timeout + 5` seconds carrying `applied_state` resolved to `applied` or a timeout failure.
- The original audit row carries `applied_state = "applied"` (not `"tls-issuing"`); the follow-up upgrades the observation. The pair is correlated via `correlation_id`.

### Audit kinds emitted

`config.applied`, `config.apply-failed`.

### Tracing events emitted

None new.

### Cross-references

- Hazard H17.
- PRD T1.1.
- Architecture §7.1, §8.1.

---

## Phase exit checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] Given desired state X and running state X, no apply is performed (covered by Phase 8 plus the equivalence check in slice 7.4).
- [ ] Given desired state Y and running state X, exactly one apply is performed and the resulting running state equals Y (slice 7.4).
- [ ] An apply that fails at Caddy validation does not advance the desired-state pointer (slice 7.5).
- [ ] All applies are wrapped in optimistic concurrency control on `config_version`; a stale apply is rejected with `ApplyError::OptimisticConflict` (slice 7.5).
- [ ] Every apply produces exactly one terminal audit row (slice 7.7).
- [ ] Apply latency p95 < 2 seconds excluding ACME issuance (architecture §13).
- [ ] `core/README.md` documents the apply lifecycle, citing ADR-0012.

## Open questions

- The Phase 7 phase reference asks for an `ApplyStarted` audit kind. Architecture §6.6 does not list an `ApplyStarted` wire kind; the closest is `mutation.submitted`. This breakdown follows the §6.6 vocabulary and emits `mutation.submitted` from the HTTP layer (Phase 9), not from the applier. If the project owner wants a dedicated `apply.started` audit kind, the §6.6 table MUST be amended in the same commit, satisfying the "Vocabulary authority" rule.
- `validate(snapshot)` in slice 7.4's `Applier` impl is partial; full validation lands in Phase 12 preflight. This breakdown leaves the method returning `ApplyError::PreflightFailed { failures: vec![] }` as a placeholder that Phase 12 replaces.
