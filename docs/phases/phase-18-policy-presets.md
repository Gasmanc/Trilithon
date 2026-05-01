# Phase 18 — Policy presets

Source of truth: [`../phases/phased-plan.md#phase-18--policy-presets`](../phases/phased-plan.md#phase-18--policy-presets).

## Pre-flight checklist

- [ ] Phase 17 complete (concurrency control surface in place).
- [ ] Capability probe results from Phase 3 are persisted in `capability_probe_results` and queryable.
- [ ] The `policy_presets` and `route_policy_attachments` tables exist from the Phase 2 migrations.
- [ ] Phase 10 secrets vault is available for basic-auth credential storage.

## Tasks

### Core types

- [ ] **Define `PolicyBody` and its components.**
  - Module: `core/crates/core/src/policy/mod.rs`.
  - Acceptance: `pub struct PolicyBody { headers: HeaderBundle, https_redirect: HttpsRedirect, ip_allowlist: Option<Vec<IpCidr>>, basic_auth: Option<BasicAuthRequirement>, rate_limit: Option<RateLimitSlot>, bot_challenge: Option<BotChallengeSlot>, body_size_limit_bytes: Option<u64>, cors: Option<CorsConfig>, forward_auth: Option<ForwardAuthSlot> }`. Each component MUST have a typed Rust definition with `serde::{Serialize, Deserialize}`. `IpCidr` MUST validate CIDR notation at deserialise time.
  - Done when: serde round-trip and CIDR-validation unit tests pass.
  - Feature: T2.2.
- [ ] **Define `PolicyDefinition`.**
  - Module: `core/crates/core/src/policy/mod.rs`.
  - Acceptance: `pub struct PolicyDefinition { id: String, version: u32, body: PolicyBody, changelog: String }` with `pub fn full_id(&self) -> String` returning `<id>@<version>`.
  - Done when: a unit test asserts `full_id`.
  - Feature: T2.2.

### Preset definitions (one task per preset)

- [ ] **Author `public-website@1`.**
  - Module: `core/crates/core/src/policy/presets/public_website.rs`.
  - Acceptance: Field schema MUST match the phased-plan section exactly. HSTS header value MUST be the literal string `max-age=31536000; includeSubDomains; preload`. CSP header value MUST be `default-src 'self'; img-src 'self' data: https:; style-src 'self' 'unsafe-inline'; script-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'self'`. `X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin`, `Permissions-Policy: accelerometer=(), camera=(), geolocation=(), microphone=()`. HTTPS redirect: status 308. Rate limit: 600 RPM per source IP (slot-only). Body size: 10 MiB. No basic auth, no IP allowlist, no bot challenge, no CORS, no forward auth.
  - Done when: a unit test enumerates every field and asserts the literal value.
  - Feature: T2.2.
- [ ] **Author `public-application@1`.**
  - Module: `core/crates/core/src/policy/presets/public_application.rs`.
  - Acceptance: HSTS as above. CSP MUST be `default-src 'self'; connect-src 'self' wss:; img-src 'self' data: https:; style-src 'self' 'unsafe-inline'; script-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'self'`. Plus `X-Frame-Options: SAMEORIGIN`, the same X-Content-Type-Options, Referrer-Policy, Permissions-Policy as `public-website`. HTTPS redirect 308. Rate limit 300 RPM per source IP. Bot challenge required (slot only). Body size 25 MiB.
  - Done when: a unit test asserts every field.
  - Feature: T2.2.
- [ ] **Author `public-admin@1`.**
  - Module: `core/crates/core/src/policy/presets/public_admin.rs`.
  - Acceptance: HSTS `max-age=63072000; includeSubDomains; preload`. CSP `default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'; form-action 'self'`. `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, `Permissions-Policy: accelerometer=(), camera=(), clipboard-read=(), clipboard-write=(self), geolocation=(), microphone=(), usb=()`, `X-Frame-Options: DENY`, `Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Resource-Policy: same-origin`, `Cross-Origin-Embedder-Policy: require-corp`. Basic auth required. Rate limit 60 RPM per source IP. Bot challenge required. Body size 10 MiB.
  - Done when: a unit test asserts every field.
  - Feature: T2.2.
- [ ] **Author `internal-application@1`.**
  - Module: `core/crates/core/src/policy/presets/internal_application.rs`.
  - Acceptance: HSTS off. CSP `default-src 'self' 'unsafe-inline'; img-src *; connect-src *`. `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin`. HTTPS redirect off. IP allowlist required (non-empty). Body size 100 MiB. No basic auth, no rate limit, no bot challenge.
  - Done when: a unit test asserts every field and a separate test asserts that `attach` rejects an empty allowlist.
  - Feature: T2.2.
- [ ] **Author `internal-admin@1`.**
  - Module: `core/crates/core/src/policy/presets/internal_admin.rs`.
  - Acceptance: HSTS off. CSP `default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; frame-ancestors 'none'`. `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, `X-Frame-Options: DENY`. IP allowlist required, basic auth required, rate limit 60 RPM per source IP, body size 10 MiB.
  - Done when: a unit test asserts every field.
  - Feature: T2.2.
- [ ] **Author `api@1`.**
  - Module: `core/crates/core/src/policy/presets/api.rs`.
  - Acceptance: HSTS as above. CSP omitted. `X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin`, `Cache-Control: no-store`. HTTPS redirect 308. CORS opt-in toggle (default no `Access-Control-Allow-Origin`). Rate limit 120 RPM per source IP plus 1,200 RPM per token. Body size 1 MiB. No bot challenge.
  - Done when: a unit test asserts every field.
  - Feature: T2.2.
- [ ] **Author `media-upload@1`.**
  - Module: `core/crates/core/src/policy/presets/media_upload.rs`.
  - Acceptance: HSTS `max-age=31536000; includeSubDomains; preload`. CSP omitted. `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`. HTTPS redirect 308. **Authentication required**: the preset MUST refuse attachment to a route lacking an authentication mechanism (basic-auth, forward-auth, or upstream-enforced token gate); the validator returns `PolicyAttachError::AuthenticationRequired`. Rate limit 30 uploads per minute per token. Body size `request_body.max_size = "10gi"` (10 gibibytes), with a per-attachment override knob bounded to mebibytes-through-gibibytes. The `reverse_proxy` stanza MUST be rendered verbatim as:

    ```json
    {
      "@id": "trilithon-preset-media-upload-v1",
      "handler": "reverse_proxy",
      "flush_interval": -1,
      "transport": {
        "protocol": "http",
        "read_timeout":  "10m",
        "write_timeout": "10m",
        "dial_timeout":  "10s"
      },
      "headers": {
        "request":  { "set": { "X-Forwarded-Proto": ["{http.request.scheme}"] } },
        "response": { "set": { "X-Frame-Options":   ["DENY"] } }
      }
    }
    ```

  - Done when: a unit test asserts every field, the verbatim `reverse_proxy` JSON stanza (compared structurally), the 10 GiB body limit, the per-attachment override bounds, and the `AuthenticationRequired` rejection on an unauthenticated route. Cite hazard H17 in the test docstring as a forward reference for the first-time-large-hostname latency note.
  - Feature: T2.2.
- [ ] **Expose the registry.**
  - Module: `core/crates/core/src/policy/presets/mod.rs`.
  - Acceptance: `pub fn v1_presets() -> [PolicyDefinition; 7]` returns the seven values; `pub const PRESET_REGISTRY: &[PolicyDefinition]` aggregates them.
  - Done when: a unit test enumerates the seven presets and asserts uniqueness of `full_id`.
  - Feature: T2.2.

### Persistence

- [ ] **Migration: add `preset_version` to `route_policy_attachments`.**
  - Module: `core/crates/adapters/migrations/0018_route_policy_attachments_version.sql`.
  - Acceptance: ALTER table to add `preset_version INTEGER NOT NULL DEFAULT 1`; back-fill existing rows with the version implied by the attached preset row; remove the default after back-fill so future inserts MUST specify the version explicitly.
  - Done when: the migration runs idempotently and a smoke test asserts the column.
  - Feature: T2.2.
- [ ] **Seed the seven presets on first run.**
  - Module: `core/crates/adapters/src/policy_store.rs`.
  - Acceptance: A startup task MUST upsert every `PRESET_REGISTRY` value into `policy_presets` keyed on `(name, version)`; if a row exists with a different `body_json` for the same `(name, version)`, abort startup with a critical event.
  - Done when: an integration test asserts the seeded rows and the abort path.
  - Feature: T2.2.

### Mutation pipeline

- [ ] **`AttachPolicy` mutation.**
  - Module: `core/crates/core/src/mutation.rs`.
  - Acceptance: `pub struct AttachPolicy { route_id: RouteId, preset_id: String, version: u32, secrets: Option<AttachedSecrets>, expected_version: i64 }`. Validation MUST reject if the preset requires basic auth and `secrets.basic_auth` is absent, MUST reject if the preset requires an IP allowlist and the route's policy attachment lacks one.
  - Done when: integration tests cover happy path, missing-credentials rejection, missing-allowlist rejection.
  - Feature: T2.2.
- [ ] **`DetachPolicy` mutation.**
  - Module: `core/crates/core/src/mutation.rs`.
  - Acceptance: `pub struct DetachPolicy { route_id: RouteId, preset_id: String, expected_version: i64 }`.
  - Done when: an integration test asserts detach removes the row and produces a snapshot.
  - Feature: T2.2.
- [ ] **`UpgradeAttachedPolicy` mutation.**
  - Module: `core/crates/core/src/mutation.rs`.
  - Acceptance: `pub struct UpgradeAttachedPolicy { route_id: RouteId, preset_id: String, target_version: u32, expected_version: i64 }`. Validation MUST reject if the target version is not greater than the currently attached version.
  - Done when: integration tests cover happy upgrade and downgrade rejection.
  - Feature: T2.2.

### Renderer

- [ ] **Implement `render`.**
  - Module: `core/crates/core/src/policy/render.rs`.
  - Acceptance: `pub fn render(policy: &PolicyDefinition, route: &Route, capabilities: &CapabilitySet) -> RenderResult`. `RenderResult { json_fragments: Vec<CaddyJsonFragment>, warnings: Vec<LossyWarning> }`. Each rendered fragment MUST be a valid Caddy JSON sub-config.
  - Done when: unit tests against each of the seven presets assert the produced fragments.
  - Feature: T2.2.
- [ ] **Validator consumes `RenderResult`.**
  - Module: `core/crates/core/src/policy/validate.rs`.
  - Acceptance: `pub fn validate(result: &RenderResult, route: &Route, capabilities: &CapabilitySet) -> Result<(), PolicyValidationError>`. Blocking warnings MUST cause rejection; non-blocking warnings MUST be appended to the snapshot's `LossyWarningSet`.
  - Done when: unit tests cover blocking and non-blocking variants.
  - Feature: T2.2 (mitigates H5).

### Capability degradation

- [ ] **Degradation table fixture.**
  - Module: `core/crates/core/src/policy/capability.rs`.
  - Acceptance: A constant table MUST encode the slot/module/posture mapping from the phased-plan capability degradation table. The renderer MUST consult this table.
  - Done when: a unit test asserts the table is exhaustive across `PolicyBody` slots.
  - Feature: T2.2 (mitigates H5).
- [ ] **Stock-Caddy degradation integration test.**
  - Module: `core/crates/adapters/tests/policy_degradation_stock.rs`.
  - Acceptance: A route with `public-admin@1` on a stock Caddy MUST apply with `rate_limit` and `bot_challenge` slots omitted and `LossyWarning::CapabilityDegraded` emitted for each.
  - Done when: the test passes.
  - Feature: T2.2.
- [ ] **Enhanced-Caddy active integration test.**
  - Module: `core/crates/adapters/tests/policy_degradation_enhanced.rs`.
  - Acceptance: The same route on an enhanced Caddy build MUST apply with both slots active and no degradation warning.
  - Done when: the test passes against a Caddy build with `caddy-ratelimit` and a bot-challenge module loaded.
  - Feature: T2.2.

### Web UI

- [ ] **Implement `PolicyTab`.**
  - Path: `web/src/features/policy/PolicyTab.tsx`.
  - Acceptance: `export function PolicyTab(props: { routeId: string }): JSX.Element`. The tab MUST host attach, detach, and upgrade actions and MUST render the per-route policy badge.
  - Done when: a Vitest test exercises attach, detach, upgrade against a stubbed adapter.
  - Feature: T2.2.
- [ ] **Implement `PresetPicker`.**
  - Path: `web/src/features/policy/PresetPicker.tsx`.
  - Acceptance: `export function PresetPicker(props: { onSelect: (id: string, version: number) => void; capabilities: CapabilitySet }): JSX.Element`. Renders seven cards labelled `public-website`, `public-application`, `public-admin`, `internal-application`, `internal-admin`, `api`, `media-upload`. Each card MUST display a capability-aware sub-label when a slot is omitted on the current Caddy build.
  - Done when: a Vitest test asserts the seven cards and the sub-label rendering.
  - Feature: T2.2.
- [ ] **Implement `PresetUpgradePrompt`.**
  - Path: `web/src/features/policy/PresetUpgradePrompt.tsx`.
  - Acceptance: `export function PresetUpgradePrompt(props: { route: Route; latest: PolicyDefinition }): JSX.Element`. Shows a diff modal between the attached version and the latest definition.
  - Done when: a Vitest test exercises the prompt and the upgrade action.
  - Feature: T2.2.
- [ ] **Implement `CapabilityNotice`.**
  - Path: `web/src/components/policy/CapabilityNotice.tsx`.
  - Acceptance: `export function CapabilityNotice(props: { slot: SlotName; missingModule: string; docHref: string }): JSX.Element`. Inline notice "unavailable on this Caddy build" with the documentation link.
  - Done when: a Vitest test asserts the rendered text and link target.
  - Feature: T2.2.

### Per-preset integration tests

- [ ] **Integration test: `public-website@1`.**
  - Module: `core/crates/adapters/tests/policy_public_website.rs`.
  - Acceptance: Loads a fresh test harness, attaches the preset to a sample route, asserts the rendered Caddy JSON contains every header and slot specified above, asserts the audit row.
  - Done when: the test passes.
  - Feature: T2.2.
- [ ] **Integration test: `public-application@1`.**
  - Module: `.../policy_public_application.rs`.
  - Done when: the test passes.
  - Feature: T2.2.
- [ ] **Integration test: `public-admin@1`.**
  - Module: `.../policy_public_admin.rs`.
  - Done when: the test passes.
  - Feature: T2.2.
- [ ] **Integration test: `internal-application@1`.**
  - Module: `.../policy_internal_application.rs`.
  - Done when: the test passes.
  - Feature: T2.2.
- [ ] **Integration test: `internal-admin@1`.**
  - Module: `.../policy_internal_admin.rs`.
  - Done when: the test passes.
  - Feature: T2.2.
- [ ] **Integration test: `api@1`.**
  - Module: `.../policy_api.rs`.
  - Done when: the test passes.
  - Feature: T2.2.
- [ ] **Integration test: `media-upload@1`.**
  - Module: `.../policy_media_upload.rs`.
  - Done when: the test passes.
  - Feature: T2.2.

### Accessibility

- [ ] **Preset picker passes axe.**
  - Module: `web/src/features/policy/PresetPicker.test.tsx`.
  - Acceptance: A `vitest-axe` assertion MUST find zero violations. Every preset card MUST have an accessible name. Tab order MUST cycle through the cards in registry order.
  - Done when: the test passes.
  - Feature: T2.2.

### Code/database consistency

- [ ] **Startup consistency check.**
  - Module: `core/crates/cli/src/startup.rs`.
  - Acceptance: A startup task MUST verify every code-defined preset has a matching `policy_presets` row; mismatch logs `policy.registry-mismatch` and aborts startup.
  - Done when: an integration test mutates the row and asserts the abort.
  - Feature: T2.2.

## Cross-references

- ADR-0013 (capability probe gates optional Caddy features).
- PRD T2.2 (policy presets).
- Architecture: `policy_presets`, `route_policy_attachments`, "Capability-aware degradation."
- Hazards: H5 (Capability mismatch).

## Sign-off checklist

- [ ] `just check` passes.
- [ ] All seven presets render to valid Caddy JSON for a representative route on stock and enhanced Caddy.
- [ ] Attaching a preset takes exactly one user action (plus secret entry where required).
- [ ] Updating a preset definition does not silently mutate any attached route; the upgrade indicator surfaces.
- [ ] Capability-degraded rendering emits `LossyWarning::CapabilityDegraded` audit rows.
- [ ] The accessibility check passes.
