# Phase 04 — Typed desired-state model and mutation API — Implementation Slices

> Phase reference: [../phases/phase-04-mutation-algebra.md](../phases/phase-04-mutation-algebra.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md) §phase-4--typed-desired-state-model-and-mutation-api-in-memory
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference: [`../phases/phase-04-mutation-algebra.md`](../phases/phase-04-mutation-algebra.md).
- Architecture §6.6 (audit `kind` vocabulary including `mutation.rejected.missing-expected-version`), §6.7 (mutations table), §7.1 (mutation lifecycle), §12.1 (`mutation.kind` span field).
- Trait signatures: `core::diff::DiffEngine` (§5), `core::policy::PresetRegistry` (§4) — both consumed but not implemented in this phase.
- ADR-0009 (immutable snapshots), ADR-0012 (optimistic concurrency), ADR-0013 (capability gating), ADR-0016 (route-policy attachments record preset version).

## Slice plan summary

| Slice | Title | Primary files | Effort (h) | Depends on |
|-------|-------|---------------|------------|------------|
| 4.1 | Aggregate identifier types and primitive value types | `crates/core/src/model/{identifiers,primitive}.rs` | 4 | Phase 3 |
| 4.2 | Route, Upstream, MatcherSet, HeaderRules, RedirectRule | `crates/core/src/model/{route,upstream,matcher,header,redirect}.rs` | 6 | 4.1 |
| 4.3 | TLS, GlobalConfig, policy attachment value types | `crates/core/src/model/{tls,global,policy}.rs` | 4 | 4.1 |
| 4.4 | `DesiredState` aggregate, serde round-trip, `BTreeMap` invariants | `crates/core/src/model/desired_state.rs` | 3 | 4.2, 4.3 |
| 4.5 | Patch types (`RoutePatch`, `UpstreamPatch`, `GlobalConfigPatch`, `TlsConfigPatch`, `ParsedCaddyfile`) | `crates/core/src/mutation/patches.rs` | 4 | 4.4 |
| 4.6 | `Mutation` enum, `MutationId`, `expected_version` envelope | `crates/core/src/mutation/types.rs`, `crates/core/src/mutation/envelope.rs` | 5 | 4.5 |
| 4.7 | `MutationOutcome`, `MutationError`, `Diff`, `AuditEvent` integration | `crates/core/src/mutation/{apply,error,outcome}.rs` | 4 | 4.6 |
| 4.8 | Capability-gating algorithm | `crates/core/src/mutation/capability.rs` | 4 | 4.6 |
| 4.9 | `apply_mutation` per-variant pure implementation | `crates/core/src/mutation/apply.rs` | 8 | 4.7, 4.8 |
| 4.10 | Property tests, schema generation, mutation README | `crates/core/tests/mutation_props.rs`, `core/build.rs`, `docs/schemas/mutations/` | 6 | 4.9 |

Total: 10 slices.

---

## Slice 4.1 [trivial] — Aggregate identifier types and primitive value types

### Goal

Define the identifier newtypes (`RouteId`, `UpstreamId`, `PolicyId`, `PresetId`, `MutationId`) and the primitive value types (`UnixSeconds`, `JsonPointer`, `CaddyModule`). Every identifier is a ULID-bearing newtype with `serde` and `Hash` derives.

### Entry conditions

- Phase 3 complete; `trilithon-core` builds with `serde`, `ulid`.

### Files to create or modify

- `core/crates/core/src/model/mod.rs` — re-exports (new).
- `core/crates/core/src/model/identifiers.rs` — id newtypes (new).
- `core/crates/core/src/model/primitive.rs` — `UnixSeconds`, `JsonPointer`, `CaddyModule` (new).
- `core/crates/core/src/lib.rs` — `pub mod model;` (modify).

### Signatures and shapes

```rust
// core/crates/core/src/model/identifiers.rs
use serde::{Deserialize, Serialize};

macro_rules! id_newtype {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct $name(pub String);

        impl $name {
            pub fn new() -> Self { Self(ulid::Ulid::new().to_string()) }
            pub fn as_str(&self) -> &str { &self.0 }
        }
    };
}

id_newtype!(RouteId);
id_newtype!(UpstreamId);
id_newtype!(PolicyId);
id_newtype!(PresetId);
id_newtype!(MutationId);
```

```rust
// core/crates/core/src/model/primitive.rs
use serde::{Deserialize, Serialize};

pub type UnixSeconds = i64;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct JsonPointer(pub String);     // RFC 6901

impl JsonPointer {
    pub fn root() -> Self { Self("".into()) }
    pub fn push(&self, segment: &str) -> Self { /* RFC 6901 escape ~ and / */ }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CaddyModule(pub String);
```

### Tests

- `core/crates/core/src/model/identifiers.rs::tests::ulid_format` — asserts `RouteId::new().0` is 26 ASCII chars matching `[0-9A-Z]{26}`.
- `core/crates/core/src/model/primitive.rs::tests::json_pointer_escapes_slash` — `root().push("foo/bar")` returns `"/foo~1bar"`.
- `core/crates/core/src/model/primitive.rs::tests::json_pointer_escapes_tilde` — `root().push("a~b")` returns `"/a~0b"`.

### Acceptance command

```
cargo test -p trilithon-core model::identifiers && \
cargo test -p trilithon-core model::primitive
```

### Exit conditions

- Five id newtypes plus three primitives compile.
- Three named tests pass.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- Architecture §6 (id columns), §6.6.
- ADR-0012.

---

## Slice 4.2 [standard] — Route, Upstream, MatcherSet, HeaderRules, RedirectRule

### Goal

Define the structural records for a route's data, upstreams, the matcher set, header rules, and redirects. Hostname patterns are validated against RFC 952 + RFC 1123 rules. The validator is unit-tested per case.

### Entry conditions

- Slice 4.1 complete.

### Files to create or modify

- `core/crates/core/src/model/route.rs` — `Route`, `HostPattern` + validator (new).
- `core/crates/core/src/model/upstream.rs` — `Upstream`, `UpstreamDestination`, `UpstreamProbe` (new).
- `core/crates/core/src/model/matcher.rs` — `MatcherSet`, `PathMatcher`, `QueryMatcher`, `HeaderMatcher`, `CidrMatcher`, `HttpMethod` (new).
- `core/crates/core/src/model/header.rs` — `HeaderRules`, `HeaderOp` (new).
- `core/crates/core/src/model/redirect.rs` — `RedirectRule` (new).
- `core/crates/core/src/model/mod.rs` — re-exports (modify).

### Signatures and shapes

```rust
// core/crates/core/src/model/route.rs
use serde::{Deserialize, Serialize};
use crate::model::{identifiers::*, primitive::UnixSeconds, matcher::*, header::HeaderRules, redirect::RedirectRule};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Route {
    pub id:                RouteId,
    pub hostnames:         Vec<HostPattern>,
    pub upstreams:         Vec<UpstreamId>,
    pub matchers:          MatcherSet,
    pub headers:           HeaderRules,
    pub redirects:         Option<RedirectRule>,
    pub policy_attachment: Option<RoutePolicyAttachment>,
    pub enabled:           bool,
    pub created_at:        UnixSeconds,
    pub updated_at:        UnixSeconds,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HostPattern { Exact(String), Wildcard(String) }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RoutePolicyAttachment {
    pub preset_id:      PresetId,
    pub preset_version: u32,
}

pub fn validate_hostname(s: &str) -> Result<HostPattern, HostnameError>;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HostnameError {
    #[error("hostname is empty")]
    Empty,
    #[error("hostname label {label} starts or ends with hyphen")]
    HyphenBoundary { label: String },
    #[error("hostname label {label} exceeds 63 characters")]
    LabelTooLong { label: String },
    #[error("hostname total length exceeds 253 characters")]
    TotalTooLong,
    #[error("hostname contains invalid character {found}")]
    InvalidCharacter { found: char },
    #[error("wildcard {pattern} must be of the form '*.example.com'")]
    InvalidWildcard { pattern: String },
}
```

```rust
// core/crates/core/src/model/upstream.rs
use serde::{Deserialize, Serialize};
use crate::model::identifiers::UpstreamId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Upstream {
    pub id:                UpstreamId,
    pub destination:       UpstreamDestination,
    pub probe:             UpstreamProbe,
    pub weight:            u16,
    pub max_request_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpstreamDestination {
    TcpAddr        { host: String, port: u16 },
    UnixSocket     { path: String },
    DockerContainer{ container_id: String, port: u16 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpstreamProbe {
    Tcp,
    Http { path: String, expected_status: u16 },
    Disabled,
}
```

```rust
// core/crates/core/src/model/matcher.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MatcherSet {
    pub paths:   Vec<PathMatcher>,
    pub methods: Vec<HttpMethod>,
    pub query:   Vec<QueryMatcher>,
    pub headers: Vec<HeaderMatcher>,
    pub remote:  Vec<CidrMatcher>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PathMatcher(pub String);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod { Get, Post, Put, Patch, Delete, Head, Options }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct QueryMatcher  { pub key: String, pub value: Option<String> }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct HeaderMatcher { pub name: String, pub value: Option<String> }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CidrMatcher(pub String);          // validated as IPv4 or IPv6 CIDR
```

```rust
// core/crates/core/src/model/header.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeaderRules {
    pub request:  Vec<HeaderOp>,
    pub response: Vec<HeaderOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum HeaderOp {
    Set    { name: String, value: String },
    Add    { name: String, value: String },
    Delete { name: String },
}
```

```rust
// core/crates/core/src/model/redirect.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RedirectRule {
    pub to:     String,
    pub status: u16,
}
```

### Algorithm

`validate_hostname(s)`:

1. If `s.is_empty()`, return `Empty`.
2. If `s.starts_with("*.")`, set `wildcard = true`; recurse on `s[2..]`. The remainder MUST contain at least one `.`.
3. If `s.len() > 253`, return `TotalTooLong`.
4. For each `label` between dots:
   1. If `label.is_empty() || label.starts_with('-') || label.ends_with('-')`, return `HyphenBoundary`.
   2. If `label.len() > 63`, return `LabelTooLong`.
   3. For each char: must be ASCII alphanumeric or `-`; otherwise return `InvalidCharacter`.
5. Return `Ok(HostPattern::Wildcard(s.into()))` or `Ok(HostPattern::Exact(s.into()))`.

### Tests

- `core/crates/core/src/model/route.rs::tests::valid_exact_host`.
- `tests::valid_wildcard_host`.
- `tests::reject_double_wildcard` — `*.*.example.com` → `InvalidWildcard`.
- `tests::reject_label_64_chars`.
- `tests::reject_total_254_chars`.
- `tests::reject_label_starting_hyphen` — `-foo.example.com`.
- `core/crates/core/src/model/matcher.rs::tests::matcher_set_serde_round_trip`.
- `core/crates/core/src/model/upstream.rs::tests::destination_tagged_serde` — round-trip `TcpAddr { host: "127.0.0.1", port: 8080 }`.

### Acceptance command

```
cargo test -p trilithon-core model::route && \
cargo test -p trilithon-core model::matcher && \
cargo test -p trilithon-core model::upstream
```

### Exit conditions

- Hostname validator covers RFC 952 + RFC 1123 cases as listed.
- All eight named tests pass.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.6.
- Architecture §7.1 (mutation lifecycle takes Route as payload).

---

## Slice 4.3 [standard] — TLS, GlobalConfig, policy attachment value types

### Goal

Define the global and TLS configuration value types plus the policy `PolicyAttachment` placeholder. Phase 18 supplies the resolved-attachment shape; this slice carries an enum sufficient for Tier 1.

### Entry conditions

- Slice 4.1 complete.

### Files to create or modify

- `core/crates/core/src/model/tls.rs` — `TlsConfig`, `TlsConfigPatch` (new).
- `core/crates/core/src/model/global.rs` — `GlobalConfig`, `GlobalConfigPatch` (new).
- `core/crates/core/src/model/policy.rs` — `PolicyAttachment`, `PresetVersion` (new).
- `core/crates/core/src/model/mod.rs` — re-exports (modify).

### Signatures and shapes

```rust
// core/crates/core/src/model/tls.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TlsConfig {
    pub email:                  Option<String>,        // ACME contact
    pub on_demand_enabled:      bool,
    pub on_demand_ask_url:      Option<String>,
    pub default_issuer:         Option<TlsIssuer>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TlsConfigPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email:                  Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_demand_enabled:      Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_demand_ask_url:      Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_issuer:         Option<Option<TlsIssuer>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "issuer", rename_all = "snake_case")]
pub enum TlsIssuer {
    Acme { directory_url: String },
    Internal,
}
```

```rust
// core/crates/core/src/model/global.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalConfig {
    pub admin_listen:   Option<String>,
    pub default_sni:    Option<String>,
    pub log_level:      Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalConfigPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin_listen: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_sni:  Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_level:    Option<Option<String>>,
}
```

```rust
// core/crates/core/src/model/policy.rs
use serde::{Deserialize, Serialize};
use crate::model::identifiers::PresetId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PresetVersion {
    pub preset_id: PresetId,
    pub version:   u32,
    pub body_json: String,           // canonical JSON of the preset body
}

/// Phase 18 supplies the resolved-attachment shape. For Phase 4 the
/// attachment is the (preset_id, version) pair held alongside the route.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyAttachment {
    pub preset_id:      PresetId,
    pub preset_version: u32,
}
```

The `Option<Option<T>>` pattern in patch types is the canonical "absent vs. set-to-None" distinction: outer `None` means "field unchanged", outer `Some(None)` means "clear the field", outer `Some(Some(v))` means "set to v".

### Tests

- `core/crates/core/src/model/tls.rs::tests::patch_distinguishes_unset_and_clear` — three round-trips covering the three states of `Option<Option<String>>`.
- `core/crates/core/src/model/global.rs::tests::patch_default_is_all_none`.
- `core/crates/core/src/model/policy.rs::tests::preset_version_serde_round_trip`.

### Acceptance command

```
cargo test -p trilithon-core model::tls && \
cargo test -p trilithon-core model::global && \
cargo test -p trilithon-core model::policy
```

### Exit conditions

- Patch-vs-clear distinction is observable in serde output.
- Three named tests pass.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- ADR-0016 (preset-version attachment).
- Architecture §6.11, §6.12.

---

## Slice 4.4 [standard] — `DesiredState` aggregate, serde round-trip, `BTreeMap` invariants

### Goal

Define `DesiredState` as the aggregate over the previous slices' types. All collections are `BTreeMap` for deterministic key ordering — the snapshot writer in Phase 5 relies on this for canonical hashing. A blanket serde round-trip test asserts equality after `serialize_then_deserialize`.

### Entry conditions

- Slices 4.1, 4.2, 4.3 complete.

### Files to create or modify

- `core/crates/core/src/model/desired_state.rs` — aggregate (new).
- `core/crates/core/src/model/mod.rs` — re-export `pub use desired_state::DesiredState;` (modify).

### Signatures and shapes

```rust
// core/crates/core/src/model/desired_state.rs
use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use crate::model::{
    identifiers::*, route::Route, upstream::Upstream,
    policy::{PolicyAttachment, PresetVersion}, tls::TlsConfig, global::GlobalConfig,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesiredState {
    /// Monotonic optimistic-concurrency anchor.
    pub version:   i64,
    pub routes:    BTreeMap<RouteId, Route>,
    pub upstreams: BTreeMap<UpstreamId, Upstream>,
    pub policies:  BTreeMap<PolicyId, PolicyAttachment>,
    pub presets:   BTreeMap<PresetId, PresetVersion>,
    pub tls:       TlsConfig,
    pub global:    GlobalConfig,
}

impl DesiredState {
    pub fn empty() -> Self { Self::default() }
}
```

### Tests

- `core/crates/core/src/model/desired_state.rs::tests::serde_round_trip` — builds a non-trivial `DesiredState` (one route, two upstreams, one preset), serialises to `serde_json::Value`, deserialises, asserts `PartialEq`.
- `tests::btreemap_iteration_is_sorted` — inserts two routes with ids `"01.."` and `"02.."` in reverse order; iterates `routes`, asserts `01..` precedes `02..`.

### Acceptance command

```
cargo test -p trilithon-core model::desired_state
```

### Exit conditions

- `DesiredState` compiles and round-trips.
- BTreeMap iteration is observably sorted.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- ADR-0009.
- Architecture §6.5 (snapshot row), §7.1.

---

## Slice 4.5 [trivial] — Patch types

### Goal

Define `RoutePatch`, `UpstreamPatch`, and the `ParsedCaddyfile` placeholder used by `Mutation::ImportFromCaddyfile`. (`GlobalConfigPatch` and `TlsConfigPatch` were defined in slice 4.3.) Patch types follow the `Option<Option<T>>` convention.

### Entry conditions

- Slice 4.4 complete.

### Files to create or modify

- `core/crates/core/src/mutation/mod.rs` — re-exports (new).
- `core/crates/core/src/mutation/patches.rs` — patch types (new).

### Signatures and shapes

```rust
// core/crates/core/src/mutation/patches.rs
use serde::{Deserialize, Serialize};
use crate::model::{
    identifiers::UpstreamId, matcher::MatcherSet, header::HeaderRules,
    redirect::RedirectRule, route::{HostPattern, RoutePolicyAttachment},
    upstream::{UpstreamDestination, UpstreamProbe},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostnames:         Option<Vec<HostPattern>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstreams:         Option<Vec<UpstreamId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matchers:          Option<MatcherSet>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers:           Option<HeaderRules>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirects:         Option<Option<RedirectRule>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_attachment: Option<Option<RoutePolicyAttachment>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled:           Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpstreamPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination:       Option<UpstreamDestination>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe:             Option<UpstreamProbe>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight:            Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_request_bytes: Option<Option<u64>>,
}

/// Phase 13 supplies the parsed-Caddyfile shape. For Phase 4 this is an
/// opaque carrier; the `apply_mutation` handler for `ImportFromCaddyfile`
/// reads `routes` and `upstreams` and merges them into `DesiredState`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParsedCaddyfile {
    pub routes:    Vec<crate::model::route::Route>,
    pub upstreams: Vec<crate::model::upstream::Upstream>,
    pub warnings:  Vec<String>,
}
```

### Tests

- `core/crates/core/src/mutation/patches.rs::tests::route_patch_serde_round_trip`.
- `tests::route_patch_default_is_all_none`.
- `tests::upstream_patch_round_trip`.

### Acceptance command

```
cargo test -p trilithon-core mutation::patches
```

### Exit conditions

- Three named tests pass.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- Architecture §7.1.

---

## Slice 4.6 [standard] — `Mutation` enum, `MutationId`, `expected_version` envelope

### Goal

Define the closed `Mutation` enum exactly as the phase reference specifies, plus the `MutationEnvelope` carrying `MutationId` and `expected_version`. Build a deserialisation helper that rejects an envelope lacking `expected_version` with the dedicated audit kind `mutation.rejected.missing-expected-version`.

### Entry conditions

- Slice 4.5 complete.

### Files to create or modify

- `core/crates/core/src/mutation/types.rs` — `Mutation` enum (new).
- `core/crates/core/src/mutation/envelope.rs` — wire envelope (new).
- `core/crates/core/src/mutation/mod.rs` — re-exports (modify).

### Signatures and shapes

```rust
// core/crates/core/src/mutation/types.rs
use serde::{Deserialize, Serialize};
use crate::model::{
    identifiers::*, route::Route, upstream::Upstream,
    tls::TlsConfigPatch, global::GlobalConfigPatch,
};
use crate::mutation::patches::{RoutePatch, UpstreamPatch, ParsedCaddyfile};
use crate::storage::types::SnapshotId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "PascalCase")]
pub enum Mutation {
    CreateRoute        { expected_version: i64, route:    Route },
    UpdateRoute        { expected_version: i64, id:       RouteId, patch: RoutePatch },
    DeleteRoute        { expected_version: i64, id:       RouteId },
    CreateUpstream     { expected_version: i64, upstream: Upstream },
    UpdateUpstream     { expected_version: i64, id:       UpstreamId, patch: UpstreamPatch },
    DeleteUpstream     { expected_version: i64, id:       UpstreamId },
    AttachPolicy       { expected_version: i64, route_id: RouteId, preset_id: PresetId, preset_version: u32 },
    DetachPolicy       { expected_version: i64, route_id: RouteId },
    UpgradePolicy      { expected_version: i64, route_id: RouteId, to_version: u32 },
    SetGlobalConfig    { expected_version: i64, patch: GlobalConfigPatch },
    SetTlsConfig       { expected_version: i64, patch: TlsConfigPatch },
    ImportFromCaddyfile{ expected_version: i64, parsed: ParsedCaddyfile },
    Rollback           { expected_version: i64, target: SnapshotId },
}

impl Mutation {
    pub fn expected_version(&self) -> i64 {
        use Mutation::*;
        match self {
            CreateRoute { expected_version, .. }
            | UpdateRoute { expected_version, .. }
            | DeleteRoute { expected_version, .. }
            | CreateUpstream { expected_version, .. }
            | UpdateUpstream { expected_version, .. }
            | DeleteUpstream { expected_version, .. }
            | AttachPolicy { expected_version, .. }
            | DetachPolicy { expected_version, .. }
            | UpgradePolicy { expected_version, .. }
            | SetGlobalConfig { expected_version, .. }
            | SetTlsConfig { expected_version, .. }
            | ImportFromCaddyfile { expected_version, .. }
            | Rollback { expected_version, .. } => *expected_version,
        }
    }

    pub fn kind(&self) -> MutationKind { /* return PascalCase variant */ }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MutationKind {
    CreateRoute, UpdateRoute, DeleteRoute,
    CreateUpstream, UpdateUpstream, DeleteUpstream,
    AttachPolicy, DetachPolicy, UpgradePolicy,
    SetGlobalConfig, SetTlsConfig, ImportFromCaddyfile, Rollback,
}
```

```rust
// core/crates/core/src/mutation/envelope.rs
use serde::{Deserialize, Serialize};
use crate::model::identifiers::MutationId;
use crate::mutation::types::Mutation;

/// Wire envelope. The `expected_version` MUST be present; absence is rejected
/// with `EnvelopeError::MissingExpectedVersion`, which the audit log writer
/// records as `mutation.rejected.missing-expected-version`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MutationEnvelope {
    pub mutation_id: MutationId,
    pub mutation:    Mutation,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EnvelopeError {
    #[error("mutation request lacks expected_version field")]
    MissingExpectedVersion,
    #[error("mutation request malformed: {detail}")]
    Malformed { detail: String },
}

/// Parse a mutation envelope from JSON bytes. Returns
/// `EnvelopeError::MissingExpectedVersion` when the embedded mutation lacks
/// the `expected_version` key. The serde-tagged form already carries the field
/// because `Mutation` is defined with it on every variant; this validator
/// catches *manually crafted* JSON that omits the field via a pre-deserialise
/// peek into the raw `serde_json::Value`.
pub fn parse_envelope(bytes: &[u8]) -> Result<MutationEnvelope, EnvelopeError>;
```

### Algorithm

`parse_envelope`:

1. `let raw: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| Malformed { detail: e.to_string() })?;`
2. Locate `raw["mutation"]`. If absent → `Malformed { detail: "missing mutation field" }`.
3. If `raw["mutation"].get("expected_version").is_none()` → `MissingExpectedVersion`.
4. Otherwise `serde_json::from_value::<MutationEnvelope>(raw)` and propagate any `Malformed`.

### Tests

- `core/crates/core/src/mutation/envelope.rs::tests::accepts_valid_envelope` — JSON with `expected_version: 5`.
- `tests::rejects_missing_expected_version` — JSON without the key, asserts `EnvelopeError::MissingExpectedVersion`.
- `tests::rejects_malformed_json`.
- `core/crates/core/src/mutation/types.rs::tests::serde_tag_is_kind` — `Mutation::CreateRoute { ... }` serialises with `"kind": "CreateRoute"`.

### Acceptance command

```
cargo test -p trilithon-core mutation::types && \
cargo test -p trilithon-core mutation::envelope
```

### Exit conditions

- `Mutation::expected_version()` returns the inner value for every variant.
- The envelope rejects missing `expected_version` deterministically.
- Four named tests pass.

### Audit kinds emitted

`mutation.rejected.missing-expected-version` (architecture §6.6) — the audit row is constructed in this slice as `AuditEvent::MutationRejectedMissingExpectedVersion`; Phase 6 flushes it.

### Tracing events emitted

None at this slice.

### Cross-references

- ADR-0012 (optimistic concurrency).
- Architecture §6.6, §7.1.

---

## Slice 4.7 [cross-cutting] — `MutationOutcome`, `MutationError`, `Diff`, `AuditEvent` integration

### Goal

Define `MutationOutcome` and `MutationError` exactly as the phase reference specifies, plus a placeholder `Diff` type and the `AuditEvent` enum (the `Display`-to-`kind` mapping appears here per architecture §6.6 "Rust `AuditEvent` ↔ wire `kind` mapping"). The `Diff` shape is an opaque structural-diff carrier; Phase 8 fleshes it out.

### Entry conditions

- Slice 4.6 complete.

### Files to create or modify

- `core/crates/core/src/mutation/error.rs` — `MutationError` (new).
- `core/crates/core/src/mutation/outcome.rs` — `MutationOutcome`, `Diff` (new).
- `core/crates/core/src/audit/mod.rs` — `AuditEvent` (new).
- `core/crates/core/src/lib.rs` — `pub mod audit;` (modify).

### Signatures and shapes

```rust
// core/crates/core/src/mutation/error.rs
use crate::model::primitive::{JsonPointer, CaddyModule};
use crate::mutation::types::MutationKind;

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum MutationError {
    #[error("validation failed: {hint} (rule: {rule:?}, path: {path:?})")]
    Validation { rule: ValidationRule, path: JsonPointer, hint: String },

    #[error("capability missing: {module:?} required by {required_by:?}")]
    CapabilityMissing { module: CaddyModule, required_by: MutationKind },

    #[error("optimistic conflict: observed version {observed_version}, mutation expected {expected_version}")]
    Conflict { observed_version: i64, expected_version: i64 },

    #[error("schema error at {field:?}: {kind:?}")]
    Schema { field: JsonPointer, kind: SchemaErrorKind },

    #[error("forbidden: {reason:?}")]
    Forbidden { reason: ForbiddenReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationRule {
    HostnameInvalid,
    UpstreamReferenceMissing,
    PolicyPresetMissing,
    DuplicateRouteId,
    PolicyAttachmentMissing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaErrorKind {
    UnknownField,
    TypeMismatch { expected: String, found: String },
    OutOfRange   { value: String, bounds: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForbiddenReason {
    RollbackTargetUnknown,
    PolicyDowngrade,
}
```

```rust
// core/crates/core/src/mutation/outcome.rs
use crate::model::desired_state::DesiredState;
use crate::audit::AuditEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationOutcome {
    pub new_state: DesiredState,
    pub diff:      Diff,
    pub kind:      AuditEvent,
}

/// Phase 8 supplies the structural-diff shape. For Phase 4 this is an
/// ordered list of changed JSON pointers plus before/after `serde_json::Value`s.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diff {
    pub changes: Vec<DiffChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffChange {
    pub path:   crate::model::primitive::JsonPointer,
    pub before: Option<serde_json::Value>,
    pub after:  Option<serde_json::Value>,
}
```

```rust
// core/crates/core/src/audit/mod.rs
use std::fmt;

/// One-to-one Rust ↔ wire mapping per architecture §6.6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEvent {
    MutationProposed,
    MutationSubmitted,
    MutationApplied,
    MutationConflicted,
    MutationRejected,
    MutationRejectedMissingExpectedVersion,
    MutationRebasedAuto,
    MutationRebasedManual,
    MutationRebaseExpired,

    ApplySucceeded,                      // -> "config.applied"
    ApplyFailed,                         // -> "config.apply-failed"
    DriftDetected,
    DriftResolved,
    ConfigRolledBack,
    ConfigRebased,

    OwnershipSentinelConflict,
    CaddyReconnected,
    CaddyUnreachable,
    CaddyCapabilityProbeCompleted,

    PolicyPresetAttached,
    PolicyPresetDetached,
    PolicyPresetUpgraded,
    PolicyRegistryMismatch,

    SecretsRevealed,
    SecretsMasterKeyRotated,

    ImportCaddyfile,
    ExportBundle,
    ExportCaddyJson,
    ExportCaddyfile,

    ToolGatewaySessionOpened,
    ToolGatewaySessionClosed,
    ToolGatewayInvoked,

    AuthLoginSucceeded,
    AuthLoginFailed,
    AuthLogout,
    AuthSessionRevoked,
    AuthBootstrapCredentialsRotated,

    DockerSocketTrustGrant,

    ProposalApproved,
    ProposalRejected,
    ProposalExpired,
}

impl fmt::Display for AuditEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use AuditEvent::*;
        let s = match self {
            MutationProposed                       => "mutation.proposed",
            MutationSubmitted                      => "mutation.submitted",
            MutationApplied                        => "mutation.applied",
            MutationConflicted                     => "mutation.conflicted",
            MutationRejected                       => "mutation.rejected",
            MutationRejectedMissingExpectedVersion => "mutation.rejected.missing-expected-version",
            MutationRebasedAuto                    => "mutation.rebased.auto",
            MutationRebasedManual                  => "mutation.rebased.manual",
            MutationRebaseExpired                  => "mutation.rebase.expired",
            ApplySucceeded                         => "config.applied",
            ApplyFailed                            => "config.apply-failed",
            DriftDetected                          => "config.drift-detected",
            DriftResolved                          => "config.drift-resolved",
            ConfigRolledBack                       => "config.rolled-back",
            ConfigRebased                          => "config.rebased",
            OwnershipSentinelConflict              => "caddy.ownership-sentinel-conflict",
            CaddyReconnected                       => "caddy.reconnected",
            CaddyUnreachable                       => "caddy.unreachable",
            CaddyCapabilityProbeCompleted          => "caddy.capability-probe-completed",
            PolicyPresetAttached                   => "policy-preset.attached",
            PolicyPresetDetached                   => "policy-preset.detached",
            PolicyPresetUpgraded                   => "policy-preset.upgraded",
            PolicyRegistryMismatch                 => "policy.registry-mismatch",
            SecretsRevealed                        => "secrets.revealed",
            SecretsMasterKeyRotated                => "secrets.master-key-rotated",
            ImportCaddyfile                        => "import.caddyfile",
            ExportBundle                           => "export.bundle",
            ExportCaddyJson                        => "export.caddy-json",
            ExportCaddyfile                        => "export.caddyfile",
            ToolGatewaySessionOpened               => "tool-gateway.session-opened",
            ToolGatewaySessionClosed               => "tool-gateway.session-closed",
            ToolGatewayInvoked                     => "tool-gateway.tool-invoked",
            AuthLoginSucceeded                     => "auth.login-succeeded",
            AuthLoginFailed                        => "auth.login-failed",
            AuthLogout                             => "auth.logout",
            AuthSessionRevoked                     => "auth.session-revoked",
            AuthBootstrapCredentialsRotated        => "auth.bootstrap-credentials-rotated",
            DockerSocketTrustGrant                 => "docker.socket-trust-grant",
            ProposalApproved                       => "proposal.approved",
            ProposalRejected                       => "proposal.rejected",
            ProposalExpired                        => "proposal.expired",
        };
        f.write_str(s)
    }
}
```

### Tests

- `core/crates/core/src/audit/mod.rs::tests::display_strings_match_six_six_vocab` — for every variant, asserts `event.to_string()` is in the §6.6 vocabulary list (imported from `core::storage::audit_vocab`). Coverage assertion: every string in `audit_vocab` MUST appear at least once on the right-hand side of `Display`.
- `tests::no_two_variants_share_a_kind` — collect all `Display` outputs into a `HashSet`; assert length equals the variant count.

### Acceptance command

```
cargo test -p trilithon-core audit
```

### Exit conditions

- `AuditEvent::Display` covers every §6.6 kind exactly once.
- `MutationOutcome` and `MutationError` compile with the documented variants.

### Audit kinds emitted

The full §6.6 vocabulary is *bound* here. None are *emitted* yet; Phase 6 wires the writer.

### Tracing events emitted

None.

### Cross-references

- Architecture §6.6 (entire vocabulary, including the `MutationRejectedMissingExpectedVersion` variant which exists specifically for this phase).

---

## Slice 4.8 [standard] — Capability-gating algorithm

### Goal

Implement the capability-gating algorithm verbatim from the phase reference. The function `Mutation::referenced_caddy_modules() -> BTreeSet<CaddyModule>` is implemented per variant. A stripped capability set causes `MutationError::CapabilityMissing` for every gated mutation.

### Entry conditions

- Slices 4.6, 4.7 complete.

### Files to create or modify

- `core/crates/core/src/mutation/capability.rs` — gating (new).

### Signatures and shapes

```rust
// core/crates/core/src/mutation/capability.rs
use std::collections::BTreeSet;
use crate::caddy::capabilities::CapabilitySet;
use crate::model::primitive::CaddyModule;
use crate::mutation::{types::Mutation, error::MutationError};

impl Mutation {
    pub fn referenced_caddy_modules(&self) -> BTreeSet<CaddyModule> { /* per-variant */ }
}

pub fn check_capabilities(
    mutation:     &Mutation,
    capabilities: &CapabilitySet,
) -> Result<(), MutationError>;
```

### Algorithm

`check_capabilities(mutation, capabilities)`:

1. Let `referenced_modules = mutation.referenced_caddy_modules()`.
2. Let `loaded_modules = &capabilities.loaded_modules`.
3. Let `missing` be the elements of `referenced_modules` not in `loaded_modules`.
4. If `missing` is empty, return `Ok(())`.
5. Otherwise let `module = missing.iter().next().unwrap().clone()` (the `BTreeSet` ordering makes this deterministic) and return `Err(MutationError::CapabilityMissing { module, required_by: mutation.kind() })`.

`referenced_caddy_modules()` per variant:

| Variant | Returns |
|---|---|
| `CreateRoute`, `UpdateRoute` | `{"http.handlers.reverse_proxy"}` if `route.upstreams` non-empty, plus `{"http.handlers.rewrite"}` if `headers.request` non-empty, plus `{"http.handlers.headers"}` if `headers.response` non-empty, plus `{"http.handlers.static_response"}` if `redirects.is_some()` |
| `DeleteRoute`, `DeleteUpstream`, `Rollback` | empty set |
| `CreateUpstream`, `UpdateUpstream` | `{"http.handlers.reverse_proxy"}` plus `{"http.health_checks.active"}` if probe is `Http` or `Tcp` (non-`Disabled`) |
| `AttachPolicy`, `UpgradePolicy` | derived from the preset body via Phase 18; for Phase 4 returns the empty set (preset registry not yet wired); a `// zd:CAP-PRESET expires:2026-12-31 reason:phase 18 wires preset module derivation` comment marks the deferred work |
| `DetachPolicy` | empty set |
| `SetGlobalConfig`, `SetTlsConfig` | `{"tls"}` if `SetTlsConfig.patch.email.is_some()` (suggests ACME) |
| `ImportFromCaddyfile` | union of `referenced_caddy_modules()` over each synthesised mutation |

### Tests

- `core/crates/core/src/mutation/capability.rs::tests::create_route_with_upstream_requires_reverse_proxy` — capability set lacking `http.handlers.reverse_proxy`; asserts `CapabilityMissing { module: CaddyModule("http.handlers.reverse_proxy"), required_by: MutationKind::CreateRoute }`.
- `tests::create_route_without_upstream_succeeds` — empty `upstreams` vec; asserts `Ok`.
- `tests::redirect_only_route_requires_static_response`.
- `tests::delete_route_requires_no_module` — empty capability set; asserts `Ok`.
- `tests::tls_email_requires_tls_module`.
- `tests::referenced_modules_is_deterministic` — same mutation across 100 invocations returns the same `BTreeSet`.

### Acceptance command

```
cargo test -p trilithon-core mutation::capability
```

### Exit conditions

- All six tests pass.
- `referenced_caddy_modules` is deterministic per variant.

### Audit kinds emitted

None directly. Phase 6 records `mutation.rejected` rows when capability gating fires.

### Tracing events emitted

None.

### Cross-references

- ADR-0013 (mitigates H5).
- Architecture §7.4 (capability probe), §6.6 (`mutation.rejected`).

---

## Slice 4.9 [standard] — `apply_mutation` per-variant pure implementation

### Goal

Implement `apply_mutation` as a pure function over `&DesiredState`, `&Mutation`, `&CapabilitySet`. Every variant has a dedicated handler returning `Result<MutationOutcome, MutationError>`. The function is free of I/O and clock reads; timestamps come from caller-supplied `UnixSeconds` already on the mutation payload.

### Entry conditions

- Slices 4.6, 4.7, 4.8 complete.

### Files to create or modify

- `core/crates/core/src/mutation/apply.rs` — entry point and per-variant handlers (new).
- `core/crates/core/src/mutation/validate.rs` — schema and pre-condition validators (new).

### Signatures and shapes

```rust
// core/crates/core/src/mutation/apply.rs
use crate::caddy::capabilities::CapabilitySet;
use crate::model::desired_state::DesiredState;
use crate::mutation::{
    types::Mutation, error::MutationError, outcome::{MutationOutcome, Diff, DiffChange},
    capability::check_capabilities,
};

/// Apply a mutation against an immutable desired state. Pure: no I/O,
/// no clock reads except via caller-supplied `UnixSeconds` already on the
/// mutation payload.
///
/// # Errors
///
/// - `MutationError::Conflict` if `mutation.expected_version()` differs from
///   `state.version`.
/// - `MutationError::CapabilityMissing` when the mutation references a Caddy
///   module absent from `capabilities`.
/// - `MutationError::Validation` for schema or pre-condition failures.
pub fn apply_mutation(
    state:        &DesiredState,
    mutation:     &Mutation,
    capabilities: &CapabilitySet,
) -> Result<MutationOutcome, MutationError>;
```

### Algorithm

`apply_mutation(state, mutation, capabilities)`:

1. **Concurrency check.** If `mutation.expected_version() != state.version`, return `Conflict { observed_version: state.version, expected_version: mutation.expected_version() }`.
2. **Capability check.** Call `check_capabilities(mutation, capabilities)?`.
3. **Schema and pre-conditions** via `validate::pre_conditions(state, mutation)?`. Per variant:
   - `CreateRoute { route }`:
     - If `state.routes.contains_key(&route.id)` → `Validation { rule: DuplicateRouteId, path: pointer_to(route.id), hint: "route id already exists" }`.
     - For each `uid` in `route.upstreams`: if `!state.upstreams.contains_key(uid)` → `Validation { rule: UpstreamReferenceMissing, path: ..., hint: ... }`.
     - For each hostname: call `validate_hostname` (slice 4.2); on error → `Validation { rule: HostnameInvalid, ... }`.
   - `UpdateRoute { id, patch }`:
     - If `!state.routes.contains_key(&id)` → `Validation { rule: ..., hint: "route does not exist" }`.
     - If `patch.upstreams` is `Some(v)`, validate each upstream id exists.
   - `DeleteRoute { id }`: route MUST exist.
   - `CreateUpstream { upstream }`: id MUST be unused.
   - `UpdateUpstream`, `DeleteUpstream`: id MUST exist.
   - `AttachPolicy { route_id, preset_id, preset_version }`: `state.routes` and `state.presets` MUST contain the referenced ids; preset MUST be at the requested version.
   - `DetachPolicy { route_id }`: route MUST currently carry an attachment.
   - `UpgradePolicy { route_id, to_version }`: `to_version` MUST be `>` current attached version (no downgrade) → `Forbidden { reason: PolicyDowngrade }` otherwise.
   - `Rollback { target }`: a Phase 5 callback resolves whether the snapshot exists; in pure-function land this slice returns `Forbidden { reason: RollbackTargetUnknown }` when the caller-supplied resolver returns `None`. (The resolver is threaded through a `RollbackContext` parameter; see Open question 5.)
4. **State application.** Build `new_state` as a clone of `state`, increment `version`. Apply the variant's mutation:
   - `CreateRoute` → `new_state.routes.insert(route.id, route)`.
   - `UpdateRoute` → fetch existing, apply patch field-by-field honouring `Option<Option<T>>`.
   - `DeleteRoute` → `new_state.routes.remove(&id)`.
   - Equivalent for upstreams, presets, attachments, global, tls.
   - `Rollback` → caller-supplied resolver returns the target snapshot's `DesiredState`; copy its body into `new_state` and set `new_state.version = state.version + 1`.
5. **Diff computation.** Build a `Diff` containing one `DiffChange` per touched JSON pointer, with `before` and `after` JSON values. The diff is structural and Phase-8-ready.
6. **Audit kind selection.** Map `MutationKind` to `AuditEvent`:
   - `CreateRoute | UpdateRoute | DeleteRoute | CreateUpstream | UpdateUpstream | DeleteUpstream | SetGlobalConfig | SetTlsConfig` → `AuditEvent::MutationApplied`.
   - `AttachPolicy` → `AuditEvent::PolicyPresetAttached`.
   - `DetachPolicy` → `AuditEvent::PolicyPresetDetached`.
   - `UpgradePolicy` → `AuditEvent::PolicyPresetUpgraded`.
   - `ImportFromCaddyfile` → `AuditEvent::ImportCaddyfile`.
   - `Rollback` → `AuditEvent::ConfigRolledBack`.
7. Return `Ok(MutationOutcome { new_state, diff, kind })`.

### Tests

- `core/crates/core/src/mutation/apply.rs::tests::create_route_succeeds`.
- `tests::create_route_with_unknown_upstream_rejected`.
- `tests::update_route_partial_patch_applies`.
- `tests::delete_route_idempotent_when_present_and_rejected_when_absent`.
- `tests::version_mismatch_returns_conflict`.
- `tests::capability_missing_returns_capability_missing`.
- `tests::policy_downgrade_forbidden`.
- `tests::policy_upgrade_strictly_increases_version`.
- `tests::rollback_resolves_via_supplied_resolver`.
- `tests::version_increments_by_one_on_success`.

### Acceptance command

```
cargo test -p trilithon-core mutation::apply::tests
```

### Exit conditions

- All ten tests pass.
- `apply_mutation` makes no I/O calls (compile-time enforced by `core` manifest forbidding `tokio`/`std::fs`).
- Returned `MutationOutcome.new_state.version == state.version + 1` on every success.

### Audit kinds emitted

Per-variant mapping above; held in the `MutationOutcome.kind` field for Phase 6 to write.

### Tracing events emitted

None at this slice (pure logic). Phase 7's `apply.started`, `apply.succeeded`, `apply.failed` consume this output.

### Cross-references

- ADR-0009, ADR-0012, ADR-0016.
- Architecture §6.6, §7.1.
- Trait signatures §5 (`DiffEngine` is the eventual home of structural diffing).

### Open questions surfaced

5. The phase reference says `apply_mutation` is "pure: no I/O, no clock reads except via the caller-supplied `UnixSeconds` timestamps already present on the mutation payload." `Rollback { target: SnapshotId }` cannot be pure without a resolver, since resolving a snapshot id to its body requires storage. The recommended resolution is a `RollbackContext` parameter that the caller fills from `Storage::get_snapshot` before invoking `apply_mutation`. Flagged for the phase reference.

---

## Slice 4.10 [cross-cutting] — Property tests, schema generation, mutation README

### Goal

Add `proptest` coverage for idempotency on `MutationId`, ordering of independent mutations, and post-condition invariants. Generate one JSON schema per mutation variant under `docs/schemas/mutations/`. Index the schemas in a `README.md`.

### Entry conditions

- Slice 4.9 complete.
- `crates/core/Cargo.toml` declares `proptest = "1"` as a dev-dependency, `schemars = "0.8"` for schema generation.

### Files to create or modify

- `core/crates/core/Cargo.toml` — `schemars` under `[dependencies]` (modify).
- `core/crates/core/src/mutation/types.rs` — derive `JsonSchema` on `Mutation` (modify).
- `core/crates/core/build.rs` — emit JSON schema files under `docs/schemas/mutations/` (new).
- `core/crates/core/tests/mutation_props.rs` — proptest harness (new).
- `docs/schemas/mutations/README.md` — index (new).
- `docs/schemas/mutations/Mutation.json` — generated (new, regenerated by build).
- One file per variant: `CreateRoute.json`, `UpdateRoute.json`, ..., `Rollback.json` — generated.

### Signatures and shapes

```rust
// core/crates/core/build.rs
fn main() {
    // Re-run if any model file changed.
    println!("cargo:rerun-if-changed=src/model");
    println!("cargo:rerun-if-changed=src/mutation");

    // Schema generation runs as a separate `xtask` binary in practice;
    // the build script invokes it. Implementation detail: emit an env var
    // pointing the test harness at the schema directory.
    let out = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-env=TRILITHON_SCHEMA_OUT_DIR={out}/../../../docs/schemas/mutations");
}
```

A separate binary `crates/core/src/bin/gen_mutation_schemas.rs` invokes `schemars::schema_for!(Mutation)` and writes one file per variant. `just check` invokes this binary and asserts `git diff --exit-code docs/schemas/mutations/`.

```rust
// core/crates/core/tests/mutation_props.rs
use proptest::prelude::*;
use trilithon_core::caddy::capabilities::CaddyCapabilities;
use trilithon_core::model::desired_state::DesiredState;
use trilithon_core::mutation::{apply::apply_mutation, types::Mutation};

proptest! {
    #[test]
    fn idempotency_on_mutation_id(state in arb_desired_state(), m in arb_compatible_mutation()) {
        let caps = caps_with_everything();
        let first  = apply_mutation(&state, &m, &caps);
        let second = apply_mutation(&state, &m, &caps);
        prop_assert_eq!(first, second);
    }

    #[test]
    fn ordering_of_independent_mutations_is_irrelevant(
        state in arb_desired_state(),
        (m1, m2) in arb_independent_pair(),
    ) {
        let caps = caps_with_everything();
        let s_a = apply_mutation(&state, &m1, &caps).unwrap().new_state;
        let s_a = apply_mutation(&s_a, &bump_version(&m2), &caps).unwrap().new_state;

        let s_b = apply_mutation(&state, &m2, &caps).unwrap().new_state;
        let s_b = apply_mutation(&s_b, &bump_version(&m1), &caps).unwrap().new_state;
        prop_assert_eq!(canonicalised(&s_a), canonicalised(&s_b));
    }

    #[test]
    fn postconditions_hold(state in arb_desired_state(), m in arb_compatible_mutation()) {
        let caps = caps_with_everything();
        if let Ok(out) = apply_mutation(&state, &m, &caps) {
            prop_assert_eq!(out.new_state.version, state.version + 1);
            // Per-variant invariants:
            // CreateRoute: route exists. DeleteRoute: route absent. UpdateRoute:
            // route's updated_at >= state.routes[id].updated_at.
        }
    }
}
```

The `docs/schemas/mutations/README.md` index lists the 13 mutation variants and links to each `*.json` file with a one-line description.

### Algorithm

The schema generator iterates the 13 `Mutation` variants and writes:

1. `Mutation.json` — the full enum schema.
2. `CreateRoute.json` ... `Rollback.json` — per-variant schemas extracted from the discriminator-tagged form.

`just check` runs the generator and asserts no git diff.

### Tests

- `core/crates/core/tests/mutation_props.rs::idempotency_on_mutation_id`.
- `core/crates/core/tests/mutation_props.rs::ordering_of_independent_mutations_is_irrelevant`.
- `core/crates/core/tests/mutation_props.rs::postconditions_hold`.
- `core/crates/core/tests/schema_drift.rs::schemas_match_committed` — runs the generator and asserts `git status --porcelain docs/schemas/mutations/` is empty.

### Acceptance command

```
cargo test -p trilithon-core --test mutation_props && \
cargo run -p trilithon-core --bin gen_mutation_schemas && \
git diff --exit-code docs/schemas/mutations/
```

### Exit conditions

- Three property tests pass with default `proptest` configuration (256 cases).
- 14 schema files exist under `docs/schemas/mutations/` (the enum file plus 13 variants).
- The README indexes every variant.
- Schemas match the committed bytes after a fresh generation.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.6.
- ADR-0012.
- Phase reference §"Tests", §"Documentation".

---

## Phase exit checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] The mutation set is closed under composition: any sequence of valid mutations produces either a valid desired state or a single, identifiable mutation failure.
- [ ] A mutation referencing an absent Caddy module is rejected with a typed error before any apply attempt (H5).
- [ ] Every mutation variant carries `expected_version: i64`; missing-envelope rejection emits `mutation.rejected.missing-expected-version`.
- [ ] Every mutation has a documented Rust type, a JSON schema, and prose pre/post-condition documentation.
- [ ] Property tests cover idempotency, ordering, and capability gating.
- [ ] Open question 5 (`apply_mutation` rollback resolver shape) is resolved by the phase reference owner before Phase 7.
