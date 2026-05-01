# Phase 18 — Policy presets — Implementation Slices

> Phase reference: [../phases/phase-18-policy-presets.md](../phases/phase-18-policy-presets.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference: `docs/phases/phase-18-policy-presets.md`.
- Architecture §4 (component view), §6.6 (audit-kind vocabulary), §6.11 (`policy_presets`), §6.12 (`route_policy_attachments`), §6.13 (`capability_probe_results`), §11 (security posture), §12.1 (tracing vocabulary).
- Trait signatures: `Storage`, `CaddyAdminClient`, `SecretsVault`, `ProbeAdapter`.
- ADRs: ADR-0013 (capability probe gates optional Caddy features), ADR-0016 (route policy attachment records preset version), ADR-0014 (secrets encrypted at rest), ADR-0001 (Caddy as the only supported reverse proxy).
- PRD: T2.2 (policy presets).
- Hazards: H5 (capability mismatch), H17 (first-time-large-hostname latency).

## Slice plan summary

| # | Slice | Primary files | Effort (h) | Depends on |
|---|-------|---------------|------------|------------|
| 18.1 | Policy core types and registry scaffolding | `crates/core/src/policy/mod.rs`, `crates/core/src/policy/presets/mod.rs` | 6 | — |
| 18.2 | Persistence migration and seeding | `crates/adapters/migrations/0018_route_policy_attachments_version.sql`, `crates/adapters/src/policy_store.rs` | 6 | 18.1 |
| 18.3 | Capability degradation table | `crates/core/src/policy/capability.rs`, `crates/core/src/policy/render.rs` | 6 | 18.1 |
| 18.4 | Preset `public-website@1` | `crates/core/src/policy/presets/public_website.rs`, `crates/adapters/tests/policy_public_website.rs` | 4 | 18.1, 18.3 |
| 18.5 | Preset `public-application@1` | `crates/core/src/policy/presets/public_application.rs`, `crates/adapters/tests/policy_public_application.rs` | 4 | 18.4 |
| 18.6 | Preset `public-admin@1` | `crates/core/src/policy/presets/public_admin.rs`, `crates/adapters/tests/policy_public_admin.rs` | 5 | 18.5 |
| 18.7 | Preset `internal-application@1` | `crates/core/src/policy/presets/internal_application.rs`, `crates/adapters/tests/policy_internal_application.rs` | 4 | 18.5 |
| 18.8 | Preset `internal-admin@1` | `crates/core/src/policy/presets/internal_admin.rs`, `crates/adapters/tests/policy_internal_admin.rs` | 4 | 18.7 |
| 18.9 | Preset `api@1` | `crates/core/src/policy/presets/api.rs`, `crates/adapters/tests/policy_api.rs` | 4 | 18.5 |
| 18.10 | Preset `media-upload@1` | `crates/core/src/policy/presets/media_upload.rs`, `crates/adapters/tests/policy_media_upload.rs` | 6 | 18.6 |
| 18.11 | Mutation pipeline (attach, detach, upgrade) | `crates/core/src/mutation.rs`, `crates/cli/src/http/policy.rs` | 6 | 18.2, 18.4–18.10 |
| 18.12 | Web UI (PolicyTab, PresetPicker, PresetUpgradePrompt, CapabilityNotice) | `web/src/features/policy/*`, `web/src/components/policy/*` | 8 | 18.11 |

---

## Slice 18.1 — Policy core types and registry scaffolding

### Goal

Land the pure `PolicyBody`, `PolicyDefinition`, and supporting types (`HeaderBundle`, `HttpsRedirect`, `IpCidr`, `BasicAuthRequirement`, `RateLimitSlot`, `BotChallengeSlot`, `CorsConfig`, `ForwardAuthSlot`, `LossyWarning`, `PolicyAttachError`). Define `PolicyDefinition::full_id` and the empty `PRESET_REGISTRY` constant. The seven preset modules are added in slices 18.4–18.10.

### Entry conditions

- Phase 17 complete.
- The `core::desired_state::Route` type from Phase 4 exists.
- The `core::capability::CapabilitySet` type from Phase 3/4 exists.

### Files to create or modify

- `core/crates/core/src/policy/mod.rs` — public type surface.
- `core/crates/core/src/policy/presets/mod.rs` — `PRESET_REGISTRY` placeholder + helper `pub fn v1_presets() -> [PolicyDefinition; 7]`.
- `core/crates/core/src/lib.rs` — add `pub mod policy;`.

### Signatures and shapes

```rust
//! `core/crates/core/src/policy/mod.rs`

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PolicyBody {
    pub headers:               HeaderBundle,
    pub https_redirect:        HttpsRedirect,
    pub ip_allowlist:          Option<Vec<IpCidr>>,
    pub basic_auth:            Option<BasicAuthRequirement>,
    pub rate_limit:            Option<RateLimitSlot>,
    pub bot_challenge:         Option<BotChallengeSlot>,
    pub body_size_limit_bytes: Option<u64>,
    pub cors:                  Option<CorsConfig>,
    pub forward_auth:          Option<ForwardAuthSlot>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HeaderBundle {
    /// Ordered map: header name → header value. Order MUST be preserved
    /// for deterministic Caddy JSON rendering.
    pub set: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum HttpsRedirect {
    Off,
    On { status: u16 }, // 301 | 302 | 307 | 308
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct IpCidr(ipnet::IpNet);

impl IpCidr {
    pub fn parse(s: &str) -> Result<Self, IpCidrError>;
    pub fn as_str(&self) -> String;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BasicAuthRequirement {
    /// The realm presented in the WWW-Authenticate challenge.
    pub realm: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RateLimitSlot {
    pub requests_per_minute: u32,
    pub key:                 RateLimitKey,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RateLimitKey { SourceIp, Token, SourceIpAndToken }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BotChallengeSlot { pub mode: BotChallengeMode }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BotChallengeMode { Required, Optional }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CorsConfig {
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
    pub allow_credentials: bool,
    pub max_age_seconds:  u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ForwardAuthSlot {
    pub upstream_url: String,
    pub copy_headers: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PolicyDefinition {
    pub id:        String,    // for example "public-website"
    pub version:   u32,       // for example 1
    pub body:      PolicyBody,
    pub changelog: String,
}

impl PolicyDefinition {
    pub fn full_id(&self) -> String { format!("{}@{}", self.id, self.version) }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AttachedSecrets {
    pub basic_auth: Option<BasicAuthCredentialsRef>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BasicAuthCredentialsRef { pub secret_id: String }

#[derive(Debug, thiserror::Error)]
pub enum PolicyAttachError {
    #[error("preset {0} requires basic-auth credentials")]
    BasicAuthCredentialsRequired(String),
    #[error("preset {0} requires a non-empty IP allowlist")]
    IpAllowlistRequired(String),
    #[error("preset {0} requires the route to enforce authentication (basic-auth, forward-auth, or upstream token gate)")]
    AuthenticationRequired(String),
    #[error("preset {preset_id} target version {target} must be greater than current {current}")]
    DowngradeRefused { preset_id: String, current: u32, target: u32 },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LossyWarning {
    CapabilityDegraded { slot: SlotName, missing_module: String },
    UnsupportedDirective { directive: String, detail: String },
    WildcardMatchSecurity { host: String, certificate_id: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SlotName {
    RateLimit, BotChallenge, IpAllowlist, BasicAuth, Cors, ForwardAuth,
    HttpsRedirect, BodySizeLimit, Headers,
}

#[derive(Debug, thiserror::Error)]
pub enum IpCidrError {
    #[error("invalid CIDR: {0}")]
    Invalid(String),
}
```

```rust
//! `core/crates/core/src/policy/presets/mod.rs`

use crate::policy::PolicyDefinition;

// Each preset module is added in its own slice (18.4–18.10).
// pub mod public_website;
// pub mod public_application;
// pub mod public_admin;
// pub mod internal_application;
// pub mod internal_admin;
// pub mod api;
// pub mod media_upload;

/// Aggregate of every V1 preset. Populated as preset modules land.
pub fn v1_presets() -> [PolicyDefinition; 7] {
    // Filled in slices 18.4 through 18.10.
    unimplemented!("preset registry assembled in slices 18.4–18.10")
}

pub static PRESET_REGISTRY: once_cell::sync::Lazy<Vec<PolicyDefinition>> =
    once_cell::sync::Lazy::new(|| v1_presets().to_vec());
```

### Algorithm

1. `IpCidr::parse` delegates to `ipnet::IpNet::from_str`. On error map to `IpCidrError::Invalid(input.to_string())`.
2. `IpCidr::serialize` writes the canonical CIDR string; `deserialize` calls `parse`.
3. `PolicyDefinition::full_id` returns `format!("{}@{}", id, version)`.

### Tests

- `core/crates/core/src/policy/mod.rs` `mod tests`:
  - `policy_body_round_trips_serde_json`.
  - `ip_cidr_parses_v4`, `ip_cidr_parses_v6`, `ip_cidr_rejects_garbage`.
  - `header_bundle_preserves_btreemap_order`.
  - `https_redirect_serde_kebab_case`.
  - `policy_definition_full_id_concatenates`.
  - `lossy_warning_capability_degraded_serialises`.
  - `attached_secrets_optional_round_trip`.

### Acceptance command

```
cargo test -p trilithon-core policy::
```

### Exit conditions

- All eight tests pass.
- `cargo build -p trilithon-core` succeeds.
- The `presets/mod.rs` `unimplemented!()` is fine for now; replaced as preset slices land.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- ADR-0016, ADR-0013, ADR-0014.
- Architecture §6.11, §6.12.

---

## Slice 18.2 — Persistence migration and seeding

### Goal

Add migration `0018_route_policy_attachments_version.sql` that introduces the `preset_version INTEGER NOT NULL DEFAULT 1` column on `route_policy_attachments`, back-fills, then drops the default. Seed `policy_presets` from `PRESET_REGISTRY` at startup; abort startup on body-mismatch with audit kind `policy.registry-mismatch`.

### Entry conditions

- Slice 18.1 shipped.
- The `route_policy_attachments` table exists from Phase 2 with columns `route_id`, `preset_id`, `attached_at`, `secrets_json`.
- The `policy_presets` table exists from Phase 2 with columns `name`, `version`, `body_json`, `changelog`.

### Files to create or modify

- `core/crates/adapters/migrations/0018_route_policy_attachments_version.sql` — DDL.
- `core/crates/adapters/src/policy_store.rs` — new module wrapping `policy_presets` and `route_policy_attachments` access.
- `core/crates/cli/src/startup.rs` — call `policy_store::seed_v1_presets`.
- `core/crates/core/src/audit.rs` — add `AuditEvent::PolicyRegistryMismatch` if not already present.

### Signatures and shapes

```sql
-- core/crates/adapters/migrations/0018_route_policy_attachments_version.sql
BEGIN;

ALTER TABLE route_policy_attachments
    ADD COLUMN preset_version INTEGER NOT NULL DEFAULT 1;

UPDATE route_policy_attachments AS rpa
SET preset_version = (
    SELECT pp.version
    FROM policy_presets pp
    WHERE pp.name = rpa.preset_id
    ORDER BY pp.version DESC
    LIMIT 1
);

-- SQLite cannot drop column defaults in place; the back-fill above guarantees
-- every row has a non-default value, and downstream INSERTs MUST pass the
-- column explicitly. The repository's lint pass (Phase 16) verifies the
-- absence of `INSERT INTO route_policy_attachments (...)` statements that
-- omit `preset_version`.

COMMIT;
```

```rust
//! `core/crates/adapters/src/policy_store.rs`

use rusqlite::Connection;
use trilithon_core::policy::PolicyDefinition;

#[derive(Debug, thiserror::Error)]
pub enum PolicyStoreError {
    #[error("storage error: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("registry mismatch: preset {preset_full_id} on disk has different body")]
    RegistryMismatch { preset_full_id: String },
    #[error("preset {0} not found")]
    NotFound(String),
}

pub struct PolicyStore;

impl PolicyStore {
    pub fn seed_v1_presets(
        conn:     &mut Connection,
        registry: &[PolicyDefinition],
    ) -> Result<SeedReport, PolicyStoreError>;

    pub fn get(
        conn:    &Connection,
        name:    &str,
        version: u32,
    ) -> Result<PolicyDefinition, PolicyStoreError>;

    pub fn list_all(
        conn: &Connection,
    ) -> Result<Vec<PolicyDefinition>, PolicyStoreError>;
}

#[derive(Debug, Eq, PartialEq)]
pub struct SeedReport { pub inserted: u32, pub updated_changelog_only: u32 }
```

### Algorithm

`seed_v1_presets`:

1. `BEGIN IMMEDIATE`.
2. For each `def` in `registry`:
   - `SELECT body_json, changelog FROM policy_presets WHERE name = ? AND version = ?`.
   - If absent: `INSERT INTO policy_presets (name, version, body_json, changelog) VALUES (?, ?, ?, ?)`. Increment `inserted`.
   - If present and `body_json` parses to the same `PolicyBody` (deep-equal): if `changelog` differs, `UPDATE` the changelog only and increment `updated_changelog_only`.
   - If present and `body_json` differs: roll back, `record_audit_event(PolicyRegistryMismatch, notes: { preset_full_id })`, return `Err(RegistryMismatch)`.
3. `COMMIT`.
4. The startup task in `cli/src/startup.rs` MUST treat `RegistryMismatch` as a fatal startup error and exit with the existing `policy-registry-mismatch` exit code.

### Tests

- `core/crates/adapters/tests/policy_store_seed.rs`:
  - `seed_inserts_seven_presets_on_empty_database` — assert seven rows post-seed; assert `inserted == 7`.
  - `seed_is_idempotent_on_unchanged_registry` — second call returns `inserted == 0` and `updated_changelog_only == 0`.
  - `seed_updates_changelog_when_only_changelog_changes`.
  - `seed_aborts_on_body_mismatch_and_emits_policy_registry_mismatch_audit` — pre-insert a row with a corrupted body; assert `RegistryMismatch`; assert one audit row with `kind = "policy.registry-mismatch"`.
- `core/crates/adapters/tests/policy_store_migration.rs`:
  - `migration_0018_adds_preset_version_column`.
  - `migration_0018_backfills_existing_rows_to_attached_preset_version`.

### Acceptance command

```
cargo test -p trilithon-adapters --test policy_store_seed --test policy_store_migration
```

### Exit conditions

- The migration runs idempotently on a database with existing attachments.
- Six tests pass.
- The seeding routine writes one `policy.registry-mismatch` audit row on body divergence and aborts.

### Audit kinds emitted

Per §6.6: `policy.registry-mismatch`.

### Tracing events emitted

Per §12.1: `storage.migrations.applied`.

### Cross-references

- ADR-0016, ADR-0006.
- Architecture §6.11, §6.12.

---

## Slice 18.3 — Capability degradation table

### Goal

Encode the slot-to-Caddy-module mapping used by the renderer to decide which slots to emit and which to drop with a `LossyWarning::CapabilityDegraded`. Implement `render(policy, route, capabilities) -> RenderResult` and `validate(result, route, capabilities) -> Result<(), PolicyValidationError>`.

### Entry conditions

- Slices 18.1, 18.2 shipped.

### Files to create or modify

- `core/crates/core/src/policy/capability.rs` — degradation table and lookup.
- `core/crates/core/src/policy/render.rs` — `RenderResult`, `render`.
- `core/crates/core/src/policy/validate.rs` — `validate`.

### Signatures and shapes

```rust
//! `core/crates/core/src/policy/capability.rs`

use crate::capability::CapabilitySet;
use crate::policy::SlotName;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DegradationPosture { Required, OptionalDegrade, AlwaysAvailable }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DegradationEntry {
    pub slot:           SlotName,
    pub caddy_module:   &'static str,
    pub posture:        DegradationPosture,
}

/// The closed degradation table. The renderer MUST consult this table
/// for every slot before emitting a Caddy JSON fragment.
pub static DEGRADATION_TABLE: &[DegradationEntry] = &[
    DegradationEntry { slot: SlotName::RateLimit,    caddy_module: "http.handlers.rate_limit",
                       posture: DegradationPosture::OptionalDegrade },
    DegradationEntry { slot: SlotName::BotChallenge, caddy_module: "http.handlers.bot_challenge",
                       posture: DegradationPosture::OptionalDegrade },
    DegradationEntry { slot: SlotName::IpAllowlist,  caddy_module: "http.matchers.remote_ip",
                       posture: DegradationPosture::AlwaysAvailable },
    DegradationEntry { slot: SlotName::BasicAuth,    caddy_module: "http.handlers.authentication",
                       posture: DegradationPosture::AlwaysAvailable },
    DegradationEntry { slot: SlotName::Cors,         caddy_module: "http.handlers.headers",
                       posture: DegradationPosture::AlwaysAvailable },
    DegradationEntry { slot: SlotName::ForwardAuth,  caddy_module: "http.handlers.forward_auth",
                       posture: DegradationPosture::OptionalDegrade },
    DegradationEntry { slot: SlotName::HttpsRedirect,    caddy_module: "http.handlers.static_response",
                       posture: DegradationPosture::AlwaysAvailable },
    DegradationEntry { slot: SlotName::BodySizeLimit,    caddy_module: "http.handlers.request_body",
                       posture: DegradationPosture::AlwaysAvailable },
    DegradationEntry { slot: SlotName::Headers,          caddy_module: "http.handlers.headers",
                       posture: DegradationPosture::AlwaysAvailable },
];

pub fn lookup(slot: SlotName) -> &'static DegradationEntry;
pub fn is_available(slot: SlotName, caps: &CapabilitySet) -> bool;
```

```rust
//! `core/crates/core/src/policy/render.rs`

use crate::capability::CapabilitySet;
use crate::desired_state::Route;
use crate::policy::{LossyWarning, PolicyDefinition};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaddyJsonFragment {
    pub anchor:  CaddyJsonAnchor,
    pub value:   serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaddyJsonAnchor {
    RouteHandler,            // appended to the route's handler chain
    ServerErrors,
    GlobalLogs,
}

#[derive(Debug, Clone)]
pub struct RenderResult {
    pub json_fragments: Vec<CaddyJsonFragment>,
    pub warnings:       Vec<LossyWarning>,
}

pub fn render(
    policy:       &PolicyDefinition,
    route:        &Route,
    capabilities: &CapabilitySet,
) -> RenderResult;
```

```rust
//! `core/crates/core/src/policy/validate.rs`

use crate::capability::CapabilitySet;
use crate::desired_state::Route;
use crate::policy::render::RenderResult;

#[derive(Debug, thiserror::Error)]
pub enum PolicyValidationError {
    #[error("blocking warning: {warning:?}")]
    BlockingWarning { warning: crate::policy::LossyWarning },
    #[error("missing required slot: {0:?}")]
    MissingRequiredSlot(crate::policy::SlotName),
}

pub fn validate(
    result:       &RenderResult,
    route:        &Route,
    capabilities: &CapabilitySet,
) -> Result<(), PolicyValidationError>;
```

### Algorithm

`render`:

1. Initialise `RenderResult { json_fragments: vec![], warnings: vec![] }`.
2. For each slot in `policy.body` that is `Some(_)`:
   - Look up the degradation entry.
   - If `is_available`: emit the corresponding Caddy JSON fragment.
   - Else if `posture == OptionalDegrade`: skip the fragment, push `LossyWarning::CapabilityDegraded { slot, missing_module: entry.caddy_module.into() }`.
   - Else if `posture == AlwaysAvailable`: this is a contract violation; emit nevertheless but log via `tracing::warn!`.
3. Always emit the headers fragment (always-available).
4. Return `RenderResult`.

`validate`:

1. For every `LossyWarning::CapabilityDegraded` in `result.warnings`: if the slot's posture is `Required` (none in V1, but a future preset MAY require), return `BlockingWarning`.
2. For each `Required` slot in `policy.body` that is `None` for this preset's contract, return `MissingRequiredSlot`.
3. Return `Ok(())`. Non-blocking warnings remain on the `RenderResult` and are written to the snapshot's `LossyWarningSet` by the caller.

### Tests

- `core/crates/core/src/policy/capability.rs` `mod tests`:
  - `every_slot_has_a_table_entry` — assert `SlotName` variants are exhaustively covered.
  - `lookup_rate_limit_returns_optional_degrade`.
- `core/crates/core/src/policy/render.rs` `mod tests`:
  - `render_emits_headers_always`.
  - `render_drops_rate_limit_on_stock_caddy_with_warning`.
  - `render_emits_rate_limit_on_enhanced_caddy`.
- `core/crates/core/src/policy/validate.rs` `mod tests`:
  - `validate_passes_with_only_non_blocking_warnings`.
  - `validate_rejects_blocking_warning`.

### Acceptance command

```
cargo test -p trilithon-core policy::
```

### Exit conditions

- All seven tests pass.
- The degradation table is exhaustive across `SlotName` variants (compile-time check via match in `lookup`).

### Audit kinds emitted

None directly. The caller writes `LossyWarning::CapabilityDegraded` to the snapshot's lossy-warning set; per Phase 5/6 conventions this surfaces in `mutation.applied` audit notes.

### Tracing events emitted

None new.

### Cross-references

- ADR-0013.
- Phase 18 task: "Degradation table fixture," "Implement `render`," "Validator consumes `RenderResult`."

---

## Slice 18.4 — Preset `public-website@1`

### Goal

Author the `public-website@1` `PolicyDefinition` with the literal field values from the phase reference. Land an integration test asserting every header, slot, and rendered Caddy JSON fragment. Add the preset to the registry.

### Entry conditions

- Slices 18.1, 18.2, 18.3 shipped.

### Files to create or modify

- `core/crates/core/src/policy/presets/public_website.rs` — preset definition.
- `core/crates/core/src/policy/presets/mod.rs` — register the preset.
- `core/crates/adapters/tests/policy_public_website.rs` — integration test.

### Signatures and shapes

```rust
//! `core/crates/core/src/policy/presets/public_website.rs`

use crate::policy::*;
use std::collections::BTreeMap;

pub fn definition() -> PolicyDefinition {
    PolicyDefinition {
        id:        "public-website".into(),
        version:   1,
        changelog: "Initial V1 preset.".into(),
        body:      PolicyBody {
            headers: HeaderBundle {
                set: {
                    let mut m = BTreeMap::new();
                    m.insert("Strict-Transport-Security".into(),
                             "max-age=31536000; includeSubDomains; preload".into());
                    m.insert("Content-Security-Policy".into(),
                             "default-src 'self'; img-src 'self' data: https:; style-src 'self' 'unsafe-inline'; script-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'self'".into());
                    m.insert("X-Content-Type-Options".into(),     "nosniff".into());
                    m.insert("Referrer-Policy".into(),            "strict-origin-when-cross-origin".into());
                    m.insert("Permissions-Policy".into(),
                             "accelerometer=(), camera=(), geolocation=(), microphone=()".into());
                    m
                },
            },
            https_redirect:        HttpsRedirect::On { status: 308 },
            ip_allowlist:          None,
            basic_auth:            None,
            rate_limit:            Some(RateLimitSlot {
                requests_per_minute: 600,
                key:                 RateLimitKey::SourceIp,
            }),
            bot_challenge:         None,
            body_size_limit_bytes: Some(10 * 1024 * 1024), // 10 MiB
            cors:                  None,
            forward_auth:          None,
        },
    }
}
```

### Algorithm

The preset is a static value. The integration test:

1. Boots a test daemon with seeded presets and an enhanced-Caddy capability set.
2. Creates a test route.
3. Submits an `AttachPolicy { preset_id: "public-website", version: 1 }` mutation.
4. Reads back the rendered Caddy JSON via the snapshot store.
5. Asserts each header byte-for-byte.
6. Asserts the HTTPS redirect status is 308.
7. Asserts the rate-limit fragment carries `requests_per_minute = 600` and `key = "source-ip"`.
8. Asserts the audit row carries `kind = "policy-preset.attached"` and `notes` includes `preset_id = "public-website"` and `preset_version = 1`.

### Tests

- `core/crates/core/src/policy/presets/public_website.rs` `mod tests`:
  - `hsts_header_is_literal`.
  - `csp_header_is_literal`.
  - `nosniff_referrer_permissions_headers_present`.
  - `https_redirect_is_308`.
  - `rate_limit_is_600_rpm_source_ip`.
  - `body_size_limit_is_10_mib`.
  - `no_basic_auth_no_ip_allowlist_no_bot_challenge_no_cors_no_forward_auth`.
- `core/crates/adapters/tests/policy_public_website.rs`:
  - `attach_renders_expected_fragments_and_writes_audit_row`.

### Acceptance command

```
cargo test -p trilithon-core policy::presets::public_website && \
cargo test -p trilithon-adapters --test policy_public_website
```

### Exit conditions

- All eight tests pass.
- The preset appears in `PRESET_REGISTRY` with `full_id() == "public-website@1"`.

### Audit kinds emitted

Per §6.6: `policy-preset.attached`, `mutation.applied`.

### Tracing events emitted

Per §12.1: `apply.started`, `apply.succeeded`.

### Cross-references

- Phase 18 task: "Author `public-website@1`."

---

## Slice 18.5 — Preset `public-application@1`

### Goal

Author the `public-application@1` preset with the literal field values from the phase reference: stricter CSP that allows `wss:` connect, `X-Frame-Options: SAMEORIGIN`, 300 RPM rate limit, bot-challenge required, 25 MiB body size.

### Entry conditions

- Slice 18.4 shipped.

### Files to create or modify

- `core/crates/core/src/policy/presets/public_application.rs`.
- `core/crates/core/src/policy/presets/mod.rs` — register.
- `core/crates/adapters/tests/policy_public_application.rs`.

### Signatures and shapes

```rust
//! `core/crates/core/src/policy/presets/public_application.rs`

pub fn definition() -> crate::policy::PolicyDefinition;
// Body fields:
// - HSTS: max-age=31536000; includeSubDomains; preload
// - CSP: "default-src 'self'; connect-src 'self' wss:; img-src 'self' data: https:; style-src 'self' 'unsafe-inline'; script-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'self'"
// - X-Frame-Options: SAMEORIGIN
// - X-Content-Type-Options: nosniff
// - Referrer-Policy: strict-origin-when-cross-origin
// - Permissions-Policy: accelerometer=(), camera=(), geolocation=(), microphone=()
// - HttpsRedirect::On { status: 308 }
// - RateLimitSlot { 300, SourceIp }
// - BotChallengeSlot { Required }
// - body_size_limit_bytes: 25 * 1024 * 1024
// - ip_allowlist: None, basic_auth: None, cors: None, forward_auth: None
```

### Algorithm

Same shape as 18.4 — static value, integration test attaches and verifies fragments.

### Tests

- `core/crates/core/src/policy/presets/public_application.rs` `mod tests`:
  - `hsts_header_literal`, `csp_header_literal`, `x_frame_options_sameorigin`.
  - `https_redirect_308`, `rate_limit_300_rpm_source_ip`.
  - `bot_challenge_required`, `body_size_25_mib`.
  - `no_basic_auth_no_ip_allowlist_no_cors_no_forward_auth`.
- `core/crates/adapters/tests/policy_public_application.rs`:
  - `attach_renders_expected_fragments_and_writes_audit_row`.
  - `attach_on_stock_caddy_drops_bot_challenge_with_warning`.

### Acceptance command

```
cargo test -p trilithon-core policy::presets::public_application && \
cargo test -p trilithon-adapters --test policy_public_application
```

### Exit conditions

- All ten tests pass.
- The preset is registered in `PRESET_REGISTRY`.

### Audit kinds emitted

Per §6.6: `policy-preset.attached`, `mutation.applied`.

### Tracing events emitted

Per §12.1: `apply.started`, `apply.succeeded`.

### Cross-references

- Phase 18 task: "Author `public-application@1`."

---

## Slice 18.6 — Preset `public-admin@1`

### Goal

Author the `public-admin@1` preset: stronger HSTS (`max-age=63072000`), strict CSP with `frame-ancestors 'none'` and `form-action 'self'`, X-Frame-Options DENY, COOP/CORP/COEP headers, basic-auth required, 60 RPM rate limit, bot-challenge required, 10 MiB body.

### Entry conditions

- Slice 18.5 shipped.
- The secrets vault from Phase 10 is available (basic-auth credentials are stored encrypted).

### Files to create or modify

- `core/crates/core/src/policy/presets/public_admin.rs`.
- `core/crates/core/src/policy/presets/mod.rs` — register.
- `core/crates/adapters/tests/policy_public_admin.rs`.

### Signatures and shapes

```rust
//! `core/crates/core/src/policy/presets/public_admin.rs`

pub fn definition() -> crate::policy::PolicyDefinition;
// HSTS: max-age=63072000; includeSubDomains; preload
// CSP: "default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'; form-action 'self'"
// X-Content-Type-Options: nosniff
// Referrer-Policy: no-referrer
// Permissions-Policy: accelerometer=(), camera=(), clipboard-read=(), clipboard-write=(self), geolocation=(), microphone=(), usb=()
// X-Frame-Options: DENY
// Cross-Origin-Opener-Policy: same-origin
// Cross-Origin-Resource-Policy: same-origin
// Cross-Origin-Embedder-Policy: require-corp
// HttpsRedirect::On { status: 308 }
// BasicAuthRequirement { realm: "Trilithon admin" }
// RateLimitSlot { 60, SourceIp }
// BotChallengeSlot { Required }
// body_size_limit_bytes: 10 * 1024 * 1024
```

### Algorithm

Static preset definition. Integration test attaches with credentials stored in the secrets vault and verifies the rendered headers and challenge slots.

### Tests

- `core/crates/core/src/policy/presets/public_admin.rs` `mod tests`:
  - `hsts_two_year_preload`, `csp_strict_frame_ancestors_none`, `permissions_policy_lists_seven_features`.
  - `coop_corp_coep_present_with_correct_values`, `x_frame_options_deny`.
  - `basic_auth_realm_trilithon_admin`, `rate_limit_60_source_ip`, `bot_challenge_required`, `body_size_10_mib`.
- `core/crates/adapters/tests/policy_public_admin.rs`:
  - `attach_with_basic_auth_credentials_succeeds`.
  - `attach_without_basic_auth_credentials_returns_basic_auth_credentials_required`.

### Acceptance command

```
cargo test -p trilithon-core policy::presets::public_admin && \
cargo test -p trilithon-adapters --test policy_public_admin
```

### Exit conditions

- All eleven tests pass.
- Attaching without `secrets.basic_auth` fails with the typed error.

### Audit kinds emitted

Per §6.6: `policy-preset.attached`, `mutation.applied`, `mutation.rejected` (on the failure path).

### Tracing events emitted

Per §12.1: `apply.started`, `apply.succeeded`.

### Cross-references

- Phase 18 task: "Author `public-admin@1`."
- ADR-0014.

---

## Slice 18.7 — Preset `internal-application@1`

### Goal

Author the `internal-application@1` preset: HSTS off, lax CSP, IP allowlist required (non-empty), 100 MiB body, no rate limit, no bot challenge.

### Entry conditions

- Slice 18.5 shipped.

### Files to create or modify

- `core/crates/core/src/policy/presets/internal_application.rs`.
- `core/crates/core/src/policy/presets/mod.rs` — register.
- `core/crates/adapters/tests/policy_internal_application.rs`.

### Signatures and shapes

```rust
//! `core/crates/core/src/policy/presets/internal_application.rs`

pub fn definition() -> crate::policy::PolicyDefinition;
// HSTS: header absent
// CSP: "default-src 'self' 'unsafe-inline'; img-src *; connect-src *"
// X-Content-Type-Options: nosniff
// Referrer-Policy: same-origin
// HttpsRedirect::Off
// IpAllowlist: required at attach time (validation rejects empty)
// body_size_limit_bytes: 100 * 1024 * 1024
// no basic_auth, no rate_limit, no bot_challenge
```

### Algorithm

Static preset. Validation (in slice 18.11) rejects attachment when `ip_allowlist` is `None` or empty. The preset's `body.ip_allowlist` is `None` here because the allowlist value is supplied at attach time per route, not encoded in the preset.

### Tests

- `core/crates/core/src/policy/presets/internal_application.rs` `mod tests`:
  - `hsts_absent`, `csp_lax_with_unsafe_inline`, `nosniff_present`, `referrer_same_origin`.
  - `https_redirect_off`, `body_size_100_mib`, `no_rate_limit_no_bot_challenge_no_basic_auth`.
- `core/crates/adapters/tests/policy_internal_application.rs`:
  - `attach_with_non_empty_allowlist_succeeds`.
  - `attach_with_empty_allowlist_returns_ip_allowlist_required`.

### Acceptance command

```
cargo test -p trilithon-core policy::presets::internal_application && \
cargo test -p trilithon-adapters --test policy_internal_application
```

### Exit conditions

- All nine tests pass.

### Audit kinds emitted

Per §6.6: `policy-preset.attached`, `mutation.applied`, `mutation.rejected`.

### Tracing events emitted

Per §12.1: `apply.started`, `apply.succeeded`.

### Cross-references

- Phase 18 task: "Author `internal-application@1`."

---

## Slice 18.8 — Preset `internal-admin@1`

### Goal

Author the `internal-admin@1` preset: HSTS off, strict CSP with `frame-ancestors 'none'`, X-Frame-Options DENY, IP allowlist required, basic-auth required, 60 RPM rate limit, 10 MiB body.

### Entry conditions

- Slice 18.7 shipped.

### Files to create or modify

- `core/crates/core/src/policy/presets/internal_admin.rs`.
- `core/crates/core/src/policy/presets/mod.rs` — register.
- `core/crates/adapters/tests/policy_internal_admin.rs`.

### Signatures and shapes

```rust
//! `core/crates/core/src/policy/presets/internal_admin.rs`

pub fn definition() -> crate::policy::PolicyDefinition;
// HSTS: absent
// CSP: "default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; frame-ancestors 'none'"
// X-Content-Type-Options: nosniff
// Referrer-Policy: no-referrer
// X-Frame-Options: DENY
// HttpsRedirect::Off (internal network is plain HTTP-OK if so configured)
// IpAllowlist: required at attach time
// BasicAuthRequirement { realm: "Trilithon internal admin" }
// RateLimitSlot { 60, SourceIp }
// body_size_limit_bytes: 10 * 1024 * 1024
```

### Algorithm

Static preset. Attachment validation rejects when either basic-auth credentials are absent or the IP allowlist is empty.

### Tests

- `core/crates/core/src/policy/presets/internal_admin.rs` `mod tests`:
  - `hsts_absent`, `csp_strict_frame_ancestors_none`, `nosniff_no_referrer_x_frame_deny`.
  - `https_redirect_off`, `basic_auth_required_realm`, `rate_limit_60_source_ip`, `body_size_10_mib`.
- `core/crates/adapters/tests/policy_internal_admin.rs`:
  - `attach_with_credentials_and_allowlist_succeeds`.
  - `attach_without_credentials_returns_basic_auth_credentials_required`.
  - `attach_without_allowlist_returns_ip_allowlist_required`.

### Acceptance command

```
cargo test -p trilithon-core policy::presets::internal_admin && \
cargo test -p trilithon-adapters --test policy_internal_admin
```

### Exit conditions

- All ten tests pass.

### Audit kinds emitted

Per §6.6: `policy-preset.attached`, `mutation.applied`, `mutation.rejected`.

### Tracing events emitted

Per §12.1: `apply.started`, `apply.succeeded`.

### Cross-references

- Phase 18 task: "Author `internal-admin@1`."

---

## Slice 18.9 — Preset `api@1`

### Goal

Author the `api@1` preset: HSTS on, CSP omitted (APIs do not render HTML), `Cache-Control: no-store`, CORS opt-in, dual rate-limit (120 RPM per source IP plus 1,200 RPM per token), 1 MiB body.

### Entry conditions

- Slice 18.5 shipped.

### Files to create or modify

- `core/crates/core/src/policy/presets/api.rs`.
- `core/crates/core/src/policy/presets/mod.rs` — register.
- `core/crates/adapters/tests/policy_api.rs`.

### Signatures and shapes

```rust
//! `core/crates/core/src/policy/presets/api.rs`

pub fn definition() -> crate::policy::PolicyDefinition;
// HSTS: max-age=31536000; includeSubDomains; preload
// CSP: omitted
// X-Content-Type-Options: nosniff
// Referrer-Policy: strict-origin-when-cross-origin
// Cache-Control: no-store
// HttpsRedirect::On { status: 308 }
// CorsConfig: present but with empty allowed_origins by default; UI surfaces an opt-in toggle
// RateLimitSlot::dual: 120 RPM per source IP plus 1,200 RPM per token
// body_size_limit_bytes: 1 * 1024 * 1024
// no bot_challenge, no basic_auth, no ip_allowlist, no forward_auth
```

The dual rate-limit requires extending `PolicyBody` or `RateLimitSlot`. The simplest representation: `pub rate_limit: Option<Vec<RateLimitSlot>>`. This slice updates `PolicyBody::rate_limit` from `Option<RateLimitSlot>` to `Option<Vec<RateLimitSlot>>` and migrates the existing presets accordingly.

### Algorithm

Static preset. Two `RateLimitSlot` entries: one with `key = SourceIp, requests_per_minute = 120`; one with `key = Token, requests_per_minute = 1200`.

### Tests

- `core/crates/core/src/policy/presets/api.rs` `mod tests`:
  - `hsts_present_csp_omitted`, `cache_control_no_store_referrer_strict_origin`.
  - `https_redirect_308`, `cors_default_empty_origins`.
  - `dual_rate_limit_120_source_ip_and_1200_token`.
  - `body_size_1_mib`, `no_bot_challenge_no_basic_auth_no_allowlist`.
- `core/crates/adapters/tests/policy_api.rs`:
  - `attach_renders_expected_fragments_and_writes_audit_row`.
  - `attach_with_cors_origins_renders_cors_fragment`.

### Acceptance command

```
cargo test -p trilithon-core policy::presets::api && \
cargo test -p trilithon-adapters --test policy_api
```

### Exit conditions

- All nine tests pass.
- Existing preset tests continue to pass after the `Vec<RateLimitSlot>` migration.

### Audit kinds emitted

Per §6.6: `policy-preset.attached`, `mutation.applied`.

### Tracing events emitted

Per §12.1: `apply.started`, `apply.succeeded`.

### Cross-references

- Phase 18 task: "Author `api@1`."

---

## Slice 18.10 — Preset `media-upload@1`

### Goal

Author the `media-upload@1` preset. Authentication MUST be required (basic-auth, forward-auth, or upstream token gate); the validator returns `PolicyAttachError::AuthenticationRequired` on an unauthenticated route. Body limit defaults to 10 GiB with a per-attachment override bounded to mebibytes-through-gibibytes. The `reverse_proxy` stanza renders verbatim.

### Entry conditions

- Slices 18.6, 18.9 shipped.
- `core::desired_state::Route` has a queryable property "is the route authentication-gated upstream of this layer."

### Files to create or modify

- `core/crates/core/src/policy/presets/media_upload.rs`.
- `core/crates/core/src/policy/presets/mod.rs` — register.
- `core/crates/core/src/policy/render.rs` — extend renderer with the verbatim `reverse_proxy` stanza emitter for this preset.
- `core/crates/adapters/tests/policy_media_upload.rs`.

### Signatures and shapes

```rust
//! `core/crates/core/src/policy/presets/media_upload.rs`

use crate::policy::*;

pub fn definition() -> PolicyDefinition;

/// Per-attachment body-size override bounds. Expressed in bytes.
pub const MIN_BODY_SIZE_OVERRIDE_BYTES: u64 = 1 * 1024 * 1024;          // 1 MiB
pub const MAX_BODY_SIZE_OVERRIDE_BYTES: u64 = 10 * 1024 * 1024 * 1024;  // 10 GiB
pub const DEFAULT_BODY_SIZE_BYTES:      u64 = 10 * 1024 * 1024 * 1024;  // 10 GiB

/// The verbatim `reverse_proxy` JSON stanza required by the phase reference.
pub fn reverse_proxy_stanza() -> serde_json::Value {
    serde_json::json!({
        "@id": "trilithon-preset-media-upload-v1",
        "handler": "reverse_proxy",
        "flush_interval": -1,
        "transport": {
            "protocol":      "http",
            "read_timeout":  "10m",
            "write_timeout": "10m",
            "dial_timeout":  "10s"
        },
        "headers": {
            "request":  { "set": { "X-Forwarded-Proto": ["{http.request.scheme}"] } },
            "response": { "set": { "X-Frame-Options":   ["DENY"] } }
        }
    })
}
```

```rust
//! Addition to the attachment validation in `core/crates/core/src/policy/validate.rs`

pub fn require_authentication_or_error(
    preset_id: &str,
    route:     &crate::desired_state::Route,
) -> Result<(), crate::policy::PolicyAttachError>;
```

### Algorithm

`require_authentication_or_error`:

1. Inspect `route` for any of: a basic-auth attachment, a forward-auth attachment, or an upstream-token-gate marker recorded in the route's metadata.
2. If none, return `Err(PolicyAttachError::AuthenticationRequired(preset_id.to_string()))`.
3. Else `Ok(())`.

`render` for `media-upload@1`:

1. Emit the headers fragment (HSTS, nosniff, Referrer-Policy: no-referrer).
2. Emit the HTTPS-redirect fragment (status 308).
3. Emit the verbatim `reverse_proxy_stanza()` as the `RouteHandler`-anchored fragment.
4. Emit the `request_body` size limit using the per-attachment override or the 10 GiB default. The override MUST be in `[MIN_BODY_SIZE_OVERRIDE_BYTES, MAX_BODY_SIZE_OVERRIDE_BYTES]`.

### Tests

- `core/crates/core/src/policy/presets/media_upload.rs` `mod tests`:
  - `hsts_present_csp_omitted_referrer_no_referrer`.
  - `https_redirect_308`, `default_body_size_10_gib`.
  - `body_size_override_below_min_rejected`, `body_size_override_above_max_rejected`.
  - `reverse_proxy_stanza_matches_verbatim_json` — assert structural equality with the literal stanza in the phase reference. Docstring forward-references hazard H17 (first-time-large-hostname latency).
- `core/crates/adapters/tests/policy_media_upload.rs`:
  - `attach_to_route_with_basic_auth_succeeds`.
  - `attach_to_route_with_forward_auth_succeeds`.
  - `attach_to_unauthenticated_route_returns_authentication_required`.
  - `body_size_override_in_bounds_renders`.

### Acceptance command

```
cargo test -p trilithon-core policy::presets::media_upload && \
cargo test -p trilithon-adapters --test policy_media_upload
```

### Exit conditions

- All ten tests pass.
- The verbatim `reverse_proxy` stanza renders byte-equivalent to the phase reference value.
- Unauthenticated routes are rejected at attach time, not at apply time.

### Audit kinds emitted

Per §6.6: `policy-preset.attached`, `mutation.applied`, `mutation.rejected`.

### Tracing events emitted

Per §12.1: `apply.started`, `apply.succeeded`.

### Cross-references

- Phase 18 task: "Author `media-upload@1`."
- Hazard H17 (cited in the test docstring as a forward reference).

---

## Slice 18.11 — Mutation pipeline (attach, detach, upgrade)

### Goal

Add the three mutation variants (`AttachPolicy`, `DetachPolicy`, `UpgradeAttachedPolicy`) to `TypedMutation`. Each runs through the standard validation pipeline including capability gating and the per-preset attachment validator. The HTTP API exposes them under `/api/v1/policy`.

### Entry conditions

- Slices 18.4–18.10 shipped (all seven presets registered).
- The mutation pipeline from Phase 4 accepts new variants.

### Files to create or modify

- `core/crates/core/src/mutation.rs` — add three variants.
- `core/crates/core/src/policy/validate.rs` — `validate_attachment(preset, route, secrets) -> Result<(), PolicyAttachError>`.
- `core/crates/cli/src/http/policy.rs` — three handlers.
- `core/crates/cli/src/http/router.rs` — mount endpoints.

### Signatures and shapes

```rust
//! Addition to `core/crates/core/src/mutation.rs`

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TypedMutation {
    // ... existing variants ...
    AttachPolicy(AttachPolicy),
    DetachPolicy(DetachPolicy),
    UpgradeAttachedPolicy(UpgradeAttachedPolicy),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AttachPolicy {
    pub route_id:         crate::desired_state::RouteId,
    pub preset_id:        String,
    pub version:          u32,
    pub secrets:          Option<crate::policy::AttachedSecrets>,
    pub ip_allowlist:     Option<Vec<crate::policy::IpCidr>>,
    pub body_size_override_bytes: Option<u64>,
    pub expected_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DetachPolicy {
    pub route_id:         crate::desired_state::RouteId,
    pub preset_id:        String,
    pub expected_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpgradeAttachedPolicy {
    pub route_id:         crate::desired_state::RouteId,
    pub preset_id:        String,
    pub target_version:   u32,
    pub expected_version: i64,
}
```

```rust
//! `core/crates/cli/src/http/policy.rs`

pub async fn attach_policy(
    State(app): State<AppState>,
    Json(req):  Json<AttachPolicy>,
) -> (StatusCode, Json<MutationResultBody>);

pub async fn detach_policy(
    State(app): State<AppState>,
    Json(req):  Json<DetachPolicy>,
) -> (StatusCode, Json<MutationResultBody>);

pub async fn upgrade_attached_policy(
    State(app): State<AppState>,
    Json(req):  Json<UpgradeAttachedPolicy>,
) -> (StatusCode, Json<MutationResultBody>);
```

### Algorithm

`validate_attachment(preset, route, secrets, ip_allowlist)`:

1. If `preset.body.basic_auth.is_some()` and `secrets.basic_auth.is_none()`: return `BasicAuthCredentialsRequired(preset.id)`.
2. If preset's contract requires an IP allowlist (`internal-application`, `internal-admin`) and `ip_allowlist.is_none() || ip_allowlist.unwrap().is_empty()`: return `IpAllowlistRequired(preset.id)`.
3. If `preset.id == "media-upload"`: call `require_authentication_or_error(preset.id, route)`.

`UpgradeAttachedPolicy` validation:

1. Look up the current attachment row.
2. If `target_version <= current_version`: return `DowngradeRefused`.
3. Run the new preset version through `validate_attachment` with the existing `secrets` and `ip_allowlist`.

### Tests

- `core/crates/adapters/tests/policy_attach.rs`:
  - `attach_happy_path_writes_attachment_row_and_audit`.
  - `attach_missing_basic_auth_returns_400_with_typed_error`.
  - `attach_missing_allowlist_returns_400_with_typed_error`.
  - `attach_unauthenticated_media_upload_returns_400`.
- `core/crates/adapters/tests/policy_detach.rs`:
  - `detach_removes_row_and_writes_audit`.
- `core/crates/adapters/tests/policy_upgrade.rs`:
  - `upgrade_to_higher_version_succeeds_and_writes_audit`.
  - `upgrade_to_same_or_lower_returns_downgrade_refused`.

### Acceptance command

```
cargo test -p trilithon-adapters --test policy_attach --test policy_detach --test policy_upgrade
```

### Exit conditions

- All seven tests pass.
- The three mutations propagate through validation, snapshot writer, and audit log.

### Audit kinds emitted

Per §6.6: `policy-preset.attached`, `policy-preset.detached`, `policy-preset.upgraded`, `mutation.applied`, `mutation.rejected`.

### Tracing events emitted

Per §12.1: `http.request.received`, `http.request.completed`, `apply.started`, `apply.succeeded`.

### Cross-references

- Phase 18 task block "Mutation pipeline."
- ADR-0016.

---

## Slice 18.12 — Web UI (PolicyTab, PresetPicker, PresetUpgradePrompt, CapabilityNotice)

### Goal

Ship the web surface: `PolicyTab` per route, `PresetPicker` showing seven cards with capability-aware sub-labels, `PresetUpgradePrompt` modal with diff, and `CapabilityNotice` inline. Accessibility: zero `vitest-axe` violations, every card has an accessible name, tab order matches registry order.

### Entry conditions

- Slice 18.11 shipped.
- The web shell from Phase 11 hosts a route detail screen.

### Files to create or modify

- `web/src/features/policy/types.ts`.
- `web/src/features/policy/PolicyTab.tsx` and `.test.tsx`.
- `web/src/features/policy/PresetPicker.tsx` and `.test.tsx`.
- `web/src/features/policy/PresetUpgradePrompt.tsx` and `.test.tsx`.
- `web/src/components/policy/CapabilityNotice.tsx` and `.test.tsx`.
- `web/src/features/policy/usePolicy.ts` — hooks for attach/detach/upgrade.

### Signatures and shapes

```typescript
// web/src/features/policy/types.ts

export type SlotName =
  | 'rate-limit' | 'bot-challenge' | 'ip-allowlist' | 'basic-auth'
  | 'cors' | 'forward-auth' | 'https-redirect' | 'body-size-limit' | 'headers';

export interface CapabilitySet {
  readonly modules: readonly string[];
}

export interface PolicyDefinitionSummary {
  readonly id: string;
  readonly version: number;
  readonly changelog: string;
}

export interface RouteAttachment {
  readonly route_id: string;
  readonly preset_id: string;
  readonly preset_version: number;
}
```

```typescript
// web/src/features/policy/PresetPicker.tsx

export function PresetPicker(props: {
  onSelect: (id: string, version: number) => void;
  capabilities: CapabilitySet;
}): JSX.Element;
```

```typescript
// web/src/features/policy/PolicyTab.tsx

export function PolicyTab(props: { routeId: string }): JSX.Element;
```

```typescript
// web/src/features/policy/PresetUpgradePrompt.tsx

export function PresetUpgradePrompt(props: {
  route: { id: string };
  current: PolicyDefinitionSummary;
  latest: PolicyDefinitionSummary;
  onConfirm: () => void;
  onCancel: () => void;
}): JSX.Element;
```

```typescript
// web/src/components/policy/CapabilityNotice.tsx

export function CapabilityNotice(props: {
  slot: SlotName;
  missingModule: string;
  docHref: string;
}): JSX.Element;
```

### Algorithm

`PresetPicker` rendering:

1. Hard-code the seven preset identifiers in registry order: `public-website`, `public-application`, `public-admin`, `internal-application`, `internal-admin`, `api`, `media-upload`.
2. For each preset, render a card with: name, one-line description, the slots it requires, and a `CapabilityNotice` for each slot whose module is missing from `capabilities.modules`.
3. Tab order MUST match registry order; the cards are rendered as `<button>` elements with `tabIndex={0}` and an `aria-label` that includes the preset id.
4. On click, call `onSelect(id, latestVersionForId)`.

`PolicyTab`:

1. `useQuery` `GET /api/v1/routes/<routeId>/policy` returns the current attachment or `null`.
2. If `null`: render `PresetPicker`. On select, prompt for credentials/allowlist as the preset requires, then `POST /api/v1/policy/attach`.
3. If attached: render the attachment summary, a Detach button, and an Upgrade button (if a higher version exists in the registry). The Upgrade button opens `PresetUpgradePrompt`.

### Tests

- `web/src/features/policy/PolicyTab.test.tsx`:
  - `attach_flow_calls_attach_endpoint_with_typed_payload`.
  - `detach_flow_calls_detach_endpoint`.
  - `upgrade_flow_opens_prompt_and_calls_upgrade_endpoint`.
- `web/src/features/policy/PresetPicker.test.tsx`:
  - `renders_seven_cards_in_registry_order`.
  - `card_has_capability_sublabel_when_module_missing`.
  - `axe_finds_zero_violations`.
  - `tab_order_cycles_in_registry_order`.
- `web/src/features/policy/PresetUpgradePrompt.test.tsx`:
  - `renders_diff_between_versions`.
  - `confirm_invokes_onconfirm`.
- `web/src/components/policy/CapabilityNotice.test.tsx`:
  - `renders_unavailable_text_with_doc_link`.

### Acceptance command

```
cd web && pnpm typecheck && pnpm lint && pnpm test --run
```

### Exit conditions

- All ten Vitest tests pass.
- `vitest-axe` reports zero violations on `PresetPicker`.
- The seven cards render in registry order; tab order matches.

### Audit kinds emitted

None directly from the web tier.

### Tracing events emitted

None directly.

### Cross-references

- Phase 18 task block "Web UI" and "Accessibility."
- ADR-0004.

---

## Phase exit checklist

- [ ] `just check` passes.
- [ ] All seven presets render to valid Caddy JSON for a representative route on stock and enhanced Caddy.
- [ ] Attaching a preset takes exactly one user action (plus secret entry where required).
- [ ] Updating a preset definition does not silently mutate any attached route; the upgrade indicator surfaces.
- [ ] Capability-degraded rendering emits `LossyWarning::CapabilityDegraded` audit rows.
- [ ] The accessibility check passes.

## Open questions

None outstanding.
