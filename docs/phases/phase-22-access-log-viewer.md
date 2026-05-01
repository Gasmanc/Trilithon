# Phase 22 — Access log viewer and explanation engine

Source of truth: [`../phases/phased-plan.md#phase-22--access-log-viewer-and-explanation-engine`](../phases/phased-plan.md#phase-22--access-log-viewer-and-explanation-engine).

## Pre-flight checklist

- [ ] Phase 21 complete.

## Tasks

### Backend / adapters crate

- [ ] **Implement the `access_log_store` adapter.**
  - Acceptance: Trilithon MUST configure Caddy to ship access logs in JSON to a Unix socket or file owned by Trilithon; the adapter MUST ingest into a rolling on-disk store sized by configuration (default 10 GiB) with oldest-first eviction. The format MUST be one append-only file per hour with a small index.
  - Done when: integration tests cover ingest, eviction, and the index.
  - Feature: T2.5.
- [ ] **Surface a 90% capacity alarm.**
  - Acceptance: The store MUST emit a tracing event when usage reaches 90% capacity.
  - Done when: an integration test asserts the alarm fires.
  - Feature: T2.5.

### Backend / core crate

- [ ] **Implement structured filters.**
  - Acceptance: Filters MUST cover host, status code, method, path, source address, latency bucket, and time range. Index dimensions MUST be host, status code, method, source address, time range; path-pattern filters MUST stream-scan.
  - Done when: unit tests cover every filter and a property test asserts no false negatives.
  - Feature: T2.5.
- [ ] **Implement the `Explanation` engine.**
  - Acceptance: Given an access log entry, the engine MUST correlate with the route configuration that handled it (matching on host, then path, then method), the policy attached, any rate-limit or access-control decision recorded by Caddy, and the upstream response. The result MUST be a typed `Explanation` value with one decision per layer.
  - Done when: a unit test corpus exercises every decision class.
  - Feature: T2.6.

### HTTP endpoints

- [ ] **Implement `GET /api/v1/access-logs`.**
  - Acceptance: The endpoint MUST accept the structured filter set and return paginated results.
  - Done when: integration tests cover happy path and pagination.
  - Feature: T2.5.
- [ ] **Implement `GET /api/v1/access-logs/tail`.**
  - Acceptance: A server-sent-events endpoint MUST stream new lines through the active filter set; backpressure MUST drop old buffered lines with a typed warning event rather than block the producer.
  - Done when: an integration test exercises tail and the backpressure event.
  - Feature: T2.5.
- [ ] **Implement `POST /api/v1/access-logs/{entry_id}/explain`.**
  - Acceptance: The endpoint MUST return the typed `Explanation` value.
  - Done when: an integration test asserts the response.
  - Feature: T2.6.

### Frontend

- [ ] **Implement the access log viewer page.**
  - Acceptance: The page MUST host a filter bar, a virtualised table, a live-tail toggle, and a per-row "Explain" button opening a side panel showing the decision trace.
  - Done when: Vitest tests cover the filter bar, the live-tail toggle, and the explain panel.
  - Feature: T2.5, T2.6.

### Performance

- [ ] **Filters apply under 200 milliseconds against 10-million-line stores.**
  - Acceptance: The filter performance MUST be under 200 milliseconds against a rolling store of 10 million lines on the reference hardware.
  - Done when: a CI performance harness asserts the timing.
  - Feature: T2.5.

### Tests

- [ ] **Synthetic 10-million-line corpus.**
  - Acceptance: A viewer MUST satisfy the latency budget against a 10-million-line synthetic store.
  - Done when: the integration test passes.
  - Feature: T2.5.
- [ ] **Explanation covers 95% of access log entries.**
  - Acceptance: For 95% of access log entries on a representative corpus, the explanation MUST trace every decision to a specific configuration object.
  - Done when: the integration test passes.
  - Feature: T2.6.
- [ ] **Logs surfaced through the gateway are wrapped in the H16 envelope.**
  - Acceptance: Logs surfaced through the Phase 19 gateway MUST be wrapped in the typed envelope.
  - Done when: an integration test asserts the envelope on `read.access-logs`.
  - Feature: T2.5 (mitigates H16).

## Cross-references

- ADR-0008 (bounded typed tool gateway — log envelope).
- PRD T2.5 (access log viewer), T2.6 (Caddy access log explanation).
- Architecture: "Access log store," "Explanation engine," "Backpressure."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] The viewer streams new lines without manual refresh.
- [ ] Filters apply in under 200 milliseconds against a rolling store of 10 million lines.
- [ ] Storage size is configurable; oldest entries are evicted first.
- [ ] For 95% of access log entries, the explanation traces every decision to a specific configuration object.
