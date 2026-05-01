# Phase 3 — Caddy adapter and capability probe

Source of truth: [`../phases/phased-plan.md#phase-3--caddy-adapter-and-capability-probe`](../phases/phased-plan.md#phase-3--caddy-adapter-and-capability-probe).

## Pre-flight checklist

- [ ] Phase 2 complete; persistence is available.
- [ ] A Caddy 2.8 (or later) instance is reachable on the configured admin endpoint during integration tests.

## Tasks

### Backend / core crate

- [ ] **Define the `CaddyClient` trait.**
  - Acceptance: `crates/core/src/caddy.rs` MUST expose async methods `get_config`, `load_config`, `patch_config`, `list_modules`, `list_certificates`, and `list_upstreams`. The trait MUST be free of HTTP types.
  - Done when: `cargo test -p trilithon-core caddy::tests::trait_is_pure` passes and the trait has no `hyper` or `reqwest` references.
  - Feature: T1.11 (capability probe), T1.1 (apply substrate).
- [ ] **Encode typed `CaddyError` variants.**
  - Acceptance: A `CaddyError` enum MUST cover connection-refused, validation-rejection, version-skew (H9), and timeout, each as a distinct variant with a stable identifier.
  - Done when: every method's `Result` is `Result<_, CaddyError>` and a unit test exercises every variant via a fake.
  - Feature: T1.11.
- [ ] **Define the `CaddyCapabilities` value type.**
  - Acceptance: A `CaddyCapabilities` record MUST capture the loaded modules, the Caddy version, and an opaque probe timestamp (UTC Unix seconds).
  - Done when: serde round-trip and `Eq`/`Hash` are exercised by unit tests.
  - Feature: T1.11.

### Backend / adapters crate

- [ ] **Implement `HyperCaddyClient` over Unix socket and loopback.**
  - Acceptance: `crates/adapters/src/caddy/hyper_client.rs` MUST implement `CaddyClient` using `hyper`, supporting a Unix-socket connector and a `127.0.0.1` connector configurable via daemon configuration.
  - Done when: integration tests against a real Caddy 2.8 binary pass via both transports.
  - Feature: T1.11.
- [ ] **Reject non-loopback admin endpoints by configuration validation.**
  - Acceptance: Configuration validation MUST reject any admin endpoint whose host is not `localhost`, `127.0.0.1`, `::1`, or a Unix socket path, satisfying H1. The optional `--allow-remote-admin` flag is OUT OF SCOPE FOR V1 and MUST exit `2`.
  - Done when: an integration test with a non-loopback admin URI exits `3`, and `--allow-remote-admin` exits `2` with a documented error.
  - Feature: T1.11 (mitigates H1).
- [ ] **Run the capability probe at startup.**
  - Acceptance: At startup the daemon MUST call `GET /config/apps` and `GET /reverse_proxy/upstreams`, parse loaded modules, and store the result in an in-memory cache and a `caddy_capabilities` row.
  - Done when: an integration test observes both the cache and the persisted row within one second of Caddy connectivity.
  - Feature: T1.11.
- [ ] **Reconnect loop with exponential backoff capped at 30 seconds.**
  - Acceptance: A reconnect loop MUST run capability probes on every fresh connection, with backoff starting at 250 ms and doubling to 30 seconds.
  - Done when: an integration test that kills and restarts Caddy observes a fresh probe within 35 seconds and bounded reconnect attempts.
  - Feature: T1.11.
- [ ] **Implement the ownership sentinel write.**
  - Acceptance: At startup Trilithon MUST read Caddy's running configuration, locate `@id: "trilithon-owner"`, and either create it (writing the daemon's installation identifier) or refuse to proceed if a different installation identifier is present, satisfying H12.
  - Done when: an integration test with a foreign sentinel exits `3` with a human-readable error referencing the conflicting identifier.
  - Feature: T1.11 (mitigates H12).
- [ ] **Implement the `--takeover` override with audit.**
  - Acceptance: `--takeover` MUST overwrite the sentinel and write an `OwnershipSentinelTakeover` audit row recording both identifiers.
  - Done when: an integration test exercises takeover and observes the audit row (audit table created in Phase 2; row writer comes online in Phase 6 — the row stub is acceptable here and asserted via direct `Storage` insert).
  - Feature: T1.11.
- [ ] **Propagate `traceparent` on every admin call.**
  - Acceptance: Every Caddy admin request MUST carry the active correlation identifier in a `traceparent` header.
  - Done when: a unit test on the `HyperCaddyClient` asserts the header for every method.
  - Feature: T1.7 substrate (H6 / H10 cross-cut).

### Tests

- [ ] **Integration tests against real Caddy 2.8.**
  - Acceptance: Integration tests in `crates/adapters/tests/caddy/` MUST launch a real Caddy 2.8 binary per test and exercise capability probe, version-skew detection, and reconnect.
  - Done when: `cargo test -p trilithon-adapters caddy` passes on macOS and Linux runners with Caddy 2.8 available.
  - Feature: T1.11.
- [ ] **Capability probe is queryable within one second.**
  - Acceptance: An integration test MUST assert the probe result is available to the rest of the daemon within one second of Caddy connectivity.
  - Done when: the timing assertion holds in the integration suite.
  - Feature: T1.11.

### Documentation

- [ ] **Document loopback-only default and the takeover flag.**
  - Acceptance: `core/README.md` MUST document the loopback default, the takeover flag, and the H12 ownership-sentinel semantics.
  - Done when: the section exists and references ADR-0011.
  - Feature: T1.11.

## Cross-references

- ADR-0001 (Caddy as the only supported reverse proxy).
- ADR-0002 (Caddy JSON admin API as source of truth).
- ADR-0011 (loopback-only by default).
- ADR-0013 (capability probe gates optional Caddy features).
- PRD T1.11 (capability probe), T1.1 (configuration ownership loop substrate).
- Architecture: "Caddy adapter — admin API," "Ownership sentinel," "Capability probe."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] Trilithon refuses to start with exit code `3` if the Caddy admin endpoint configuration points to a non-loopback address without `--allow-remote-admin`; `--allow-remote-admin` itself exits `2`.
- [ ] The capability probe result is available to the rest of the daemon within one second of Caddy connectivity.
- [ ] An ownership sentinel collision exits `3` with a human-readable error referencing the conflicting installation identifier.
- [ ] All Caddy admin calls carry the active correlation identifier in a `traceparent` header.
