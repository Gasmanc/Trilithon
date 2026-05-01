# Phase 14 — TLS visibility and upstream health

Source of truth: [`../phases/phased-plan.md#phase-14--tls-visibility-and-upstream-health`](../phases/phased-plan.md#phase-14--tls-visibility-and-upstream-health).

> **Path-form note.** All `crates/<name>/...` paths are workspace-relative; rooted at `core/` on disk. Phase 14 introduces `crates/adapters/src/tls_inventory.rs` and `crates/adapters/src/upstream_health.rs`. See [`README.md`](README.md) "Path conventions".

> **Authoritative cross-references.** `core::caddy::CaddyClient` and `core::probe::ProbeAdapter` trait surfaces are documented in [`../architecture/trait-signatures.md`](../architecture/trait-signatures.md). The tracing event `upstream.probe.completed` and the existing `caddy.capability-probe.completed` are bound by architecture §12.1.

## Pre-flight checklist

- [ ] Phase 12 complete (preflight infrastructure and probe adapters exist).

## Tasks

### Backend / adapters crate

- [ ] **Implement the `TlsInventory` adapter.**
  - Path: `crates/adapters/src/tls_inventory.rs`.
  - Acceptance: The adapter MUST expose:

    ```rust
    pub async fn refresh_tls_inventory(
        client: &dyn CaddyClient,
        store:  &dyn Storage,
    ) -> Result<TlsInventoryReport, TlsInventoryError>;

    pub struct TlsInventoryReport {
        pub certificates: Vec<TlsCertificate>,
        pub fetched_at:   UnixSeconds,
    }
    pub struct TlsCertificate {
        pub host:        String,
        pub issuer:      String,
        pub not_before:  UnixSeconds,
        pub not_after:   UnixSeconds,
        pub renewal_state: RenewalState,
        pub source:      CertificateSource,        // Acme | Internal | Imported
    }
    ```

    The adapter MUST periodically call `GET /pki/ca/local/certificates` AND `GET /config/apps/tls/certificates` against Caddy, parse certificates, merge by host, compute `renewal_state` from `(not_after - now)`, and persist via the supplied `Storage`. The freshness algorithm MUST be implemented as numbered pseudocode:

    ```
    1. Tokio interval with period = 5 minutes.
    2. On tick: fetch GET /pki/ca/local/certificates AND /config/apps/tls/certificates.
    3. Merge by host; compute renewal_state based on (not_after - now).
    4. Persist to capability_probe_results.tls_inventory column (JSON blob).
    5. Emit tracing event `caddy.capability-probe.completed` with span field `caddy.module = "tls"`.
    ```
  - Done when: integration tests against a real Caddy assert the persisted rows and the 5-minute cadence.
  - Feature: T1.9.
- [ ] **Implement the `UpstreamHealth` adapter.**
  - Path: `crates/adapters/src/upstream_health.rs`.
  - Acceptance: The adapter MUST expose:

    ```rust
    pub async fn probe_upstream(
        probe:    &dyn ProbeAdapter,
        upstream: &Upstream,
    ) -> UpstreamProbeResult;
    ```

    The freshness algorithm MUST be implemented as numbered pseudocode:

    ```
    1. Tokio interval with period = 30 seconds (configurable).
    2. ALSO subscribe to caddy `/reverse_proxy/upstreams` long-poll endpoint where supported; on flip, fire a debounced refresh within 1 second.
    3. Refresh = for each route's upstreams, tcp_reachable() through ProbeAdapter unless route opts out.
    4. Persist last result + transition timestamps; emit tracing event `upstream.probe.completed` (added to architecture §12.1 in this same edit).
    ```
  - Done when: integration tests cover both data sources (Caddy `/reverse_proxy/upstreams` and direct TCP probes), the 30-second cadence, and the long-poll-driven debounced refresh.
  - Feature: T1.10.
- [ ] **Honour the route-level probe opt-out.**
  - Acceptance: When `disable_trilithon_probes` is set, only Caddy-reported reachability MUST be surfaced.
  - Done when: an integration test with the flag set asserts no TCP probe occurs.
  - Feature: T1.10.

### Database migrations

- [ ] **Author migration `0005_tls_and_health.sql`.**
  - Acceptance: A migration MUST add `tls_certificates` and `upstream_health` tables matching the adapter's schema, every column carrying UTC Unix seconds where time is stored.
  - Done when: a schema-introspection test asserts the columns.
  - Feature: T1.9, T1.10.

### HTTP endpoints

- [ ] **Implement `GET /api/v1/tls/certificates`.**
  - Acceptance: The endpoint MUST return the persisted certificate inventory.
  - Done when: an integration test asserts the response shape.
  - Feature: T1.9.
- [ ] **Implement `GET /api/v1/upstreams/health`.**
  - Acceptance: The endpoint MUST return the persisted upstream-health state.
  - Done when: an integration test asserts the response shape.
  - Feature: T1.10.

### Frontend

- [ ] **Render per-route TLS badge with thresholds.**
  - Path: `web/src/features/routes/RouteCard.tsx` and `web/src/features/routes/RouteCard.test.tsx`.
  - Acceptance: The badge MUST be green if expiry > 14 days, amber if 14 days >= expiry > 3 days, red if expiry <= 3 days OR `renewal_status = failed`. Vitest tests MUST be named exactly `renders_green_when_expiry_gt_14_days`, `renders_amber_when_expiry_within_14_days`, `renders_red_when_expiry_within_3_days`, `renders_red_when_renewal_failed`.
  - Done when: Vitest tests cover all four states with the named test functions.
  - Feature: T1.9.
- [ ] **Render per-route upstream-health badge.**
  - Acceptance: The badge MUST show reachable, unreachable, or probe-disabled.
  - Done when: Vitest tests cover all three states.
  - Feature: T1.10.
- [ ] **Implement the dashboard "TLS expiring soon" widget.**
  - Acceptance: The widget MUST list every certificate within 14 days of expiry.
  - Done when: a Vitest test renders the widget against a fixture.
  - Feature: T1.9.
- [ ] **Distinguish "issuing" from "applied" with ACME error surfacing.**
  - Acceptance: When an apply introduces a new managed host, the route MUST show an "issuing" indicator until Caddy reports the certificate ready; ACME failures MUST surface with actionable messages from Caddy's status endpoint, satisfying H17.
  - Done when: integration tests cover issuing → applied and issuing → error transitions; Vitest tests render both UI states.
  - Feature: T1.9 (mitigates H17).

### Freshness

- [ ] **Enforce health-state freshness windows.**
  - Acceptance: TLS state MUST refresh at most every 5 minutes; upstream-health state MUST refresh within 30 seconds of an underlying transition.
  - Done when: integration tests assert both freshness windows.
  - Feature: T1.9, T1.10.

### Tests

- [ ] **Issuing-to-applied transition test.**
  - Acceptance: A freshly added managed host MUST transition from issuing to applied or to error within the configured timeout window (default 5 minutes).
  - Done when: the integration test passes.
  - Feature: T1.9 (mitigates H17).
- [ ] **Unreachable-upstream flip within 30 seconds.**
  - Acceptance: An unreachable upstream MUST flip to red within 30 seconds.
  - Done when: the integration test passes.
  - Feature: T1.10.

## Cross-references

- ADR-0002 (Caddy JSON admin API as source of truth).
- PRD T1.9 (TLS certificate visibility), T1.10 (basic upstream health visibility).
- Architecture: "TLS inventory," "Upstream health," "Failure modes — TLS issuance latency."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] Certificates expiring within 14 days are flagged amber; within 3 days flagged red; failed renewals flagged red regardless of expiry.
- [ ] Health state updates within 30 seconds of an underlying transition.
- [ ] The user can disable Trilithon-side probes per route.
- [ ] "Issuing certificate" is a distinct visible state from "applied," and ACME errors surface with actionable messages.
