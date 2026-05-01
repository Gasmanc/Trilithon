# Phase 08 — Drift detection loop — Implementation Slices

> Phase reference: [../phases/phase-08-drift-detection.md](../phases/phase-08-drift-detection.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference [../phases/phase-08-drift-detection.md](../phases/phase-08-drift-detection.md).
- Architecture §6.6 (audit kinds `config.drift-detected`, `config.drift-resolved`), §7.2 (drift loop and the Caddy-managed-paths ignore list), §9 (concurrency model), §12.1 (tracing events `drift.detected`, `drift.resolved`).
- Trait signatures: `core::diff::DiffEngine`, `core::caddy::CaddyClient`, `core::storage::Storage::record_drift_event` and `latest_drift_event`.
- ADRs: ADR-0002, ADR-0009.

## Slice plan summary

| # | Title | Primary files | Effort (h) | Depends on |
|---|-------|---------------|-----------:|-----------|
| 8.1 | `DiffEngine` structural diff over canonical JSON | `core/crates/core/src/diff.rs` | 8 | Phase 5, Phase 7 |
| 8.2 | Caddy-managed-paths ignore list (architecture §7.2) | `core/crates/core/src/diff/ignore_list.rs` | 3 | 8.1 |
| 8.3 | `DriftEvent`, `DiffCounts`, and `DesiredState::unknown_extensions` round-trip | `core/crates/core/src/diff.rs` (extension), `core/crates/core/src/desired_state.rs` | 4 | 8.1 |
| 8.4 | Three resolution APIs in core: adopt, reapply, defer | `core/crates/core/src/diff/resolve.rs` | 5 | 8.3, Phase 4 |
| 8.5 | `DriftDetector` scheduler with `tokio::time::interval` and apply-in-flight skip | `core/crates/adapters/src/drift.rs` | 6 | 8.4, Phase 7 |
| 8.6 | Drift audit row writer plus deduplication per cycle | `core/crates/adapters/src/drift.rs` (extension) | 4 | 8.5 |

---

## Slice 8.1 — `DiffEngine` structural diff over canonical JSON

### Goal

Implement `core::diff::DiffEngine::structural_diff` per the trait signature in `trait-signatures.md` §5. The implementation flattens both desired-state values to `BTreeMap<JsonPointer, JsonValue>`, computes set differences, and returns a `Diff` with `added`, `removed`, and `modified` entries. Pure-core; no async; no I/O.

### Entry conditions

- Phase 4 ships `core::DesiredState` and a canonical-JSON serialiser.
- Phase 5 ships content-addressing using the same canonical JSON.

### Files to create or modify

- `core/crates/core/src/diff.rs` — module root and `DefaultDiffEngine`.
- `core/crates/core/src/diff/flatten.rs` — JsonPointer-keyed flattener.
- `core/crates/core/src/lib.rs` — `pub mod diff;`.

### Signatures and shapes

```rust
use std::collections::BTreeMap;
use serde_json::Value;
use crate::desired_state::DesiredState;

#[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct JsonPointer(pub String);

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DiffEntry {
    Added    { path: JsonPointer, after:  Value },
    Removed  { path: JsonPointer, before: Value },
    Modified { path: JsonPointer, before: Value, after: Value },
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Diff {
    pub entries:        Vec<DiffEntry>,
    pub ignored_count:  u32,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DiffError {
    #[error("incompatible shape at {path}: cannot diff {before_kind} against {after_kind}")]
    IncompatibleShape { path: JsonPointer, before_kind: String, after_kind: String },
    #[error("redaction violated: plaintext secret remains at {path}")]
    RedactionViolated { path: JsonPointer },
}

pub trait DiffEngine: Send + Sync + 'static {
    fn structural_diff(
        &self,
        before: &DesiredState,
        after:  &DesiredState,
    ) -> Result<Diff, DiffError>;

    fn apply_diff(
        &self,
        state: &DesiredState,
        diff:  &Diff,
    ) -> Result<DesiredState, DiffError>;

    fn redact_diff(
        &self,
        diff:   &Diff,
        schema: &crate::schema::SchemaRegistry,
    ) -> Result<RedactedDiff, DiffError>;
}

pub struct DefaultDiffEngine;
```

### Algorithm

Per phase reference, verbatim:

1. Flatten each side to `BTreeMap<JsonPointer, JsonValue>`: `flat_a = flatten(state_a.canonical_json())`, `flat_b = flatten(state_b.canonical_json())`. The flattener walks objects and arrays, emitting `(JsonPointer, leaf)` tuples; only scalar leaves are emitted (objects and arrays expand to nested paths).
2. Compute key sets `keys_a`, `keys_b`. Symmetric difference yields `added` and `removed`.
3. For each key in `keys_a ∩ keys_b`, if `flat_a[k] != flat_b[k]`, classify as `Modified { before, after }`.
4. Discard any entry whose path matches the ignore list (slice 8.2). Increment `ignored_count` for each discard.
5. Return `Diff { entries, ignored_count }`.

`apply_diff` is the inverse: given `state` and a `Diff`, walk the entries, mutate the `state.canonical_json()` value at each pointer, and reparse. Returns `DesiredState` or `IncompatibleShape` on a structural conflict.

### Tests

- `core::diff::tests::adds_detected` — leaf added at a deeply nested path.
- `core::diff::tests::removes_detected` — leaf removed.
- `core::diff::tests::modifies_detected` — leaf changed.
- `core::diff::tests::unchanged_returns_empty_diff` — `Diff::is_empty()` is true.
- `core::diff::tests::array_index_pointer_format` — array changes report pointers like `/routes/2/upstreams/0/dial`.
- `core::diff::tests::deterministic_ordering` — entries are ordered by `JsonPointer` lexicographically.
- `core::diff::tests::apply_diff_inverse_round_trip` — `apply_diff(state_a, diff(state_a, state_b)) == state_b`.

### Acceptance command

`cargo test -p trilithon-core diff::tests`

### Exit conditions

- The engine produces deterministic, ordered entries.
- `Diff::is_empty()` returns true when the inputs are byte-equal in canonical JSON.
- `apply_diff` MUST be the inverse of `structural_diff` for any non-conflicting input pair.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- trait-signatures.md §5 `DiffEngine`.
- PRD T1.4 substrate.
- Architecture §7.2.

---

## Slice 8.2 — Caddy-managed-paths ignore list (architecture §7.2)

### Goal

Encode the closed list of JSON pointers that Caddy mutates on its own and that the diff engine MUST discard. Per architecture §7.2 the list covers TLS issuance state, upstream health caches, `automatic_https.disable_redirects` autopopulation, and `request_id` placeholders. The ignore list is a static table with a single matcher function; new entries land here in subsequent phases.

### Entry conditions

- Slice 8.1 done.

### Files to create or modify

- `core/crates/core/src/diff/ignore_list.rs` — the table and matcher.
- `core/crates/core/src/diff.rs` — wire the matcher into `structural_diff`.

### Signatures and shapes

```rust
use crate::diff::JsonPointer;

/// Ordered, closed list. New entries MUST be added in the same commit
/// that updates architecture §7.2.
pub const CADDY_MANAGED_PATH_PATTERNS: &[&str] = &[
    // TLS issuance state, populated by Caddy's ACME machinery.
    "^/apps/tls/automation/policies/[^/]+/managed_certificates(/.*)?$",
    "^/storage/.*",
    // Upstream health caches surfaced via /reverse_proxy/upstreams.
    "^/apps/http/servers/[^/]+/routes/[^/]+/handle/[^/]+/upstreams/[^/]+/health(/.*)?$",
    // automatic_https populates this when the user has not.
    "^/apps/http/servers/[^/]+/automatic_https/disable_redirects$",
    // Request id placeholder injection.
    "^/apps/http/servers/[^/]+/request_id$",
];

pub fn is_caddy_managed(path: &JsonPointer) -> bool;
```

### Algorithm

1. Compile every pattern in `CADDY_MANAGED_PATH_PATTERNS` once at startup using `once_cell::sync::Lazy<Vec<regex::Regex>>`.
2. `is_caddy_managed` returns true if any compiled regex matches `path.0`.
3. The structural-diff routine (slice 8.1 step 4) calls this for every candidate entry.

### Tests

- `core::diff::ignore_list::tests::matches_managed_certificates` — `/apps/tls/automation/policies/default/managed_certificates/example.com` matches.
- `core::diff::ignore_list::tests::matches_upstream_health` — a deep upstream-health path matches.
- `core::diff::ignore_list::tests::does_not_match_user_owned_route_field` — `/apps/http/servers/srv0/routes/0/handle/0/upstreams/0/dial` does NOT match (this is user-owned).
- `core::diff::ignore_list::tests::patterns_compile` — `regex::Regex::new` succeeds on every pattern.

### Acceptance command

`cargo test -p trilithon-core diff::ignore_list::tests`

### Exit conditions

- The pattern set MUST be the closed list above.
- Every pattern MUST compile.
- `is_caddy_managed` is the only consumer of the list outside tests.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- Architecture §7.2.
- PRD T1.4.

---

## Slice 8.3 — `DriftEvent`, `DiffCounts`, and `DesiredState::unknown_extensions` round-trip

### Goal

Land the `DriftEvent` record (consumed by `Storage::record_drift_event`), the `DiffCounts` summary keyed by object kind, and the `DesiredState::unknown_extensions` field. The unknown-extension preservation closes the loop: a Caddy field Trilithon does not yet model is preserved through render → ingest → render unchanged.

### Entry conditions

- Slice 8.1 done.

### Files to create or modify

- `core/crates/core/src/diff.rs` — extend with `DriftEvent`, `DiffCounts`, `ObjectKind`.
- `core/crates/core/src/desired_state.rs` — add `pub unknown_extensions: BTreeMap<JsonPointer, serde_json::Value>` if Phase 4 has not already.

### Signatures and shapes

```rust
use std::collections::BTreeMap;
use ulid::Ulid;
use crate::snapshot::SnapshotId;
use crate::diff::JsonPointer;

#[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ObjectKind { Route, Upstream, Tls, Server, Policy, Other }

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiffCounts { pub added: u32, pub removed: u32, pub modified: u32 }

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DriftEvent {
    pub before_snapshot_id: SnapshotId,
    pub running_state_hash: String,                   // SHA-256, lowercase hex
    pub diff_summary:       BTreeMap<ObjectKind, DiffCounts>,
    pub detected_at:        i64,                      // unix seconds (UTC)
    pub correlation_id:     Ulid,
    pub redacted_diff_json: String,                   // canonical JSON, redacted
    pub redaction_sites:    u32,
}

// Field added to DesiredState (or assert pre-existence).
pub struct DesiredState {
    /* existing fields … */
    pub unknown_extensions: BTreeMap<JsonPointer, serde_json::Value>,
}
```

### Algorithm

1. `DriftEvent` is constructed by the detector (slice 8.5) after running the diff engine plus the redactor.
2. `DiffCounts` is populated by classifying each `DiffEntry`'s `JsonPointer` to an `ObjectKind`. The classifier is a static prefix matcher: `/apps/http/servers/*/routes/*` → `Route`; `/apps/tls/*` → `Tls`; `/apps/http/servers/*` → `Server`; further patterns extend this list as the desired-state model grows.
3. `unknown_extensions` is populated during ingest (Phase 13 for Caddyfile import; here only the field shape ships) and consumed during render (slice 7.1).
4. Canonical-JSON serialisation MUST sort keys lexicographically; the round-trip property test confirms byte-stability under random re-order of insertion.

### Tests

- `core::diff::tests::drift_event_serde_round_trip` — full row round-trip.
- `core::diff::tests::diff_counts_classifier` — corpus mapping pointers to `ObjectKind`.
- `core::desired_state::tests::unknown_extensions_round_trip` — set `unknown_extensions["/apps/foo"] = json!({"bar":1})`; canonicalise; reparse; assert byte-equal.
- `core::desired_state::tests::canonical_json_byte_stable_under_insert_order` — property test (`proptest`) inserting random keys in two orders; byte-equal after canonicalise.

### Acceptance command

`cargo test -p trilithon-core diff::tests::drift_event_ desired_state::tests::`

### Exit conditions

- `DriftEvent` round-trips through serde.
- `unknown_extensions` is preserved byte-stably.

### Audit kinds emitted

None directly (slice 8.6 emits `config.drift-detected` carrying the event payload).

### Tracing events emitted

None.

### Cross-references

- PRD T1.4.
- Architecture §6.6, §7.2.
- trait-signatures.md `Storage::record_drift_event`.

---

## Slice 8.4 — Three resolution APIs in core: adopt, reapply, defer

### Goal

Provide pure-core functions that translate a drift event into exactly one mutation per resolution path. `adopt_running_state` synthesises a mutation that replaces desired with the running state; `reapply_desired_state` synthesises a mutation that re-targets the existing desired state through the apply path; `defer_for_manual_reconciliation` records a no-op marker that surfaces in the dual-pane editor (Phase 15).

### Entry conditions

- Slice 8.3 done.
- Phase 4 ships `core::Mutation` and the typed mutation set.

### Files to create or modify

- `core/crates/core/src/diff/resolve.rs` — the three resolvers.

### Signatures and shapes

```rust
use crate::diff::{DriftEvent, JsonPointer};
use crate::desired_state::DesiredState;
use crate::mutation::Mutation;
use crate::snapshot::SnapshotId;

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ResolveError {
    #[error("running state could not be parsed as desired state at {path}")]
    UnparseableRunning { path: JsonPointer },
    #[error("drift event references missing snapshot {0}")]
    MissingSnapshot(SnapshotId),
}

pub fn adopt_running_state(
    event:         &DriftEvent,
    running_state: &DesiredState,
) -> Result<Mutation, ResolveError>;

pub fn reapply_desired_state(
    event:         &DriftEvent,
    desired_state: &DesiredState,
) -> Result<Mutation, ResolveError>;

pub fn defer_for_manual_reconciliation(
    event:         &DriftEvent,
) -> Mutation;
```

### Algorithm

1. `adopt_running_state` constructs `Mutation::ReplaceDesiredState { new_state: running_state.clone(), source: ResolveSource::DriftAdopt(event.correlation_id) }`. The mutation flows through Phase 7 like any other apply.
2. `reapply_desired_state` constructs `Mutation::ReapplySnapshot { snapshot_id: event.before_snapshot_id, source: ResolveSource::DriftReapply(event.correlation_id) }`. The applier renders and pushes the same desired state again.
3. `defer_for_manual_reconciliation` constructs `Mutation::DriftDeferred { event_correlation: event.correlation_id }`. This mutation is a no-op at the apply path; it produces a `config.drift-resolved` audit row (slice 8.6) with `notes.resolution = "deferred"`.
4. Each resolver produces exactly one `Mutation`.

### Tests

- `core::diff::resolve::tests::adopt_produces_replace_mutation` — assert variant.
- `core::diff::resolve::tests::reapply_targets_before_snapshot_id` — assert the snapshot id matches the event.
- `core::diff::resolve::tests::defer_produces_no_op_marker` — assert the `DriftDeferred` variant.
- `core::diff::resolve::tests::exactly_one_mutation_per_call` — invocation count is one.

### Acceptance command

`cargo test -p trilithon-core diff::resolve::tests`

### Exit conditions

- Each resolver MUST produce exactly one `Mutation`.
- The resolvers MUST be pure and synchronous.

### Audit kinds emitted

None directly. The mutation that each resolver produces flows through the standard pipeline; slice 8.6 emits `config.drift-resolved` once the resolution mutation reaches a terminal state.

### Tracing events emitted

None.

### Cross-references

- PRD T1.4.
- Architecture §7.2.

---

## Slice 8.5 — `DriftDetector` scheduler with `tokio::time::interval` and apply-in-flight skip

### Goal

Run the drift loop as a long-lived `tokio` task: tick once at startup, then every `drift_check_interval_seconds` (default 60). Each tick fetches `GET /config/`, computes the diff, and either records a drift event or returns silently. A tick that overlaps an in-flight apply is skipped with a tracing event but no audit row.

### Entry conditions

- Slices 8.1, 8.2, 8.3, 8.4 done.
- Phase 7 ships the `tokio::sync::Mutex` per `caddy_instance_id` (slice 7.6); the drift detector borrows that mutex's `try_lock` to detect in-flight applies.

### Files to create or modify

- `core/crates/adapters/src/drift.rs` — the scheduler.
- `core/crates/adapters/src/lib.rs` — re-export.
- `core/crates/cli/src/main.rs` — spawn the task during daemon bootstrap.

### Signatures and shapes

```rust
use std::sync::Arc;
use std::time::Duration;
use trilithon_core::diff::{DiffEngine, DefaultDiffEngine, DriftEvent};
use trilithon_core::caddy::CaddyClient;
use trilithon_core::storage::Storage;
use crate::audit_writer::AuditWriter;
use crate::tracing_correlation::with_correlation_span;
use ulid::Ulid;

#[derive(Clone, Debug)]
pub struct DriftDetectorConfig {
    pub interval:    Duration,        // default 60 s
    pub instance_id: String,          // 'local' for V1
}

impl Default for DriftDetectorConfig {
    fn default() -> Self { Self { interval: Duration::from_secs(60), instance_id: "local".into() } }
}

pub struct DriftDetector {
    pub config:        DriftDetectorConfig,
    pub client:        Arc<dyn CaddyClient>,
    pub diff_engine:   Arc<dyn DiffEngine>,
    pub storage:       Arc<dyn Storage>,
    pub audit:         Arc<AuditWriter>,
    pub apply_mutex:   Arc<tokio::sync::Mutex<()>>,   // shared with the applier
}

impl DriftDetector {
    pub async fn run(self, shutdown: tokio::sync::watch::Receiver<bool>) -> ();
    pub async fn tick_once(&self) -> Result<TickOutcome, TickError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TickOutcome {
    Clean,
    Drifted { event: DriftEvent },
    SkippedApplyInFlight,
}

#[derive(Debug, thiserror::Error)]
pub enum TickError {
    #[error("caddy fetch failed: {0}")]
    CaddyFetch(String),
    #[error("storage: {0}")]
    Storage(String),
    #[error("diff: {0}")]
    Diff(#[from] trilithon_core::diff::DiffError),
}
```

### Algorithm

1. `run` acquires a `tokio::time::interval(self.config.interval)`. The first tick fires immediately at startup; subsequent ticks at the configured cadence.
2. On each tick, run `with_correlation_span(Ulid::new(), "system", "drift-detector", self.tick_once())`.
3. `tick_once`:
   1. `try_lock` the apply mutex. If `Err(_)`, emit `tracing::info!(target = "drift.skipped", reason = "apply-in-flight")`, return `Ok(SkippedApplyInFlight)`. No audit row.
   2. Fetch `let running = self.client.get_running_config().await?;`. Convert to `DesiredState` via the same canonical parser used by the renderer (round-trip). Unknown fields land in `unknown_extensions`.
   3. Read the latest desired-state snapshot via `self.storage.latest_desired_state().await?`. If `None`, the detector is running before bootstrap; return `Clean`.
   4. Compute `diff = self.diff_engine.structural_diff(&desired, &running)?`. If `diff.entries.is_empty()`, return `Clean`.
   5. Compute the running-state hash (SHA-256 of canonical JSON of `running`).
   6. Build a `DriftEvent` with `before_snapshot_id`, `running_state_hash`, `diff_summary` (built from `DiffCounts`-by-`ObjectKind`), `detected_at`, `correlation_id`, `redacted_diff_json` (via the redactor in Phase 6), `redaction_sites`.
   7. Return `Ok(Drifted { event })`. Slice 8.6 handles the audit append.
4. On shutdown signal, the loop terminates cleanly within one tick.

### Tests

- `core/crates/adapters/tests/drift_clean_state_silent.rs` — start the detector against an unchanged Caddy fake; run for five ticks; assert zero `config.drift-detected` audit rows.
- `core/crates/adapters/tests/drift_out_of_band_mutation.rs` — induce a Caddy config divergence; assert `Drifted { event }` outcome on the next tick.
- `core/crates/adapters/tests/drift_skip_when_apply_in_flight.rs` — hold the apply mutex; assert `SkippedApplyInFlight` and zero audit rows.
- `core/crates/adapters/tests/drift_default_interval_is_60s.rs` — assert `DriftDetectorConfig::default().interval == Duration::from_secs(60)`.
- `core/crates/adapters/tests/drift_interval_overridable.rs` — set the interval to 1 second; assert two ticks within 2.5 seconds.

### Acceptance command

`cargo test -p trilithon-adapters drift_`

### Exit conditions

- Default interval MUST be 60 seconds and MUST be configurable.
- An apply-in-flight tick MUST be skipped without audit.
- A clean state MUST not write any audit row.

### Audit kinds emitted

None directly (slice 8.6 emits `config.drift-detected`).

### Tracing events emitted

`drift.detected` (architecture §12.1) on a non-empty diff. The skip path emits `drift.skipped` (this event SHOULD be added to architecture §12.1 in the same commit; flagged below).

### Cross-references

- PRD T1.4.
- Architecture §7.2, §9, §12.1.
- trait-signatures.md §5 `DiffEngine`, §1 `Storage::latest_desired_state`.

---

## Slice 8.6 — Drift audit row writer plus deduplication per cycle

### Goal

When `tick_once` returns `Drifted { event }`, persist exactly one `config.drift-detected` audit row per detection cycle until the drift is resolved. The detector deduplicates against the previous tick's `running_state_hash` so a single drift produces one row, not one row per tick.

### Entry conditions

- Slice 8.5 done.
- Phase 6 ships the audit writer.

### Files to create or modify

- `core/crates/adapters/src/drift.rs` — extend with the writer and the in-memory `last_running_hash` cache.

### Signatures and shapes

```rust
pub struct DriftDetector {
    /* … existing fields … */
    pub last_running_hash: tokio::sync::Mutex<Option<String>>,
}

impl DriftDetector {
    /// Called by `run` whenever `tick_once` yields `Drifted`.
    pub async fn record(&self, event: DriftEvent) -> Result<(), TickError>;

    /// Called by Phase 9's `POST /api/v1/drift/{id}/{adopt|reapply|defer}`.
    pub async fn mark_resolved(
        &self,
        correlation_id: ulid::Ulid,
        resolution:     ResolutionKind,
    ) -> Result<(), TickError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResolutionKind { Adopt, Reapply, Defer }
```

### Algorithm

1. `record(event)`:
   1. Acquire `self.last_running_hash.lock().await`.
   2. If `*guard == Some(event.running_state_hash.clone())`, return `Ok(())` (already recorded this cycle).
   3. Otherwise, write one `config.drift-detected` audit row via `AuditWriter::record` with `correlation_id = event.correlation_id`, `actor = ActorRef::System { component: "drift-detector" }`, `event = AuditEvent::DriftDetected`, `snapshot_id = Some(event.before_snapshot_id)`, `redacted_diff_json = Some(event.redacted_diff_json)`, `notes = Some(serde_json::to_string(&event.diff_summary)?)`.
   4. Also persist the row in the typed drift table via `self.storage.record_drift_event(event).await?`.
   5. Update `*guard = Some(event.running_state_hash.clone())`.
2. `mark_resolved(correlation_id, resolution)`:
   1. Write one `config.drift-resolved` audit row with `correlation_id`, `notes = Some(serde_json::json!({ "resolution": resolution }).to_string())`.
   2. Reset `*self.last_running_hash.lock().await = None;` so the next tick re-evaluates.

### Tests

- `core/crates/adapters/tests/drift_records_one_row_per_cycle.rs` — induce drift; run 10 ticks against the same divergence; assert exactly one `config.drift-detected` audit row.
- `core/crates/adapters/tests/drift_records_two_rows_for_two_cycles.rs` — induce drift, resolve via `mark_resolved`, induce a different drift; assert two `config.drift-detected` rows separated by one `config.drift-resolved` row.
- `core/crates/adapters/tests/drift_resolution_paths.rs` — call `mark_resolved` with each `ResolutionKind`; assert one `config.drift-resolved` row per call carrying the matching `notes.resolution`.
- `core/crates/adapters/tests/drift_never_silently_overwrites.rs` — assert the detector never invokes `CaddyClient::load_config` or `CaddyClient::patch_config` directly. Implementation: a `Storage` double that records all calls to a sibling `CaddyClient` double; the detector's mock `CaddyClient` records zero mutating calls.

### Acceptance command

`cargo test -p trilithon-adapters drift_records_ drift_resolution_paths drift_never_silently_overwrites`

### Exit conditions

- A non-empty diff MUST produce exactly one `config.drift-detected` audit row per detection cycle until resolved.
- Each `mark_resolved` call MUST produce exactly one `config.drift-resolved` audit row.
- The detector MUST NOT mutate Caddy directly.

### Audit kinds emitted

`config.drift-detected`, `config.drift-resolved` (architecture §6.6).

### Tracing events emitted

`drift.detected`, `drift.resolved` (architecture §12.1).

### Cross-references

- PRD T1.4.
- Architecture §6.6, §7.2.
- trait-signatures.md §1 `Storage::record_drift_event`, `latest_drift_event`.

---

## Phase exit checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] A non-empty diff produces exactly one `config.drift-detected` audit row per detection cycle until resolved (slice 8.6).
- [ ] Drift detection does not silently overwrite Caddy (slice 8.6 enforcement test).
- [ ] The three resolution paths are implemented and exercised by integration tests (slices 8.4, 8.6).
- [ ] The default detection interval is 60 seconds and is configuration-overridable (slice 8.5).
- [ ] `core/README.md` describes the detector schedule, the three resolution paths, and the "never silently overwrite" invariant.

## Open questions

- The `drift.skipped` tracing event in slice 8.5 is not yet listed in architecture §12.1. The slice flags adding it; the prompt forbids inventing names silently.
- The classifier in slice 8.3 maps JsonPointers to `ObjectKind` via static prefixes; future Caddy modules MAY introduce paths that miss the classifier and fall through to `ObjectKind::Other`. Whether `Other` is acceptable or should be an error is unresolved and is filed here.
