# Phase 17 — Concurrency-control surface

Source of truth: [`../phases/phased-plan.md#phase-17--concurrency-control-surface`](../phases/phased-plan.md#phase-17--concurrency-control-surface).

## Pre-flight checklist

- [ ] Phase 16 complete (Tier 1 hardened).
- [ ] The snapshot writer's compare-and-swap path exists from Phase 5; the `UNIQUE INDEX snapshots_config_version (caddy_instance_id, config_version)` is in place.
- [ ] The `mutations` table from Phase 4/9 carries an `expected_version INTEGER NOT NULL` column.

## Tasks

### Core types

- [ ] **Define `ConflictError` and `ActorRef`.**
  - Module: `core/crates/core/src/concurrency.rs`.
  - Acceptance: `pub struct ConflictError { current_version: i64, attempted_version: i64, conflicting_snapshot_id: String, conflicting_actor: ActorRef, current_desired_state: DesiredState, attempted_mutation: TypedMutation }`. `pub enum ActorRef { User { id: String, username: String }, Token { id: String, name: String }, System { component: String } }`. Both MUST be `Serialize + Deserialize`.
  - Done when: a unit test serialises and deserialises a representative `ConflictError`.
  - Feature: T2.10 (mitigates H8).
- [ ] **Define `MutationCommutativity`, `RebasePlan`, `ThreeWayDiff`, `FieldConflict`, `FieldResolution`, `FieldPath`.**
  - Module: `core/crates/core/src/concurrency.rs`.
  - Acceptance: As specified in the phased-plan section. `FieldPath` MUST serialise as JSON Pointer.
  - Done when: a unit test exercises every variant and asserts JSON-Pointer round-trip on `FieldPath`.
  - Feature: T2.10.
- [ ] **Define `RebaseError`.**
  - Module: `core/crates/core/src/concurrency.rs`.
  - Acceptance: `pub enum RebaseError { TokenExpired, TokenConsumed, ConflictAppeared { latest_version: i64 }, ResolutionInvalid { path: FieldPath, reason: String }, ValidationFailed(ValidationErrorSet) }`.
  - Done when: a unit test enumerates the variants.
  - Feature: T2.10.

### Snapshot writer compare-and-swap

- [ ] **Wire `ConflictError` into the snapshot writer.**
  - Module: `core/crates/adapters/src/snapshot_store.rs`.
  - Acceptance: `pub fn insert_if_absent(...) -> Result<SnapshotInsertOutcome, SnapshotWriteError>` MUST detect a stale `expected_version` inside the `BEGIN IMMEDIATE` transaction and return `SnapshotWriteError::Conflict(ConflictError)`. The error MUST carry the current `DesiredState` and the attempted mutation.
  - Done when: an integration test simulates two writers and asserts the `Conflict` variant on the loser.
  - Feature: T2.10 (mitigates H8).

### Rebase planner (pure)

- [ ] **Implement `plan_rebase`.**
  - Module: `core/crates/core/src/concurrency/rebase.rs`.
  - Acceptance: `pub fn plan_rebase(base: &DesiredState, theirs: &DesiredState, mine: &TypedMutation) -> RebasePlan`. Pure, no I/O. Classification MUST inspect each typed-mutation variant's effective field set.
  - Done when: unit tests cover commutative, conflicting, and identical-mutation cases; a `proptest` harness generates random pairs and asserts that classification's commutative verdict implies field-set disjointness.
  - Feature: T2.10.
- [ ] **Implement `apply_resolutions`.**
  - Module: `core/crates/core/src/concurrency/rebase.rs`.
  - Acceptance: `pub fn apply_resolutions(plan: &RebasePlan, resolutions: &[FieldResolution]) -> Result<TypedMutation, RebaseError>`. The function MUST require resolutions for every `FieldConflict` in the plan; missing resolutions MUST yield `RebaseError::ResolutionInvalid`.
  - Done when: unit tests cover full-merge, partial-merge, custom-value, and missing-resolution cases.
  - Feature: T2.10.

### Storage

- [ ] **Define the in-memory `RebaseToken` record.**
  - Module: `core/crates/core/src/concurrency.rs`.
  - Acceptance: `pub struct RebaseToken { pub id: ulid::Ulid, pub conflicting_snapshot_id: SnapshotId, pub base_version: u64, pub head_version: u64, pub actor: ActorId, pub plan: RebasePlan, pub created_at: UnixSeconds, pub expires_at: UnixSeconds }`. The record is `Clone + Debug`. It is **not** `Serialize`/`Deserialize` for storage purposes; the on-the-wire form going to the HTTP client is the existing `RebasePlan` body, not this struct.
  - Done when: a unit test instantiates and clones the record.
  - Feature: T2.10.
- [ ] **Implement `RebaseTokenStore` as an in-memory `DashMap`.**
  - Module: `core/crates/adapters/src/rebase_token_store.rs`.
  - Acceptance: The store wraps `DashMap<RebaseTokenId, RebaseToken>`. Public surface: `pub fn issue(&self, plan: RebasePlan, actor: ActorId, ttl: Duration) -> RebaseTokenId`, `pub fn consume(&self, id: RebaseTokenId) -> Result<RebaseToken, RebaseTokenError>` (returns `TokenExpired`, `TokenNotFound`, or the consumed token; consumption removes the entry atomically). Each public method MUST sweep expired tokens out of the map before serving its own request; this is the only garbage-collection mechanism. **Tokens MUST NOT be persisted to SQLite or any other on-disk store.** A daemon restart invalidates every outstanding token by construction.
  - Done when: integration tests cover issue, consume, double-consume rejection, sweep-on-call expiry, and daemon-restart invalidation (a fresh store has no carry-over tokens).
  - Feature: T2.10.
- [ ] **Add the `rebase_token_ttl_minutes` configuration knob with bounds-checking validator.**
  - Module: `core/crates/core/src/config.rs`.
  - Acceptance: `[concurrency] rebase_token_ttl_minutes: u32` with default **30**. Bounds: minimum 5, maximum 1440. The configuration validator MUST reject values outside `[5, 1440]` with a typed `ConfigError::OutOfRange { field: "concurrency.rebase_token_ttl_minutes", value, min: 5, max: 1440 }` before the daemon accepts the configuration; the daemon refuses to start with the existing configuration-error exit code.
  - Done when: unit tests cover the default, an in-bounds value, the lower-bound boundary (5), the upper-bound boundary (1440), and out-of-bounds values on both sides; an integration test boots the daemon with `rebase_token_ttl_minutes = 4` and asserts the typed error and refusal-to-start.
  - Feature: T2.10.

### HTTP API

- [ ] **Surface the existing `expected_version` mismatch as a UI-visible conflict resolution flow.**
  - Module: `core/crates/cli/src/http/mutations/mod.rs`.
  - Acceptance: The `expected_version` envelope is already enforced from Phase 4 onward (Phase 4 defines it on every `Mutation` variant; Phase 9 enforces 400 on missing field and 409 on stale version). Phase 17 does NOT retrofit the field — Phase 17 surfaces the existing 409 as a user-visible conflict resolution flow with a `RebasePlan` payload. This task verifies that the envelope is present on every mutation endpoint introduced or extended in Phase 17 and that the 409 response carries the new `RebasePlan` body shape.
  - Done when: a parametrised integration test asserts every mutation endpoint returns 400 on missing `expected_version` (with audit kind `mutation.rejected.missing-expected-version`) and 409 with a `RebasePlan` body on stale version.
  - Feature: T2.10.
- [ ] **Return typed `409 Conflict` with `RebasePlan`.**
  - Module: `core/crates/cli/src/http/mutations/conflict.rs`.
  - Acceptance: On `SnapshotWriteError::Conflict`, the handler MUST issue a `RebasePlan`, persist a rebase-token row, and return `409 Conflict` with body `{ kind: "conflict", current_version, attempted_version, conflicting_snapshot_id, conflicting_actor, rebase_token, rebase_plan }`.
  - Done when: an integration test simulating two actors asserts the body shape.
  - Feature: T2.10.
- [ ] **Implement `POST /api/v1/mutations/rebase`.**
  - Module: `core/crates/cli/src/http/mutations/rebase.rs`.
  - Acceptance: Body `{ rebase_token: String, resolutions: Vec<FieldResolution> }`. Response: `200 OK` with `{ mutation_id, new_version }`, `409 Conflict` (third-actor race), `410 Gone` (expired or consumed token), `422 Unprocessable Entity` (validation failed). The handler MUST run the merged result through the standard validation pipeline before submission.
  - Done when: integration tests cover all four response cases.
  - Feature: T2.10.

### Audit

- [ ] **Add audit kinds.**
  - Module: `core/crates/core/src/audit.rs`.
  - Acceptance: Add `MutationConflicted`, `MutationRebasedAuto`, `MutationRebasedManual`, `MutationRebaseExpired`, `MutationRejectedMissingExpectedVersion`. Each MUST carry a stable kebab-case `kind`.
  - Done when: a unit test asserts every variant's `kind` is unique.
  - Feature: T2.10 / T1.7.
- [ ] **Audit row authoring on each transition.**
  - Acceptance: On each transition (conflict detected, auto-rebase applied, manual rebase applied, token expired) the audit log MUST contain exactly one corresponding row with the `notes` shape specified in the phased plan.
  - Done when: integration tests assert the audit rows on every transition.
  - Feature: T2.10 / T1.7.

### Tool gateway placeholder

- [ ] **Reuse `ConflictError` as the gateway's conflict shape.**
  - Module: `core/crates/core/src/gateway/types.rs`.
  - Acceptance: Phase 19's gateway MUST surface the same `ConflictError` body verbatim. A contract test against a placeholder gateway client MUST assert the JSON shape matches.
  - Done when: the contract test passes.
  - Feature: T2.10 / T2.3 / T2.4.

### Web UI

- [ ] **Implement `useRebase` query/mutation hook.**
  - Path: `web/src/features/concurrency/useRebase.ts`.
  - Acceptance: `export function useRebase(): { startRebase: (token: string, resolutions: FieldResolution[]) => Promise<RebaseResult>; status: ... }`. Strongly typed; no `any`.
  - Done when: a Vitest test exercises a stubbed call and asserts type narrowing.
  - Feature: T2.10.
- [ ] **Implement `ConflictBanner`.**
  - Path: `web/src/features/concurrency/ConflictBanner.tsx`.
  - Acceptance: `export function ConflictBanner(props: { conflict: ConflictResponse }): JSX.Element`. MUST render a banner above any mutation form when the most recent submission returned a 409, with a link to the rebase view. The copy MUST contain the literal phrase "rebase your changes onto v<N>".
  - Done when: a Vitest test asserts the rendered link target and the literal phrase.
  - Feature: T2.10.
- [ ] **Implement `RebaseView`.**
  - Path: `web/src/features/concurrency/RebaseView.tsx`.
  - Acceptance: Route `/conflicts/:rebaseToken`. Renders the three-way diff, per-field radio (`theirs` / `mine` / `custom`), a JSON editor for the custom case, a "Validate" button (calls a dry-run validation endpoint), and a "Submit rebase" button. Submission disabled until all conflicts have a resolution.
  - Done when: a Vitest test exercises a multi-conflict diff with mixed resolutions.
  - Feature: T2.10.
- [ ] **Implement `ThreeWayDiff` presentational component.**
  - Path: `web/src/components/diff/ThreeWayDiff.tsx`.
  - Acceptance: Props `{ base: unknown; theirs: unknown; mine: unknown; conflicts: FieldConflict[]; onResolve: (resolutions: FieldResolution[]) => void }`. Pure presentational; no fetching.
  - Done when: a Vitest snapshot test asserts a representative render.
  - Feature: T2.10.

### Tests

- [ ] **Two-actor commutative scenario.**
  - Module: `core/crates/adapters/tests/concurrency_commutative.rs`.
  - Acceptance: Two simulated actors mutate disjoint routes concurrently. Exactly one conflict MUST be detected; the auto-rebase path MUST produce one final winning snapshot per actor (two total) without manual intervention.
  - Done when: the integration test passes.
  - Feature: T2.10.
- [ ] **Two-actor conflicting scenario.**
  - Module: `core/crates/adapters/tests/concurrency_conflicting.rs`.
  - Acceptance: Two actors mutate the same field on the same route. Exactly one conflict and one rebase-token issued; manual resolution submission produces exactly one winning snapshot.
  - Done when: the integration test passes.
  - Feature: T2.10.
- [ ] **Identical-mutation deduplication.**
  - Acceptance: Two actors submit identical mutations concurrently; the conflict resolver MUST detect equality and produce one winning snapshot, skipping rebase.
  - Done when: the integration test passes.
  - Feature: T2.10.
- [ ] **Expired rebase token rejected.**
  - Acceptance: A submission with an expired token MUST receive `410 Gone` with `kind = "rebase-token-expired"`.
  - Done when: the integration test passes.
  - Feature: T2.10.
- [ ] **Third-actor mid-rebase race.**
  - Acceptance: A third actor mutates after the rebase plan was generated but before submission; the rebase submission MUST receive `409 Conflict` again with a fresh plan.
  - Done when: the integration test passes.
  - Feature: T2.10.
- [ ] **Conflict during rollback.**
  - Acceptance: A rollback that becomes a stale-version mutation due to a concurrent forward-mutation MUST surface the same `ConflictError` and reach the `RebaseView` flow.
  - Done when: the integration test passes.
  - Feature: T2.10 / T1.3.

## Cross-references

- ADR-0008 (bounded typed tool gateway for language models).
- ADR-0009 (immutable content-addressed snapshots and audit log).
- ADR-0012 (optimistic concurrency on monotonic `config_version`).
- PRD T2.10 (concurrency control).
- Architecture: "Concurrency model," "Mutation lifecycle," section 9.
- Hazards: H8 (Concurrent modification).

## Sign-off checklist

- [ ] `just check` passes.
- [ ] Every typed-mutation entry carries an `expected_version`; missing entry rejected with audit row.
- [ ] Stale-version submissions return `409 Conflict` with the typed body.
- [ ] Commutative conflicts auto-rebase; conflicting conflicts surface a `ThreeWayDiff` and route through `RebaseView`.
- [ ] The conflict path is reachable from the web UI and the gateway placeholder client.
- [ ] All six integration scenarios pass.
