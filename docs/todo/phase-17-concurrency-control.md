# Phase 17 — Concurrency-control surface — Implementation Slices

> Phase reference: [../phases/phase-17-concurrency-control.md](../phases/phase-17-concurrency-control.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference: `docs/phases/phase-17-concurrency-control.md`.
- Architecture §4.1, §4.2, §4.3 (component view), §6.5 (`snapshots`), §6.6 (audit-kind vocabulary), §6.7 (`mutations`), §7.1 (mutation lifecycle), §9 (concurrency model), §11 (security posture), §12.1 (tracing vocabulary).
- Trait signatures: `Storage`, `SnapshotStore`, `Reconciler`, `ToolGateway` (the `ConflictError` shape MUST appear verbatim on the gateway boundary).
- ADRs: ADR-0009 (immutable content-addressed snapshots and audit log), ADR-0012 (optimistic concurrency on monotonic `config_version`).
- PRD: T2.10 (concurrency control), T1.3 (rollback), T1.7 (audit log).
- Hazards: H8 (concurrent modification).

## Slice plan summary

| # | Slice | Primary files | Effort (h) | Depends on |
|---|-------|---------------|------------|------------|
| 17.1 | Conflict and rebase types | `crates/core/src/concurrency.rs` | 4 | — |
| 17.2 | Rebase planner (pure) | `crates/core/src/concurrency/rebase.rs` | 8 | 17.1 |
| 17.3 | Three-way diff and resolution apply | `crates/core/src/concurrency/diff.rs`, `crates/core/src/concurrency/rebase.rs` | 6 | 17.2 |
| 17.4 | Snapshot writer compare-and-swap and rebase-token store | `crates/adapters/src/snapshot_store.rs`, `crates/adapters/src/rebase_token_store.rs`, `crates/core/src/config.rs` | 8 | 17.1 |
| 17.5 | Conflict HTTP envelope and audit kinds | `crates/cli/src/http/mutations/conflict.rs`, `crates/core/src/audit.rs` | 6 | 17.4 |
| 17.6 | `POST /api/v1/mutations/rebase` endpoint | `crates/cli/src/http/mutations/rebase.rs` | 6 | 17.3, 17.5 |
| 17.7 | Conflict banner and rebase view (web UI) | `web/src/features/concurrency/*`, `web/src/components/diff/ThreeWayDiff.tsx` | 8 | 17.5, 17.6 |
| 17.8 | End-to-end concurrency scenarios | `crates/adapters/tests/concurrency_*.rs` | 6 | 17.6, 17.7 |

---

## Slice 17.1 [standard] — Conflict and rebase types

### Goal

Land the pure type surface that the rest of the phase consumes. Trilithon defines `ConflictError`, `ActorRef`, `MutationCommutativity`, `RebasePlan`, `ThreeWayDiff`, `FieldConflict`, `FieldResolution`, `FieldPath`, `RebaseError`, and the in-memory `RebaseToken` record. No I/O, no async, no storage. The wire shape MUST round-trip through `serde_json` and the `FieldPath` MUST serialise as a JSON Pointer string.

### Entry conditions

- Phase 16 complete (Tier 1 hardened).
- The `crates/core` crate compiles with `cargo build -p trilithon-core`.
- `core::mutation::TypedMutation` and `core::desired_state::DesiredState` exist from Phase 4.

### Files to create or modify

- `core/crates/core/src/concurrency.rs` — new module with the public type surface.
- `core/crates/core/src/lib.rs` — add `pub mod concurrency;`.

### Signatures and shapes

```rust
//! `core/crates/core/src/concurrency.rs`

use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::desired_state::DesiredState;
use crate::mutation::TypedMutation;
use crate::time::UnixSeconds;
use crate::validation::ValidationErrorSet;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ActorRef {
    User { id: String, username: String },
    Token { id: String, name: String },
    System { component: String },
}

pub type ActorId = String;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConflictError {
    pub current_version: i64,
    pub attempted_version: i64,
    pub conflicting_snapshot_id: String,
    pub conflicting_actor: ActorRef,
    pub current_desired_state: DesiredState,
    pub attempted_mutation: TypedMutation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "kebab-case")]
pub enum MutationCommutativity {
    Commutative,
    Conflicting { conflicts: Vec<FieldConflict> },
    Identical,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RebasePlan {
    pub conflicting_snapshot_id: String,
    pub base_version: i64,
    pub head_version: i64,
    pub commutativity: MutationCommutativity,
    pub three_way: ThreeWayDiff,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ThreeWayDiff {
    pub base: serde_json::Value,
    pub theirs: serde_json::Value,
    pub mine: serde_json::Value,
    pub conflicts: Vec<FieldConflict>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FieldConflict {
    pub path: FieldPath,
    pub base_value: serde_json::Value,
    pub theirs_value: serde_json::Value,
    pub mine_value: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "choice", rename_all = "kebab-case")]
pub enum FieldResolution {
    Theirs { path: FieldPath },
    Mine { path: FieldPath },
    Custom { path: FieldPath, value: serde_json::Value },
}

/// JSON-Pointer encoded field path (RFC 6901). The string form is the
/// canonical wire form; the `segments()` accessor returns the decoded
/// path components.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct FieldPath(String);

impl FieldPath {
    pub fn from_pointer(pointer: impl Into<String>) -> Result<Self, FieldPathError> { /* ... */ unimplemented!() }
    pub fn as_pointer(&self) -> &str { &self.0 }
    pub fn segments(&self) -> Vec<String> { /* RFC 6901 unescape */ unimplemented!() }
}

impl Serialize for FieldPath {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> { s.serialize_str(&self.0) }
}

impl<'de> Deserialize<'de> for FieldPath {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> { /* ... */ unimplemented!() }
}

#[derive(Debug, thiserror::Error)]
pub enum FieldPathError {
    #[error("JSON Pointer must start with '/' or be empty: {0:?}")]
    InvalidStart(String),
    #[error("invalid escape in JSON Pointer: {0:?}")]
    InvalidEscape(String),
}

#[derive(Debug, thiserror::Error)]
pub enum RebaseError {
    #[error("rebase token expired")]
    TokenExpired,
    #[error("rebase token already consumed")]
    TokenConsumed,
    #[error("a third actor moved the head; latest_version={latest_version}")]
    ConflictAppeared { latest_version: i64 },
    #[error("resolution invalid at {path:?}: {reason}")]
    ResolutionInvalid { path: FieldPath, reason: String },
    #[error("validation failed")]
    ValidationFailed(ValidationErrorSet),
}

pub type RebaseTokenId = Ulid;
pub type SnapshotId = String;

#[derive(Clone, Debug)]
pub struct RebaseToken {
    pub id: RebaseTokenId,
    pub conflicting_snapshot_id: SnapshotId,
    pub base_version: u64,
    pub head_version: u64,
    pub actor: ActorId,
    pub plan: RebasePlan,
    pub created_at: UnixSeconds,
    pub expires_at: UnixSeconds,
}
```

### Algorithm

1. `FieldPath::from_pointer` validates that the input is empty (root) or starts with `/`. Each segment between `/` is unescaped per RFC 6901: `~1` becomes `/`, `~0` becomes `~`. Any other `~` is `InvalidEscape`.
2. `FieldPath::segments` performs the inverse: split on unescaped `/`, then unescape each segment.
3. The `Serialize`/`Deserialize` impls for `FieldPath` use the string form directly, so `serde_json` round-trips through `String`.

### Tests

- `core/crates/core/src/concurrency.rs` `mod tests`:
  - `actor_ref_user_round_trips_through_serde_json`.
  - `conflict_error_round_trips_through_serde_json` — populate a `ConflictError` with a representative `DesiredState` and `TypedMutation`, serialise to JSON, deserialise, assert `Eq`.
  - `field_path_round_trips_through_json_pointer` — inputs `""`, `"/routes/0/match/host/0"`, `"/headers/X-Foo~1Bar"`, asserting `as_pointer` returns the input unchanged after deserialisation.
  - `field_path_rejects_invalid_escape` — input `"/foo~2bar"` returns `FieldPathError::InvalidEscape`.
  - `field_path_rejects_invalid_start` — input `"foo"` returns `FieldPathError::InvalidStart`.
  - `mutation_commutativity_serialises_kebab_tag` — assert `{"verdict":"conflicting","conflicts":[...]}` shape.
  - `rebase_error_variants_construct` — instantiate every variant and `Display`.
  - `rebase_token_clone_preserves_fields` — assert clone equality via field-by-field comparison.

### Acceptance command

```
cargo test -p trilithon-core concurrency::tests
```

### Exit conditions

- `cargo build -p trilithon-core` succeeds.
- All eight tests above pass.
- `serde_json::to_string(&ConflictError { .. })` produces a JSON object with the documented field names.
- `FieldPath` serialises as a bare JSON string, never as an object or array.

### Audit kinds emitted

None in this slice (pure types). The kinds added in §6.6 by Phase 17 (`mutation.conflicted`, `mutation.rebased.auto`, `mutation.rebased.manual`, `mutation.rebase.expired`, `mutation.rejected.missing-expected-version`) are introduced in slice 17.5.

### Tracing events emitted

None in this slice.

### Cross-references

- ADR-0012 (optimistic concurrency).
- Architecture §6.5, §9.
- Phase 17 task: "Define `ConflictError` and `ActorRef`," "Define `MutationCommutativity`, `RebasePlan`, `ThreeWayDiff`, `FieldConflict`, `FieldResolution`, `FieldPath`," "Define `RebaseError`," "Define the in-memory `RebaseToken` record."

---

## Slice 17.2 [standard] — Rebase planner (pure)

### Goal

Implement `plan_rebase`: given a base `DesiredState`, the new head `DesiredState` ("theirs"), and the actor's pending `TypedMutation` ("mine"), classify the relationship as `Commutative`, `Conflicting { conflicts }`, or `Identical`, and produce a `RebasePlan`. Pure function, no I/O, no SQLite, no async.

### Entry conditions

- Slice 17.1 shipped.
- `core::mutation::effective_field_set(&TypedMutation) -> BTreeSet<FieldPath>` exists or is added in this slice; this slice MAY add it as a pure helper.

### Files to create or modify

- `core/crates/core/src/concurrency/rebase.rs` — new module with `plan_rebase`.
- `core/crates/core/src/concurrency.rs` — re-export `pub mod rebase; pub use rebase::plan_rebase;`.
- `core/crates/core/src/mutation.rs` — add `pub fn effective_field_set(&self) -> BTreeSet<FieldPath>` on `TypedMutation`.

### Signatures and shapes

```rust
//! `core/crates/core/src/concurrency/rebase.rs`

use crate::concurrency::{
    FieldConflict, FieldPath, MutationCommutativity, RebasePlan, ThreeWayDiff,
};
use crate::desired_state::DesiredState;
use crate::mutation::TypedMutation;

/// Pure rebase planner. Classifies the actor's `mine` mutation against
/// the head state `theirs`, given the common ancestor `base`.
pub fn plan_rebase(
    base:   &DesiredState,
    theirs: &DesiredState,
    mine:   &TypedMutation,
) -> RebasePlan;
```

```rust
//! Addition to `core/crates/core/src/mutation.rs`

impl TypedMutation {
    /// The set of JSON-Pointer paths into `DesiredState` that this mutation
    /// would write. Used by the rebase planner to detect commutativity.
    pub fn effective_field_set(&self) -> std::collections::BTreeSet<FieldPath>;
}
```

### Algorithm

1. Compute `theirs_diff = field_set(base, theirs)` — every JSON-Pointer path whose value differs between `base` and `theirs`.
2. Compute `mine_fields = mine.effective_field_set()`.
3. If `mine_fields.is_disjoint(&theirs_diff)`: return `RebasePlan { commutativity: Commutative, three_way: { base, theirs, mine: <projection of mine onto base>, conflicts: [] }, ... }`.
4. Else compute `intersection = mine_fields ∩ theirs_diff`. For each path in the intersection, build a `FieldConflict { path, base_value, theirs_value, mine_value }`.
5. If every `FieldConflict` has `theirs_value == mine_value`: return `MutationCommutativity::Identical` (the actor and the head writer chose the same value).
6. Else return `MutationCommutativity::Conflicting { conflicts }`.
7. Always populate `three_way.base`, `three_way.theirs`, `three_way.mine`, `three_way.conflicts` so the UI can render the diff regardless of verdict.
8. `base_version` and `head_version` are read from the snapshot metadata threaded into the call site (the caller fills these; the planner is generic over `DesiredState`).

### Tests

- `core/crates/core/src/concurrency/rebase.rs` `mod tests`:
  - `commutative_disjoint_fields_yields_commutative` — `mine` adds a header to route A; `theirs` changes the upstream of route B. Verdict `Commutative`, `conflicts.is_empty()`.
  - `same_field_different_value_yields_conflicting` — both mutate route A's upstream to different ports.
  - `same_field_same_value_yields_identical` — both mutate route A's upstream to the identical port.
  - `multi_field_partial_overlap_yields_conflicting_with_subset` — `mine` touches `/routes/0/match` and `/routes/0/upstream`; `theirs` touched only `/routes/0/upstream`. Conflicts list has one entry, `match` is in the auto-mergeable set.
  - `proptest_commutative_implies_disjoint` — generate random pairs of `TypedMutation` that pass a disjoint-fields predicate and assert the planner agrees; generate random pairs that fail and assert the planner agrees.

### Acceptance command

```
cargo test -p trilithon-core concurrency::rebase::tests
```

### Exit conditions

- All five tests pass.
- The `proptest` harness runs at least 256 cases without falsification.
- `plan_rebase` does not call any I/O function; a `clippy::disallowed_methods` rule for `std::fs`, `tokio`, and `rusqlite` symbols passes against the module.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- Phase 17 task: "Implement `plan_rebase`."
- ADR-0012.
- Architecture §9.

---

## Slice 17.3 [standard] — Three-way diff and resolution apply

### Goal

Implement `apply_resolutions`: given a `RebasePlan` and a slice of `FieldResolution` entries, produce a single `TypedMutation` ready for resubmission. Validation: every `FieldConflict` in the plan MUST have a corresponding resolution; missing resolutions yield `RebaseError::ResolutionInvalid`. Custom values MUST type-check against the field's expected schema.

### Entry conditions

- Slice 17.2 shipped.
- `core::mutation::TypedMutation::with_field(path, value)` exists or is added in this slice.

### Files to create or modify

- `core/crates/core/src/concurrency/rebase.rs` — extend with `apply_resolutions`.
- `core/crates/core/src/concurrency/diff.rs` — new module with the JSON-Pointer apply helper.
- `core/crates/core/src/mutation.rs` — add `pub fn with_field(&self, path: &FieldPath, value: serde_json::Value) -> Result<TypedMutation, MutationFieldError>`.

### Signatures and shapes

```rust
//! Addition to `core/crates/core/src/concurrency/rebase.rs`

use crate::concurrency::{FieldResolution, RebaseError, RebasePlan};
use crate::mutation::TypedMutation;

/// Take a planned rebase and the actor's resolutions, return a single
/// re-targeted mutation. Pure; the caller submits the result through the
/// standard mutation pipeline.
pub fn apply_resolutions(
    plan:        &RebasePlan,
    mine:        &TypedMutation,
    resolutions: &[FieldResolution],
) -> Result<TypedMutation, RebaseError>;
```

```rust
//! `core/crates/core/src/concurrency/diff.rs`

use crate::concurrency::FieldPath;

/// Set the value at `path` inside `target`. Creates intermediate objects
/// as needed; returns an error if a path component crosses a non-object.
pub fn json_pointer_set(
    target: &mut serde_json::Value,
    path:   &FieldPath,
    value:  serde_json::Value,
) -> Result<(), JsonPointerError>;

#[derive(Debug, thiserror::Error)]
pub enum JsonPointerError {
    #[error("path component {component:?} traverses a non-object")]
    NotObject { component: String },
    #[error("array index {index} out of bounds (len={len})")]
    OutOfBounds { index: usize, len: usize },
}
```

### Algorithm

1. Build `expected_paths: BTreeSet<FieldPath>` from `plan.three_way.conflicts`.
2. Build `provided_paths: BTreeSet<FieldPath>` from `resolutions`.
3. If `expected_paths != provided_paths`, return `RebaseError::ResolutionInvalid { path: <first missing or extra>, reason: <"missing"|"unexpected"> }`.
4. Start `result = mine.clone()`.
5. For each `FieldResolution`:
   - `Theirs { path }` → look up `theirs_value` for `path` in `plan.three_way.conflicts`, call `result.with_field(&path, theirs_value)`.
   - `Mine { path }` → no-op (the value is already in `mine`).
   - `Custom { path, value }` → call `result.with_field(&path, value)`. Any `MutationFieldError` is wrapped as `RebaseError::ResolutionInvalid`.
6. Return `Ok(result)`.

### Tests

- `core/crates/core/src/concurrency/rebase.rs` `mod tests`:
  - `apply_resolutions_full_theirs_keeps_their_values`.
  - `apply_resolutions_full_mine_keeps_my_values`.
  - `apply_resolutions_custom_value_replaces`.
  - `apply_resolutions_missing_resolution_returns_invalid`.
  - `apply_resolutions_extra_resolution_returns_invalid`.
- `core/crates/core/src/concurrency/diff.rs` `mod tests`:
  - `json_pointer_set_root_object`.
  - `json_pointer_set_nested_object_creates_path`.
  - `json_pointer_set_array_index_in_bounds`.
  - `json_pointer_set_array_index_out_of_bounds`.
  - `json_pointer_set_traverses_non_object_errors`.

### Acceptance command

```
cargo test -p trilithon-core concurrency::
```

### Exit conditions

- All ten tests pass.
- `apply_resolutions` rejects partial inputs with a typed error.
- The merged `TypedMutation` carries a fresh `expected_version` field consumed by the caller; the planner does not invent the version.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- Phase 17 task: "Implement `apply_resolutions`."
- Architecture §9.

---

## Slice 17.4 [cross-cutting] — Snapshot writer compare-and-swap and rebase-token store

### Goal

Wire `ConflictError` into the snapshot writer's `BEGIN IMMEDIATE` transaction so a stale `expected_version` returns `SnapshotWriteError::Conflict(ConflictError)` carrying the head's `DesiredState` and the attempted `TypedMutation`. Add the in-memory `RebaseTokenStore` (DashMap-backed, sweep-on-call expiry, never persisted). Add the `[concurrency] rebase_token_ttl_minutes` configuration knob with bounds-checking validator.

### Entry conditions

- Slice 17.1 shipped.
- The `snapshots` table has the `UNIQUE INDEX snapshots_config_version (caddy_instance_id, config_version)` from Phase 5.
- `core::config::Config` exists from Phase 1.

### Files to create or modify

- `core/crates/adapters/src/snapshot_store.rs` — extend `insert_if_absent` to detect stale `expected_version`.
- `core/crates/adapters/src/rebase_token_store.rs` — new module.
- `core/crates/adapters/src/lib.rs` — export `rebase_token_store`.
- `core/crates/core/src/config.rs` — add `concurrency.rebase_token_ttl_minutes`.
- `core/crates/core/src/error.rs` — add `ConfigError::OutOfRange { field, value, min, max }` if not already present.

### Signatures and shapes

```rust
//! Extension to `core/crates/adapters/src/snapshot_store.rs`

use trilithon_core::concurrency::ConflictError;
use trilithon_core::desired_state::DesiredState;
use trilithon_core::mutation::TypedMutation;

#[derive(Debug, thiserror::Error)]
pub enum SnapshotWriteError {
    #[error("conflict: head moved")]
    Conflict(Box<ConflictError>),
    #[error("storage error: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("integrity: {0}")]
    Integrity(String),
}

pub enum SnapshotInsertOutcome {
    Inserted { id: String, config_version: i64 },
    Idempotent { existing_id: String },
}

impl SnapshotStore {
    pub fn insert_if_absent(
        &self,
        desired:          &DesiredState,
        attempted:        &TypedMutation,
        expected_version: i64,
        actor:            &ActorRef,
    ) -> Result<SnapshotInsertOutcome, SnapshotWriteError>;
}
```

```rust
//! `core/crates/adapters/src/rebase_token_store.rs`

use std::time::Duration;
use dashmap::DashMap;
use trilithon_core::concurrency::{ActorId, RebasePlan, RebaseToken, RebaseTokenId};
use trilithon_core::time::{Clock, UnixSeconds};

#[derive(Debug, thiserror::Error)]
pub enum RebaseTokenError {
    #[error("token not found")]
    NotFound,
    #[error("token expired")]
    Expired,
    #[error("token already consumed")]
    Consumed,
}

pub struct RebaseTokenStore<C: Clock> {
    map:   DashMap<RebaseTokenId, RebaseToken>,
    clock: C,
}

impl<C: Clock> RebaseTokenStore<C> {
    pub fn new(clock: C) -> Self;

    /// Issue a new rebase token. Sweeps expired tokens before insertion.
    pub fn issue(
        &self,
        plan:  RebasePlan,
        actor: ActorId,
        ttl:   Duration,
    ) -> RebaseTokenId;

    /// Atomic remove + return. Sweeps expired tokens first. Double-consume
    /// returns `RebaseTokenError::NotFound`.
    pub fn consume(
        &self,
        id: RebaseTokenId,
    ) -> Result<RebaseToken, RebaseTokenError>;

    /// Inspect without consuming. Sweeps first.
    pub fn peek(
        &self,
        id: RebaseTokenId,
    ) -> Result<RebaseToken, RebaseTokenError>;

    fn sweep(&self, now: UnixSeconds);
}
```

```rust
//! Addition to `core/crates/core/src/config.rs`

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize)]
pub struct ConcurrencyConfig {
    /// Time-to-live for outstanding rebase tokens, in minutes.
    /// Bounds: minimum 5, maximum 1440. Default 30.
    #[serde(default = "ConcurrencyConfig::default_ttl_minutes")]
    pub rebase_token_ttl_minutes: u32,
}

impl ConcurrencyConfig {
    pub const DEFAULT_TTL_MINUTES: u32 = 30;
    pub const MIN_TTL_MINUTES:     u32 = 5;
    pub const MAX_TTL_MINUTES:     u32 = 1440;

    fn default_ttl_minutes() -> u32 { Self::DEFAULT_TTL_MINUTES }

    pub fn validate(&self) -> Result<(), crate::error::ConfigError>;
}
```

### Algorithm

`SnapshotStore::insert_if_absent`:

1. Open a `BEGIN IMMEDIATE` transaction.
2. `SELECT MAX(config_version), id, actor_kind, actor_id FROM snapshots WHERE caddy_instance_id = ?1` → `(current_version, head_snapshot_id, head_actor_kind, head_actor_id)`.
3. If `current_version != expected_version`:
   - Load the head `DesiredState` from `head_snapshot_id`.
   - Construct `ConflictError { current_version, attempted_version: expected_version + 1, conflicting_snapshot_id: head_snapshot_id, conflicting_actor: <ActorRef from head_actor_kind/id>, current_desired_state: <head>, attempted_mutation: attempted.clone() }`.
   - Roll back; return `Err(SnapshotWriteError::Conflict(Box::new(...)))`.
4. Compute `new_version = current_version + 1`.
5. Compute `id = blake3(serde_json::to_vec(&desired)?)`.
6. `INSERT OR IGNORE INTO snapshots ...`. If `INSERT OR IGNORE` returns 0 rows (duplicate `id`), return `SnapshotInsertOutcome::Idempotent { existing_id: id }`.
7. Commit; return `SnapshotInsertOutcome::Inserted { id, config_version: new_version }`.

`RebaseTokenStore::sweep`:

1. Walk `self.map.iter()`; collect `id`s where `expires_at < now`.
2. For each collected `id`, call `self.map.remove(&id)`.

`RebaseTokenStore::issue`:

1. `now = clock.now()`.
2. `self.sweep(now)`.
3. `id = Ulid::new()`.
4. `expires_at = now + ttl.as_secs()`.
5. `self.map.insert(id, RebaseToken { id, ..., created_at: now, expires_at })`.
6. Return `id`.

`RebaseTokenStore::consume`:

1. `now = clock.now()`.
2. `self.sweep(now)`.
3. `match self.map.remove(&id) { Some((_, token)) if token.expires_at >= now => Ok(token), Some(_) => Err(Expired), None => Err(NotFound) }`.
4. The `dashmap::DashMap::remove` is atomic, so a second consumer observes `None` even under contention.

`ConcurrencyConfig::validate`:

1. If `rebase_token_ttl_minutes < MIN_TTL_MINUTES` or `> MAX_TTL_MINUTES`, return `ConfigError::OutOfRange { field: "concurrency.rebase_token_ttl_minutes".into(), value: self.rebase_token_ttl_minutes as i64, min: MIN_TTL_MINUTES as i64, max: MAX_TTL_MINUTES as i64 }`.

### Tests

- `core/crates/adapters/tests/snapshot_store_conflict.rs`:
  - `insert_with_stale_expected_version_returns_conflict`.
  - `insert_with_correct_expected_version_inserts_and_advances`.
  - `concurrent_inserts_one_winner_one_conflict` — spawn two threads, assert exactly one `Inserted` and one `Conflict`.
  - `idempotent_insert_of_identical_desired_state_returns_idempotent`.
- `core/crates/adapters/tests/rebase_token_store.rs`:
  - `issue_then_consume_returns_token`.
  - `double_consume_returns_not_found`.
  - `expired_token_consume_returns_expired`.
  - `sweep_on_call_removes_expired_entries`.
  - `daemon_restart_invalidates_all_tokens` — instantiate a fresh store; assert `peek` returns `NotFound` for every previously-issued id.
- `core/crates/core/src/config.rs` `mod tests`:
  - `concurrency_default_is_thirty`.
  - `concurrency_in_bounds_accepts`.
  - `concurrency_lower_boundary_five_accepts`.
  - `concurrency_upper_boundary_1440_accepts`.
  - `concurrency_below_min_rejects`.
  - `concurrency_above_max_rejects`.
- `core/crates/cli/tests/daemon_refuses_invalid_ttl.rs`:
  - `daemon_refuses_to_start_with_ttl_below_min` — write a config TOML with `rebase_token_ttl_minutes = 4`; spawn the daemon; assert exit code matches the existing configuration-error code; assert stderr carries the typed `OutOfRange` field name.

### Acceptance command

```
cargo test -p trilithon-adapters --test snapshot_store_conflict --test rebase_token_store && \
cargo test -p trilithon-core config::tests && \
cargo test -p trilithon-cli --test daemon_refuses_invalid_ttl
```

### Exit conditions

- All thirteen tests pass.
- The daemon refuses to start with an out-of-range TTL.
- The token store contains zero rows on a fresh process; no SQLite migration adds a `rebase_tokens` table.
- The conflict body carries the head `DesiredState` verbatim.

### Audit kinds emitted

None directly in this slice. The conflict path's audit row (`mutation.conflicted` per architecture §6.6) is written by the HTTP handler in slice 17.5.

### Tracing events emitted

None new. Existing `apply.failed` continues to fire on the failure path; this slice does not introduce a new event.

### Cross-references

- ADR-0012, ADR-0009.
- Architecture §6.5, §9.
- Trait signatures: `SnapshotStore`.
- Phase 17 tasks: "Wire `ConflictError` into the snapshot writer," "Implement `RebaseTokenStore` as an in-memory `DashMap`," "Add the `rebase_token_ttl_minutes` configuration knob."

---

## Slice 17.5 [cross-cutting] — Conflict HTTP envelope and audit kinds

### Goal

Surface the existing 409 conflict on every mutation endpoint with the typed `RebasePlan` body. Add the five new audit kinds (`mutation.conflicted`, `mutation.rebased.auto`, `mutation.rebased.manual`, `mutation.rebase.expired`, `mutation.rejected.missing-expected-version`). Write the audit row on each conflict transition.

### Entry conditions

- Slice 17.4 shipped.
- The HTTP mutation handler from Phase 9 already enforces 400 on missing `expected_version` and 409 on stale `expected_version`. This slice only changes the 409 body shape and adds audit emission.

### Files to create or modify

- `core/crates/cli/src/http/mutations/conflict.rs` — new module producing the typed conflict body.
- `core/crates/cli/src/http/mutations/mod.rs` — call the new helper on the conflict path.
- `core/crates/core/src/audit.rs` — add five `AuditEvent` variants.
- `core/crates/adapters/src/audit_log_store.rs` — no schema change; the regex check accepts the new kebab-case kinds because they match the existing regex.

### Signatures and shapes

```rust
//! `core/crates/cli/src/http/mutations/conflict.rs`

use axum::http::StatusCode;
use serde::Serialize;
use trilithon_core::concurrency::{ConflictError, RebasePlan, RebaseTokenId};

#[derive(Debug, Serialize)]
pub struct ConflictResponseBody {
    pub kind: &'static str, // "conflict"
    pub current_version: i64,
    pub attempted_version: i64,
    pub conflicting_snapshot_id: String,
    pub conflicting_actor: trilithon_core::concurrency::ActorRef,
    pub rebase_token: String,        // ULID, lower-case
    pub rebase_plan: RebasePlan,
}

pub fn render_conflict(
    err:   &ConflictError,
    token: RebaseTokenId,
    plan:  RebasePlan,
) -> (StatusCode, ConflictResponseBody);
```

```rust
//! Addition to `core/crates/core/src/audit.rs`

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum AuditEvent {
    // ... existing variants ...
    MutationConflicted,
    MutationRebasedAuto,
    MutationRebasedManual,
    MutationRebaseExpired,
    MutationRejectedMissingExpectedVersion,
}

impl std::fmt::Display for AuditEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            // ...
            Self::MutationConflicted                     => "mutation.conflicted",
            Self::MutationRebasedAuto                    => "mutation.rebased.auto",
            Self::MutationRebasedManual                  => "mutation.rebased.manual",
            Self::MutationRebaseExpired                  => "mutation.rebase.expired",
            Self::MutationRejectedMissingExpectedVersion => "mutation.rejected.missing-expected-version",
        };
        f.write_str(s)
    }
}
```

### Algorithm

`render_conflict`:

1. Build `ConflictResponseBody { kind: "conflict", current_version: err.current_version, attempted_version: err.attempted_version, conflicting_snapshot_id: err.conflicting_snapshot_id.clone(), conflicting_actor: err.conflicting_actor.clone(), rebase_token: token.to_string(), rebase_plan: plan }`.
2. Return `(StatusCode::CONFLICT, body)`.

Mutation handler conflict branch (in `mutations/mod.rs`):

1. On `SnapshotWriteError::Conflict(err)`, call `plan_rebase(&err.current_desired_state, &theirs, &err.attempted_mutation)` where `theirs` is the head snapshot's `DesiredState` (already on `err`).
2. `let token = self.rebase_token_store.issue(plan.clone(), actor, Duration::from_secs(ttl_minutes * 60))`.
3. `record_audit_event(AuditEvent::MutationConflicted, notes: { "head_snapshot_id": ..., "attempted_version": ..., "current_version": ... })`.
4. Return `render_conflict(&err, token, plan)` as the HTTP response.

### Tests

- `core/crates/core/src/audit.rs` `mod tests`:
  - `display_uses_dotted_kebab` — assert each new variant's `Display` returns the table value verbatim.
  - `kind_strings_are_unique_and_in_section_6_6` — assert the five new kinds are distinct and match `/^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+$/`.
- `core/crates/cli/tests/http_mutations_conflict.rs`:
  - `stale_expected_version_returns_409_with_typed_body` — submit a mutation with the wrong `expected_version`, assert status 409 and the body matches `ConflictResponseBody`.
  - `missing_expected_version_returns_400_and_writes_audit` — submit a mutation without `expected_version`, assert 400 and exactly one audit row with `kind = "mutation.rejected.missing-expected-version"`.
  - `conflict_emits_mutation_conflicted_audit` — assert exactly one audit row with `kind = "mutation.conflicted"` after a 409.

### Acceptance command

```
cargo test -p trilithon-cli --test http_mutations_conflict && \
cargo test -p trilithon-core audit::tests
```

### Exit conditions

- The five new audit kinds appear in the §6.6 table; this slice updates `architecture.md` if a kind is missing.
- Every conflict response carries `rebase_token` and `rebase_plan`.
- Every conflict transition writes exactly one audit row.

### Audit kinds emitted

Per §6.6: `mutation.conflicted`, `mutation.rejected.missing-expected-version`. The remaining three kinds (`mutation.rebased.auto`, `mutation.rebased.manual`, `mutation.rebase.expired`) are introduced here in the enum but written from slice 17.6.

### Tracing events emitted

`http.request.received` and `http.request.completed` per §12.1, fired by the existing HTTP middleware. No new event names.

### Cross-references

- ADR-0009.
- Architecture §6.6, §7.1.
- Phase 17 tasks: "Surface the existing `expected_version` mismatch as a UI-visible conflict resolution flow," "Return typed `409 Conflict` with `RebasePlan`," "Add audit kinds."

---

## Slice 17.6 [standard] — `POST /api/v1/mutations/rebase` endpoint

### Goal

Implement the rebase submission endpoint. Body: `{ rebase_token: String, resolutions: Vec<FieldResolution> }`. Responses: 200 (`{ mutation_id, new_version }`), 409 (third-actor race), 410 (expired or consumed token), 422 (validation failed). The handler runs the merged result through the standard validation pipeline before submission.

### Entry conditions

- Slices 17.3, 17.4, 17.5 shipped.
- The standard mutation validation pipeline is callable as `validate_mutation(&TypedMutation, &Capabilities, &Policies) -> Result<(), ValidationErrorSet>`.

### Files to create or modify

- `core/crates/cli/src/http/mutations/rebase.rs` — new handler.
- `core/crates/cli/src/http/router.rs` — mount `POST /api/v1/mutations/rebase`.

### Signatures and shapes

```rust
//! `core/crates/cli/src/http/mutations/rebase.rs`

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use trilithon_core::concurrency::FieldResolution;

#[derive(Debug, Deserialize)]
pub struct RebaseRequest {
    pub rebase_token: String,            // ULID
    pub resolutions:  Vec<FieldResolution>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RebaseResponseBody {
    Ok { mutation_id: String, new_version: i64 },
    Conflict { rebase_token: String, rebase_plan: trilithon_core::concurrency::RebasePlan, current_version: i64 },
    Gone { reason: &'static str }, // "rebase-token-expired" | "rebase-token-consumed"
    Unprocessable { errors: trilithon_core::validation::ValidationErrorSet },
}

pub async fn submit_rebase(
    State(app): State<AppState>,
    Json(req):  Json<RebaseRequest>,
) -> (StatusCode, Json<RebaseResponseBody>);
```

### Algorithm

1. Parse `req.rebase_token` as `Ulid`. If parse fails, return `(StatusCode::BAD_REQUEST, ...)` with a typed error body (existing 400 path).
2. `let token = app.rebase_token_store.consume(id)`. On `Err(Expired)` return `(StatusCode::GONE, RebaseResponseBody::Gone { reason: "rebase-token-expired" })` and write `AuditEvent::MutationRebaseExpired`. On `Err(NotFound | Consumed)` return `(StatusCode::GONE, ..., reason: "rebase-token-consumed")`.
3. `let merged = apply_resolutions(&token.plan, &mine_from_token, &req.resolutions)?`. Errors map: `RebaseError::ResolutionInvalid` → 422; other variants → 422.
4. Run `validate_mutation(&merged, ...)?`. On `ValidationErrorSet` return 422.
5. Submit `merged` through the standard mutation pipeline with `expected_version = token.head_version`.
6. If the snapshot writer returns `SnapshotWriteError::Conflict(err)`:
   - Generate a fresh `RebasePlan`.
   - Issue a fresh rebase token.
   - Return `(StatusCode::CONFLICT, RebaseResponseBody::Conflict { rebase_token, rebase_plan, current_version: err.current_version })`.
7. On success:
   - Write one audit row. If `token.plan.commutativity == Commutative` → `AuditEvent::MutationRebasedAuto`; else → `AuditEvent::MutationRebasedManual`.
   - Return `(StatusCode::OK, RebaseResponseBody::Ok { mutation_id, new_version })`.

### Tests

- `core/crates/cli/tests/rebase_endpoint.rs`:
  - `rebase_happy_path_returns_200_and_writes_manual_audit`.
  - `rebase_with_commutative_plan_writes_auto_audit`.
  - `rebase_third_actor_race_returns_409_with_fresh_plan`.
  - `rebase_with_expired_token_returns_410_gone`.
  - `rebase_with_consumed_token_returns_410_gone`.
  - `rebase_with_invalid_resolution_returns_422`.
  - `rebase_with_validation_failure_returns_422`.

### Acceptance command

```
cargo test -p trilithon-cli --test rebase_endpoint
```

### Exit conditions

- All seven tests pass.
- The endpoint is mounted under `/api/v1/mutations/rebase` and returns the documented status codes for every input.
- Auto vs manual audit rows are differentiated by `commutativity`.

### Audit kinds emitted

Per §6.6: `mutation.rebased.auto`, `mutation.rebased.manual`, `mutation.rebase.expired`, `mutation.applied` (on the success path, written by the existing apply pipeline), `mutation.conflicted` (on the third-actor race path, written by slice 17.5's helper invoked from this handler).

### Tracing events emitted

Per §12.1: `http.request.received`, `http.request.completed`, `apply.started`, `apply.succeeded`, `apply.failed`. No new events introduced.

### Cross-references

- Phase 17 task: "Implement `POST /api/v1/mutations/rebase`."
- ADR-0012.

---

## Slice 17.7 [standard] — Conflict banner and rebase view (web UI)

### Goal

Ship the conflict banner, the `useRebase` hook, the `RebaseView` route, and the `ThreeWayDiff` presentational component. The user sees a 409 surface that links to `/conflicts/:rebaseToken`, picks per-field resolutions, validates them via a dry-run endpoint, and submits the rebase.

### Entry conditions

- Slice 17.6 shipped.
- The web shell's authenticated layout from Phase 11 is in place.
- The TanStack Query client is already configured.

### Files to create or modify

- `web/src/features/concurrency/types.ts` — TypeScript mirror of `ConflictResponseBody`, `RebasePlan`, `FieldConflict`, `FieldResolution`.
- `web/src/features/concurrency/useRebase.ts` — hook.
- `web/src/features/concurrency/ConflictBanner.tsx` — banner component.
- `web/src/features/concurrency/RebaseView.tsx` — route view.
- `web/src/components/diff/ThreeWayDiff.tsx` — presentational diff.
- `web/src/features/concurrency/ConflictBanner.test.tsx`.
- `web/src/features/concurrency/RebaseView.test.tsx`.
- `web/src/components/diff/ThreeWayDiff.test.tsx`.
- `web/src/features/concurrency/useRebase.test.ts`.
- `web/src/router.tsx` — register `/conflicts/:rebaseToken`.

### Signatures and shapes

```typescript
// web/src/features/concurrency/types.ts

export type ActorRef =
  | { kind: 'user'; id: string; username: string }
  | { kind: 'token'; id: string; name: string }
  | { kind: 'system'; component: string };

export interface FieldConflict {
  readonly path: string;          // JSON Pointer
  readonly base_value: unknown;
  readonly theirs_value: unknown;
  readonly mine_value: unknown;
}

export type FieldResolution =
  | { choice: 'theirs'; path: string }
  | { choice: 'mine'; path: string }
  | { choice: 'custom'; path: string; value: unknown };

export interface RebasePlan {
  readonly conflicting_snapshot_id: string;
  readonly base_version: number;
  readonly head_version: number;
  readonly commutativity:
    | { verdict: 'commutative' }
    | { verdict: 'identical' }
    | { verdict: 'conflicting'; conflicts: readonly FieldConflict[] };
  readonly three_way: {
    readonly base: unknown;
    readonly theirs: unknown;
    readonly mine: unknown;
    readonly conflicts: readonly FieldConflict[];
  };
}

export interface ConflictResponse {
  readonly kind: 'conflict';
  readonly current_version: number;
  readonly attempted_version: number;
  readonly conflicting_snapshot_id: string;
  readonly conflicting_actor: ActorRef;
  readonly rebase_token: string;
  readonly rebase_plan: RebasePlan;
}

export type RebaseResult =
  | { kind: 'ok'; mutation_id: string; new_version: number }
  | { kind: 'conflict'; rebase_token: string; rebase_plan: RebasePlan; current_version: number }
  | { kind: 'gone'; reason: 'rebase-token-expired' | 'rebase-token-consumed' }
  | { kind: 'unprocessable'; errors: readonly { path: string; reason: string }[] };
```

```typescript
// web/src/features/concurrency/useRebase.ts

export type RebaseStatus = 'idle' | 'submitting' | 'succeeded' | 'failed';

export function useRebase(): {
  startRebase: (token: string, resolutions: readonly FieldResolution[]) => Promise<RebaseResult>;
  status: RebaseStatus;
  lastError: string | null;
};
```

```typescript
// web/src/features/concurrency/ConflictBanner.tsx

export function ConflictBanner(props: { conflict: ConflictResponse }): JSX.Element;
```

```typescript
// web/src/features/concurrency/RebaseView.tsx

export function RebaseView(): JSX.Element;
// Reads :rebaseToken from the URL, fetches the plan, renders ThreeWayDiff,
// validates with POST /api/v1/mutations/rebase/dry-run, submits with
// POST /api/v1/mutations/rebase.
```

```typescript
// web/src/components/diff/ThreeWayDiff.tsx

export function ThreeWayDiff(props: {
  base: unknown;
  theirs: unknown;
  mine: unknown;
  conflicts: readonly FieldConflict[];
  onResolve: (resolutions: readonly FieldResolution[]) => void;
}): JSX.Element;
```

### Algorithm

`ConflictBanner` rendering:

1. Build text `Configuration changed since you started. Rebase your changes onto v<conflict.current_version>.`.
2. Render an anchor to `/conflicts/<conflict.rebase_token>`.
3. The literal phrase `rebase your changes onto v<N>` MUST appear verbatim in the rendered DOM.

`RebaseView` flow:

1. Read `rebaseToken` from `useParams()`. If absent, render a "missing token" error.
2. `useQuery` against `GET /api/v1/mutations/rebase/<token>` to fetch the plan body.
3. On success, render `ThreeWayDiff` with `onResolve` updating local component state.
4. The "Submit rebase" button is disabled until `resolutions.length === plan.three_way.conflicts.length` and every conflict has a resolution.
5. The "Validate" button calls `POST /api/v1/mutations/rebase/dry-run` with the current resolutions; render the result inline.
6. On submit, call `useRebase().startRebase(rebaseToken, resolutions)`. Branch on the discriminated `RebaseResult.kind`.

`ThreeWayDiff` rendering:

1. For each conflict, render three columns (`base` / `theirs` / `mine`) with the value JSON-pretty-printed.
2. Render a radio group with three options: `theirs`, `mine`, `custom`.
3. The custom option reveals a Monaco-style JSON editor; on blur, parse and validate, calling `onResolve` with the merged resolutions array.

### Tests

- `web/src/features/concurrency/ConflictBanner.test.tsx`:
  - `renders_link_to_rebase_view`.
  - `contains_literal_phrase_rebase_your_changes_onto_v<N>`.
- `web/src/features/concurrency/RebaseView.test.tsx`:
  - `submit_disabled_until_all_conflicts_resolved`.
  - `submit_button_enables_on_full_resolution_set`.
  - `mixed_resolutions_call_onresolve_with_complete_set`.
- `web/src/components/diff/ThreeWayDiff.test.tsx`:
  - `snapshot_renders_two_conflicts_with_radios`.
  - `custom_value_invokes_onresolve_on_blur`.
- `web/src/features/concurrency/useRebase.test.ts`:
  - `startrebase_returns_ok_on_200`.
  - `startrebase_returns_gone_on_410`.
  - `startrebase_returns_conflict_on_409_with_fresh_plan`.

### Acceptance command

```
cd web && pnpm typecheck && pnpm lint && pnpm test --run
```

### Exit conditions

- All nine Vitest tests pass.
- `pnpm typecheck` succeeds with `noUncheckedIndexedAccess` and `exactOptionalPropertyTypes`.
- The `ConflictBanner`, `RebaseView`, `ThreeWayDiff`, and `useRebase` modules contain no `any`, no non-null assertions, no `@ts-ignore`.

### Audit kinds emitted

None directly from the web tier. The backend writes the rows in slice 17.6.

### Tracing events emitted

None directly from the web tier.

### Cross-references

- Phase 17 tasks: "Implement `useRebase`," "Implement `ConflictBanner`," "Implement `RebaseView`," "Implement `ThreeWayDiff`."
- ADR-0004 (frontend stack).

---

## Slice 17.8 [standard] — End-to-end concurrency scenarios

### Goal

Land the six end-to-end integration tests that exercise the full conflict path: commutative auto-merge, conflicting manual merge, identical-mutation deduplication, expired-token rejection, third-actor race, and conflict-during-rollback.

### Entry conditions

- Slices 17.4, 17.5, 17.6 shipped.
- A test harness with two simulated actors and a clock fake exists from the Phase 16 hardening work.

### Files to create or modify

- `core/crates/adapters/tests/concurrency_commutative.rs`.
- `core/crates/adapters/tests/concurrency_conflicting.rs`.
- `core/crates/adapters/tests/concurrency_identical.rs`.
- `core/crates/adapters/tests/concurrency_expired_token.rs`.
- `core/crates/adapters/tests/concurrency_third_actor_race.rs`.
- `core/crates/adapters/tests/concurrency_rollback_conflict.rs`.

### Signatures and shapes

No new public types. The tests construct `TypedMutation` payloads, drive two actors against a shared `SnapshotStore`, `RebaseTokenStore`, and HTTP server, and assert audit rows and final snapshot versions.

### Algorithm

`concurrency_commutative.rs`:

1. Boot two HTTP clients sharing one daemon.
2. Actor A submits `AddRouteHeader { route_id: "a", header: "X-A": "1" }` with `expected_version = 0`.
3. Actor B submits `UpdateUpstream { route_id: "b", port: 9001 }` with `expected_version = 0`.
4. Run both submissions concurrently.
5. Assert exactly one 200 response on the first; the second returns 409.
6. The second client follows the link, calls `POST /api/v1/mutations/rebase` with all-`mine` resolutions (none, since the plan is commutative) — actually the commutative path auto-merges in the original submission via `apply_resolutions(&[])`.
7. End state: two snapshots beyond the base; `mutation.rebased.auto` audit row count == 1.

`concurrency_conflicting.rs`:

1. Both actors target the same field `/routes/0/upstream/port` with different values.
2. Loser receives 409.
3. Loser POSTs to `/api/v1/mutations/rebase` with `[{ choice: "mine", path: "/routes/0/upstream/port" }]`.
4. End state: one final winning snapshot; `mutation.rebased.manual` audit row count == 1.

`concurrency_identical.rs`:

1. Both actors submit byte-identical `TypedMutation`.
2. The snapshot writer returns `SnapshotInsertOutcome::Idempotent` for the second; HTTP returns 200 with the existing `mutation_id`.
3. End state: one final snapshot; zero `mutation.conflicted` rows; one `mutation.applied` row (the loser's submission deduplicates).

`concurrency_expired_token.rs`:

1. Drive a 409.
2. Advance the test clock past the token TTL.
3. Submit the rebase.
4. Assert 410 with `kind: "rebase-token-expired"` and one `mutation.rebase.expired` audit row.

`concurrency_third_actor_race.rs`:

1. Drive a 409 between actors A and B.
2. Actor C submits a successful mutation against the head.
3. Actor A submits the rebase using the token from step 1.
4. Assert 409 with a fresh `rebase_plan` whose `base_version` matches actor C's new head.

`concurrency_rollback_conflict.rs`:

1. Trigger a rollback on a snapshot at version N.
2. While the rollback's compare-and-swap is in flight, a forward mutation lands at version N+1.
3. The rollback observes `expected_version` mismatch.
4. Assert the same `ConflictError` shape and that the user can complete the flow through `RebaseView`.

### Tests

The six files above. Each contains a single `#[tokio::test]` that drives the scenario end-to-end.

### Acceptance command

```
cargo test -p trilithon-adapters --test concurrency_commutative \
  --test concurrency_conflicting --test concurrency_identical \
  --test concurrency_expired_token --test concurrency_third_actor_race \
  --test concurrency_rollback_conflict
```

### Exit conditions

- All six integration tests pass.
- The audit row counts in each scenario match the assertions.
- No flake on ten consecutive runs.

### Audit kinds emitted

Per §6.6: `mutation.conflicted`, `mutation.rebased.auto`, `mutation.rebased.manual`, `mutation.rebase.expired`, `mutation.applied`, `config.applied`, `config.rolled-back`.

### Tracing events emitted

Per §12.1: `http.request.received`, `http.request.completed`, `apply.started`, `apply.succeeded`, `apply.failed`.

### Cross-references

- Phase 17 task block "Tests."
- Hazard H8.

---

## Phase exit checklist

- [ ] `just check` passes.
- [ ] Every typed-mutation entry carries an `expected_version`; missing entry rejected with audit row `mutation.rejected.missing-expected-version`.
- [ ] Stale-version submissions return `409 Conflict` with the typed body.
- [ ] Commutative conflicts auto-rebase; conflicting conflicts surface a `ThreeWayDiff` and route through `RebaseView`.
- [ ] The conflict path is reachable from the web UI and the gateway placeholder client.
- [ ] All six integration scenarios pass.

## Open questions

None outstanding.
