# Phase 9 — HTTP API surface (read + mutate)

Source of truth: [`../phases/phased-plan.md#phase-9--http-api-surface-read--mutate`](../phases/phased-plan.md#phase-9--http-api-surface-read--mutate).

## Pre-flight checklist

- [ ] Phase 8 complete (mutation, snapshot, audit, drift are all in place).

## Tasks

### Backend / adapters crate

- [ ] **Stand up the `axum` HTTP server.**
  - Acceptance: An `axum`-based HTTP server MUST live in `crates/adapters/` and MUST be the only HTTP surface; `core` MUST remain pure.
  - Done when: the server compiles, binds the configured port, and serves `/api/v1/health` with 200.
  - Feature: T1.13 (server side).
- [ ] **Loopback binding by default.**
  - Acceptance: The listener MUST bind `127.0.0.1:<port>` by default; binding `0.0.0.0` MUST require `network.allow_remote_binding = true` and MUST emit a stark startup warning, satisfying H1 and T1.13.
  - Done when: an integration test with the flag absent rejects external bind attempts; with the flag present the warning is logged.
  - Feature: T1.13 (mitigates H1).
- [ ] **Authentication middleware for sessions and tokens.**
  - Acceptance: Middleware MUST validate session cookies against `sessions` and tool-gateway tokens against `gateway_tokens`; both MUST reject with `401` on absence or invalidity.
  - Done when: integration tests cover happy path, missing credential, and invalid credential for both schemes.
  - Feature: T1.14.
- [ ] **Argon2id password hashing with RFC 9106 first-recommendation parameters.**
  - Acceptance: Password hashing MUST use the `argon2` crate with `m_cost=19456 KiB`, `t_cost=2`, `p_cost=1`. Hashes MUST live only in `users`.
  - Done when: a unit test asserts the parameters and a property test asserts hash uniqueness across distinct passwords.
  - Feature: T1.14.
- [ ] **Implement bootstrap-account flow.**
  - Acceptance: On first startup with empty `users`, Trilithon MUST generate a random 24-character password, write it to `<data_dir>/bootstrap-credentials.txt` with mode `0600`, and log a single line directing the user to the file. The credentials MUST NOT appear in process arguments, environment variables, or any other log line, satisfying H13.
  - Done when: an integration test asserts the file mode, the single log line, and absence in environment, args, and other logs.
  - Feature: T1.14 (mitigates H13).
- [ ] **Force password change on bootstrap login.**
  - Acceptance: Login with bootstrap credentials MUST require an immediate password change before any other endpoint becomes reachable.
  - Done when: an integration test exercising the bootstrap login asserts a step-up redirect to `change-password`.
  - Feature: T1.14.
- [ ] **Rate-limit `POST /auth/login`.**
  - Acceptance: Login MUST tolerate at most five failures per source address per minute; further attempts MUST back off exponentially to a 60-second ceiling.
  - Done when: an integration test asserts the threshold and the backoff curve.
  - Feature: T1.14.

### HTTP endpoints

- [ ] **Implement auth endpoints.**
  - Acceptance: `POST /api/v1/auth/login`, `POST /api/v1/auth/logout`, and `POST /api/v1/auth/change-password` MUST be implemented.
  - Done when: integration tests cover each endpoint.
  - Feature: T1.14.
- [ ] **Implement `GET /api/v1/capabilities`.**
  - Acceptance: The endpoint MUST return the cached Caddy capability probe result.
  - Done when: an integration test against a live Caddy returns the cached payload.
  - Feature: T1.11.
- [ ] **Implement `POST /api/v1/mutations`.**
  - Acceptance: The endpoint MUST accept any variant of the typed mutation set and return the resulting snapshot identifier and `config_version`. Every POST/PATCH/DELETE mutation endpoint MUST accept the envelope `{ "expected_version": <i64>, "body": { ... } }` and MUST return `409 Conflict` if the version is stale. The Phase 17 work surfaces the existing 409 to the user; the field MUST be threaded from Phase 4 onward. Requests that omit `expected_version` MUST be rejected with 400 and the audit row MUST record `mutation.rejected.missing-expected-version`.
  - Done when: integration tests cover at least one mutation per Tier 1 variant, including a missing-`expected_version` test that asserts the 400 response and the audit row kind.
  - Feature: T1.6, T1.8.
- [ ] **Implement snapshot read endpoints.**
  - Acceptance: `GET /api/v1/snapshots`, `GET /api/v1/snapshots/{id}`, and `GET /api/v1/snapshots/{id}/diff/{other_id}` MUST be implemented.
  - Done when: integration tests cover all three.
  - Feature: T1.2.
- [ ] **Implement `GET /api/v1/audit` with filters.**
  - Acceptance: The endpoint MUST accept filters for time range, actor, event type, and correlation identifier with default page 100, maximum 1000.
  - Done when: integration tests assert pagination and every filter.
  - Feature: T1.7.
- [ ] **Implement drift endpoints.**
  - Acceptance: `GET /api/v1/drift/current`, `POST /api/v1/drift/{event_id}/adopt`, `POST /api/v1/drift/{event_id}/reapply`, `POST /api/v1/drift/{event_id}/defer` MUST be implemented.
  - Done when: integration tests cover each transition.
  - Feature: T1.4.
- [ ] **Implement `GET /api/v1/health`.**
  - Acceptance: The endpoint MUST always return 200 once the daemon is fully started.
  - Done when: an integration test confirms 200 within five seconds of `trilithon run`.
  - Feature: T1.13.
- [ ] **Serve the OpenAPI document at `/api/v1/openapi.json`.**
  - Acceptance: The OpenAPI document MUST be generated from typed handlers via `utoipa`.
  - Done when: an integration test fetches the document and validates it against the OpenAPI 3.1 schema.
  - Feature: T1.13.

### Concurrency and conflicts

- [ ] **Surface `409 Conflict` on stale `config_version`.**
  - Acceptance: A mutation with a stale `config_version` MUST return a typed 409 conflict response, satisfying H8 substrate.
  - Done when: an integration test simulating concurrent mutations asserts the 409.
  - Feature: T1.8 (substrate for T2.10).

### Tests

- [ ] **Unauthenticated mutation returns 401.**
  - Acceptance: Any unauthenticated request to a mutation endpoint MUST return 401.
  - Done when: an integration test asserts the response.
  - Feature: T1.14.
- [ ] **Bootstrap flow creates the credentials file with mode 0600.**
  - Acceptance: An integration test on a fresh data directory MUST observe the file with mode `0600`.
  - Done when: the test passes on macOS and Linux runners.
  - Feature: T1.14.
- [ ] **Successful mutation produces a snapshot, audit row, and 200 response.**
  - Acceptance: An integration test exercising a successful mutation MUST observe one new snapshot row, one new audit row, and a 200 response.
  - Done when: the test passes.
  - Feature: T1.6, T1.7.

### Documentation

- [ ] **Document the API surface in `docs/`.**
  - Acceptance: A `docs/api/README.md` MUST link the OpenAPI document and describe authentication, loopback default, and the bootstrap flow.
  - Done when: the README exists.
  - Feature: T1.13.

## Cross-references

- ADR-0011 (loopback-only by default with explicit opt-in for remote access).
- ADR-0012 (optimistic concurrency on monotonic `config_version`).
- ADR-0009 (audit log invariants).
- PRD T1.13 (web UI delivery — server side), T1.14 (authentication and session management), T1.8 (route CRUD), T1.6 (typed mutation API), T1.7 (audit log).
- Architecture: "HTTP surface," "Authentication," "Bootstrap flow," "Loopback binding."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] No mutation endpoint is reachable without an authenticated session or a valid tool-gateway token.
- [ ] Sessions are stored server-side and are revocable via `POST /auth/logout` and via an admin operation.
- [ ] The bootstrap account flow satisfies every clause of H13.
- [ ] Loopback-only binding is the default; remote binding requires an explicit flag and logs a warning.
- [ ] Opening `http://127.0.0.1:7878/api/v1/health` after first start returns a 200 within five seconds of `trilithon run`.
