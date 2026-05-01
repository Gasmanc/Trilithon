# Phase 4 — Typed desired-state model and mutation API (in-memory)

Source of truth: [`../phases/phased-plan.md#phase-4--typed-desired-state-model-and-mutation-api-in-memory`](../phases/phased-plan.md#phase-4--typed-desired-state-model-and-mutation-api-in-memory).

> **Path-form note.** All `crates/<name>/...` paths in this file are workspace-relative; rooted at `core/` on disk. So `crates/core/src/foo.rs` resolves to `core/crates/core/src/foo.rs`. See [`README.md`](README.md) "Path conventions".

> **Authoritative cross-references.** Trait surfaces consumed or introduced here are documented in [`../architecture/trait-signatures.md`](../architecture/trait-signatures.md). Audit `kind` values are bound by architecture §6.6. Tracing event names are bound by architecture §12.1.

## Pre-flight checklist

- [ ] Phase 3 complete; capability probe results are queryable.

## Tasks

### Backend / core crate — file layout

The Phase 4 work expands `crates/core/` into the following files. Each task below names the exact file(s) it touches.

- `crates/core/src/model/mod.rs` — re-exports.
- `crates/core/src/model/route.rs` — `Route`, `RouteId`, `HostPattern`, `RoutePolicyAttachment`.
- `crates/core/src/model/upstream.rs` — `Upstream`, `UpstreamId`, `UpstreamDestination`, `UpstreamProbe`.
- `crates/core/src/model/policy.rs` — `PolicyId`, `PolicyAttachment`, `PresetId`, `PresetVersion`.
- `crates/core/src/model/matcher.rs` — `MatcherSet`, `PathMatcher`, `QueryMatcher`, `HeaderMatcher`, `CidrMatcher`, `HttpMethod`.
- `crates/core/src/model/header.rs` — `HeaderRules`, `HeaderOp`.
- `crates/core/src/model/redirect.rs` — `RedirectRule`.
- `crates/core/src/model/tls.rs` — `TlsConfig`, `TlsConfigPatch`.
- `crates/core/src/model/global.rs` — `GlobalConfig`, `GlobalConfigPatch`.
- `crates/core/src/mutation/mod.rs` — re-exports.
- `crates/core/src/mutation/types.rs` — the `Mutation` enum and patch types.
- `crates/core/src/mutation/apply.rs` — `apply_mutation` entry point.
- `crates/core/src/mutation/error.rs` — `MutationError` enum.
- `crates/core/src/mutation/validate.rs` — schema and pre-condition validators.
- `crates/core/src/mutation/capability.rs` — capability-gating algorithm.

### Define typed records for the desired-state model

- [ ] **Define `DesiredState` and the route/upstream/policy aggregate types.**
  - Path: `crates/core/src/model/*.rs`.
  - Acceptance: The following Rust definitions MUST appear verbatim in the named files (every field type pinned):

    ```rust
    pub struct DesiredState {
        pub version:       i64,                    // monotonic, the optimistic-concurrency anchor
        pub routes:        BTreeMap<RouteId, Route>,
        pub upstreams:     BTreeMap<UpstreamId, Upstream>,
        pub policies:      BTreeMap<PolicyId, PolicyAttachment>,
        pub presets:       BTreeMap<PresetId, PresetVersion>,
        pub tls:           TlsConfig,
        pub global:        GlobalConfig,
    }
    pub struct Route {
        pub id:                RouteId,
        pub hostnames:         Vec<HostPattern>,   // RFC 952 + RFC 1123 validated
        pub upstreams:         Vec<UpstreamId>,    // non-empty for reverse_proxy
        pub matchers:          MatcherSet,
        pub headers:           HeaderRules,
        pub redirects:         Option<RedirectRule>,
        pub policy_attachment: Option<RoutePolicyAttachment>,
        pub enabled:           bool,
        pub created_at:        UnixSeconds,
        pub updated_at:        UnixSeconds,
    }
    pub struct Upstream {
        pub id:                  UpstreamId,
        pub destination:         UpstreamDestination, // TcpAddr | UnixSocket | DockerContainer
        pub probe:               UpstreamProbe,       // Tcp | Http { path, expected_status } | Disabled
        pub weight:              u16,                 // load balancing
        pub max_request_bytes:   Option<u64>,
    }
    pub enum HostPattern { Exact(String), Wildcard(String) }
    pub struct MatcherSet {
        pub paths:    Vec<PathMatcher>,
        pub methods:  Vec<HttpMethod>,
        pub query:    Vec<QueryMatcher>,
        pub headers:  Vec<HeaderMatcher>,
        pub remote:   Vec<CidrMatcher>,
    }
    pub struct HeaderRules {
        pub request:  Vec<HeaderOp>,    // Set | Add | Delete
        pub response: Vec<HeaderOp>,
    }
    pub struct RedirectRule { pub to: String, pub status: u16 }
    pub struct RoutePolicyAttachment { pub preset_id: PresetId, pub preset_version: u32 }
    pub struct PolicyAttachment { /* see Phase 18 for the resolved-attachment shape */ }
    ```

  - Done when: every type derives `serde::{Serialize, Deserialize}` and `Debug` and `cargo test -p trilithon-core model::tests::serde_round_trip` passes.
  - Feature: T1.6.

### Define the `Mutation` enum for Tier 1

- [ ] **Closed `Mutation` enum, every variant carrying `expected_version: i64`.**
  - Path: `crates/core/src/mutation/types.rs`.
  - Acceptance: The Rust enum MUST be defined verbatim:

    ```rust
    pub enum Mutation {
        CreateRoute       { expected_version: i64, route:    Route },
        UpdateRoute       { expected_version: i64, id:       RouteId, patch: RoutePatch },
        DeleteRoute       { expected_version: i64, id:       RouteId },
        CreateUpstream    { expected_version: i64, upstream: Upstream },
        UpdateUpstream    { expected_version: i64, id:       UpstreamId, patch: UpstreamPatch },
        DeleteUpstream    { expected_version: i64, id:       UpstreamId },
        AttachPolicy      { expected_version: i64, route_id: RouteId, preset_id: PresetId, preset_version: u32 },
        DetachPolicy      { expected_version: i64, route_id: RouteId },
        UpgradePolicy     { expected_version: i64, route_id: RouteId, to_version: u32 },
        SetGlobalConfig   { expected_version: i64, patch: GlobalConfigPatch },
        SetTlsConfig      { expected_version: i64, patch: TlsConfigPatch },
        ImportFromCaddyfile { expected_version: i64, parsed: ParsedCaddyfile },
        Rollback          { expected_version: i64, target: SnapshotId },
    }
    ```

    Every mutation request envelope MUST include `expected_version`; the snapshot writer compares against the current `config_version` and returns `MutationError::Conflict` on mismatch. The conflict UI does not arrive until Phase 17, but the field MUST be threaded from Phase 4 onward.
  - Done when: the enum compiles, every variant carries `expected_version: i64`, and a unit test exercises the missing-`expected_version` rejection path against an envelope deserialiser (records `mutation.rejected.missing-expected-version`).
  - Feature: T1.6.

### Define `MutationOutcome` and `MutationError`

- [ ] **`MutationOutcome` and `MutationError` Rust definitions.**
  - Path: `crates/core/src/mutation/{apply.rs,error.rs}`.
  - Acceptance: The following Rust definitions MUST appear verbatim:

    ```rust
    pub struct MutationOutcome {
        pub new_state: DesiredState,
        pub diff:      Diff,                       // structural diff vs `state`
        pub kind:      AuditEvent,                 // dotted kind via Display
    }

    pub enum MutationError {
        Validation         { rule: ValidationRule, path: JsonPointer, hint: String },
        CapabilityMissing  { module: CaddyModule, required_by: MutationKind },
        Conflict           { observed_version: i64, expected_version: i64 },
        Schema             { field: JsonPointer, kind: SchemaErrorKind },
        Forbidden          { reason: ForbiddenReason },
    }
    ```
  - Done when: a unit test exercises one successful and one of each failure variant and asserts the typed outcome.
  - Feature: T1.6.

### Implement `apply_mutation`

- [ ] **`DesiredState::apply_mutation` — pure entry point.**
  - Path: `crates/core/src/mutation/apply.rs`.
  - Acceptance: The function signature MUST be exactly:

    ```rust
    /// Apply a mutation against an immutable desired state. Pure: no I/O,
    /// no clock reads except via the caller-supplied `UnixSeconds` timestamps
    /// already present on the mutation payload.
    ///
    /// # Errors
    ///
    /// Returns `MutationError::Conflict` if `mutation.expected_version` does
    /// not match `state.version`. Returns `MutationError::CapabilityMissing`
    /// when the mutation references a Caddy module absent from `capabilities`.
    pub fn apply_mutation(
        state: &DesiredState,
        mutation: &Mutation,
        capabilities: &CapabilitySet,
    ) -> Result<MutationOutcome, MutationError>;
    ```
  - Done when: a unit test asserts equal outputs for equal inputs and a clippy lint forbids `#[allow(dead_code)]` shortcuts.
  - Feature: T1.6.

### Carry a client-supplied `MutationId` ULID with every mutation

- [ ] **`MutationId` ULID idempotency.**
  - Acceptance: Every mutation MUST carry a `MutationId` ULID alongside `expected_version`; repeated application with the same identifier MUST produce the same outcome.
  - Done when: a property test asserts idempotency on identifier.
  - Feature: T1.6.

### Capability gating at validation time

- [ ] **Capability-gating algorithm.**
  - Path: `crates/core/src/mutation/capability.rs`.
  - Acceptance: The capability-gating algorithm MUST be implemented as numbered pseudocode:

    ```
    1. Let `referenced_modules = mutation.referenced_caddy_modules()`.
    2. Let `loaded_modules = capabilities.loaded_modules`.
    3. Let `missing = referenced_modules - loaded_modules`.
    4. If `missing` is empty, proceed to schema validation.
    5. Otherwise return `Err(MutationError::CapabilityMissing { module: missing.first(), required_by: mutation.kind() })`.
    ```

    The function `mutation.referenced_caddy_modules() -> BTreeSet<CaddyModule>` is implemented per variant in the same file and is unit-tested per variant.
  - Done when: a unit test with a stripped capability set rejects the relevant mutation. Satisfies H5.
  - Feature: T1.6 (mitigates H5).

### Schema generation

- [ ] **Generate one JSON schema per mutation variant.**
  - Acceptance: A build-time generator MUST produce one JSON schema per mutation variant under `docs/schemas/mutations/`.
  - Done when: `just check` regenerates the schemas and asserts no drift.
  - Feature: T1.6.
- [ ] **Document pre/post conditions in prose for every mutation.**
  - Acceptance: Every mutation variant MUST have a Rust doc comment stating its pre-condition, post-condition, and idempotency contract.
  - Done when: a `cargo doc --no-deps` build passes with the documented items and a lint requires doc comments on every public item.
  - Feature: T1.6.

### Tests

- [ ] **Property test: idempotency on `MutationId`.**
  - Acceptance: A `proptest` MUST assert that applying the same mutation twice produces equal desired states.
  - Done when: `cargo test -p trilithon-core model::props::idempotency` passes.
  - Feature: T1.6.
- [ ] **Property test: ordering of independent mutations is irrelevant.**
  - Acceptance: A `proptest` MUST assert that the order of independent mutations does not affect the final desired state.
  - Done when: `cargo test -p trilithon-core model::props::ordering` passes.
  - Feature: T1.6.
- [ ] **Property test: post-conditions hold on every successful application.**
  - Acceptance: A `proptest` MUST assert that no mutation produces a desired state that fails its post-condition.
  - Done when: `cargo test -p trilithon-core model::props::postconditions` passes.
  - Feature: T1.6.
- [ ] **Capability-gating unit tests.**
  - Acceptance: Unit tests MUST cover every mutation that references a capability-gated module and assert rejection when the module is absent.
  - Done when: the test asserts `MutationError::CapabilityMissing` for every gated variant.
  - Feature: T1.6 (mitigates H5).

### Documentation

- [ ] **Index the mutation schemas in `docs/schemas/mutations/README.md`.**
  - Acceptance: A README MUST list every Tier 1 mutation, its schema file, and a one-line description.
  - Done when: the index is present and links to every generated file.
  - Feature: T1.6.

## Cross-references

- ADR-0009 (immutable content-addressed snapshots and audit log — substrate consumes mutations).
- ADR-0012 (optimistic concurrency on monotonic `config_version`).
- ADR-0013 (capability probe gates optional Caddy features).
- PRD T1.6 (typed mutation API).
- Architecture: "Mutation algebra," "Validation pipeline," "Capability gating," §6.6 audit `kind` vocabulary.
- [`../architecture/trait-signatures.md`](../architecture/trait-signatures.md) — `core::diff::DiffEngine` (consumed by `MutationOutcome`), `core::policy::PresetRegistry` (consumed by `AttachPolicy` / `UpgradePolicy`).

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] The mutation set is closed under composition: any sequence of valid mutations produces either a valid desired state or a single, identifiable mutation failure.
- [ ] A mutation referencing an absent Caddy module is rejected with a typed error before any apply attempt, satisfying H5.
- [ ] Every mutation variant carries `expected_version: i64`; missing-envelope rejection emits `mutation.rejected.missing-expected-version`.
- [ ] Every mutation has a documented Rust type, a JSON schema, and prose pre/post-condition documentation.
- [ ] Property tests cover idempotency, ordering, and capability gating.
