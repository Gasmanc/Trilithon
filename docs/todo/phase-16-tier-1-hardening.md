# Phase 16 — Tier 1 hardening and integration test sweep — Implementation Slices

> Phase reference: [../phases/phase-16-tier-1-hardening.md](../phases/phase-16-tier-1-hardening.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference [phase-16-tier-1-hardening.md](../phases/phase-16-tier-1-hardening.md).
- Architecture sections: §6 (data model, all subsections), §6.6 (`audit_log` and the V1 `kind` vocabulary), §10 (failure model — every row maps to a slice in this phase), §11 (security posture), §12.1 (tracing vocabulary), §13 (performance budget — every row maps to a performance-verification slice), §14 (upgrade and migration).
- Trait signatures: every trait in `trait-signatures.md` is exercised by at least one failure-mode test in this phase.
- ADRs: ADR-0001 through ADR-0014 are reviewed end-to-end by the security-review slice.
- PRD T1.1 through T1.15 (all Tier 1 features).
- Hazards: H1 through H17. The security review confirms a written paragraph against the implementation for every hazard.

## Phase 16 in context

Phase 16 is the Tier 1 closing gate. It introduces no new product surface; every slice exercises, measures, or documents code shipped in Phases 1 through 15. Phase 16 succeeds when:

- Every failure mode in architecture §10 has at least one passing integration test.
- Every performance budget in architecture §13 has at least one passing measurement.
- Every hazard from H1 through H17 has a written confirmation paragraph in `docs/architecture/security-review.md`.
- `just check` runs in strict mode (Phase 16's gate upgrade) and passes.
- The end-to-end demo runs cleanly against a fresh Caddy 2.8 instance.

A failing slice in Phase 16 is a Tier 1 ship-blocker. Tier 2 work (Phase 17 onward) MUST NOT begin until Phase 16 is green.

## Trait surfaces exercised by this phase

Every trait in `trait-signatures.md` is exercised by at least one Phase 16 slice. Mapping:

- §1 `core::storage::Storage` — slice 16.2 (SQLite locked, corruption).
- §2 `core::caddy::CaddyClient` — slice 16.1 (Caddy unreachable).
- §3 `core::secrets::SecretsVault` — slice 16.4 (master-key access denied) and slice 16.7 (secrets-leak simulation).
- §4 `core::policy::PresetRegistry` — exercised indirectly through the demo script in slice 16.8.
- §5 `core::diff::DiffEngine` — exercised by the secrets-leak simulation in slice 16.7.
- §6 `core::reconciler::Applier` — slice 16.1 (apply mid-flight) and slice 16.5 (mutation apply percentiles).
- §7 `core::tool_gateway::ToolGateway` — out of scope for Tier 1; first exercised in Phase 19.
- §8 `core::probe::ProbeAdapter` — slice 16.5 (drift-tick performance) exercises through the drift detector's reachability cache.
- §9 `core::config::EnvProvider` — exercised by every test through configuration loading.
- §10 `core::http::HttpServer` — exercised by every integration test through real HTTP requests.
- §11 `core::docker::DockerWatcher` — slice 16.3 (Docker socket gone).

## Slice plan summary

| # | Slice title | Primary files | Effort (h) | Depends on |
|---|---|---|---|---|
| 16.1 | Failure-mode batch A: Caddy unreachable (startup, mid-flight) | `core/crates/cli/tests/failure_caddy_unreachable.rs` | 6 | Phases 1–15 |
| 16.2 | Failure-mode batch B: SQLite locked, SQLite corruption | `core/crates/cli/tests/failure_sqlite_locked.rs`, `core/crates/cli/tests/failure_sqlite_corruption.rs` | 6 | Phases 1–15 |
| 16.3 | Failure-mode batch C: Docker socket gone, capability probe failure | `core/crates/cli/tests/failure_docker_socket_gone.rs`, `core/crates/cli/tests/failure_capability_probe.rs` | 5 | Phases 1–15 |
| 16.4 | Failure-mode batch D: bootstrap credentials unwritable, master-key access denied | `core/crates/cli/tests/failure_bootstrap_credentials.rs`, `core/crates/cli/tests/failure_master_key_access.rs` | 6 | Phases 1–15 |
| 16.5 | Performance verification: cold start, mutation apply, drift tick, route list render, idle memory ceiling | `core/crates/cli/benches/performance_budget.rs`, `web/src/features/routes/RoutesIndex.perf.test.tsx` | 8 | 16.1–16.4 |
| 16.6 | Security review document covering H1–H17 | `docs/architecture/security-review.md` | 6 | 16.1–16.4 |
| 16.7 | Strict-mode `just check` upgrade | `justfile`, `.github/workflows/ci.yml` | 4 | 16.5 |
| 16.8 | End-to-end demo script | `docs/demos/tier-1.md`, `core/crates/cli/tests/demo_e2e.rs` | 6 | 16.5 |
| 16.9 | Documentation pass: doc-comments, header comments, user README | `core/crates/**/*.rs`, `web/src/**/*.tsx`, `docs/README.md` | 8 | 16.5 |

After every slice: `cargo build --workspace` succeeds; `pnpm typecheck` succeeds where the slice touches the web; the slice's named tests pass.

---

## Slice 16.1 [cross-cutting] — Failure-mode batch A: Caddy unreachable (startup and mid-flight)

### Goal

Two integration tests covering the Caddy-unreachable failure rows from architecture §10. With Caddy unreachable at daemon startup, Trilithon MUST retry on the documented exponential back-off (5 attempts, capped at 16 seconds total per architecture §8.1), surface a banner, and attempt no apply. With Caddy unreachable mid-flight (during an in-progress apply), the in-flight call MUST return `ApplyError::Unreachable`, the desired-state pointer MUST remain untouched, and one audit row with `kind = config.apply-failed, error_kind = caddy_unreachable` MUST be written.

### Entry conditions

- Phases 1–15 complete.
- The integration test harness can launch Trilithon against a configurable Caddy admin endpoint (real Caddy or a controllable stub).

### Files to create or modify

- `core/crates/cli/tests/failure_caddy_unreachable.rs`.
- `core/crates/adapters/tests/helpers/caddy_stub.rs` — a small `axum`-backed stub that can be made unreachable mid-test.

### Signatures and shapes

```rust
// core/crates/adapters/tests/helpers/caddy_stub.rs
pub struct CaddyStub {
    pub addr: SocketAddr,
    handle: tokio::task::JoinHandle<()>,
    shutdown: tokio::sync::watch::Sender<bool>,
    pub call_log: Arc<Mutex<Vec<StubCall>>>,
}

impl CaddyStub {
    pub async fn start() -> Self;
    pub async fn drop_listener(&self);   // simulate Caddy disappearing
    pub async fn shutdown(self);
}
```

### Algorithm — `caddy_unreachable_at_startup_retries_with_back_off_and_writes_no_apply`

1. Configure Trilithon to point at a closed loopback port (no `CaddyStub` running).
2. Spawn the daemon. Capture stderr / tracing output.
3. Within 20 seconds, assert:
   1. Five connection attempts were made (exponential back-off times: ~0, 1, 2, 4, 8 seconds — actual sums to under 16 seconds).
   2. A banner string `"Caddy is unreachable"` appears in the structured tracing output.
   3. No `config.applied` audit row is written.
   4. The daemon does not panic; it remains in the "Caddy unreachable" state.

### Algorithm — `caddy_unreachable_mid_flight_preserves_desired_state_pointer`

1. Start the daemon against a `CaddyStub` running normally.
2. Submit a `CreateRoute` mutation. Observe `mutation.submitted` audit row.
3. Inject a delay into the stub's `POST /load` handler (1 second). During the delay, call `CaddyStub::drop_listener` to simulate Caddy disappearing.
4. Wait for the mutation to complete with `ApplyError::Unreachable`.
5. Assert:
   1. The current `config_version` in storage is unchanged (still equal to the pre-mutation value).
   2. Exactly one audit row with `kind = config.apply-failed` and `error_kind = caddy_unreachable` was written.
   3. No `config.applied` row was written for this correlation id.

### Tests

Both tests live in `core/crates/cli/tests/failure_caddy_unreachable.rs`:

- `caddy_unreachable_at_startup_retries_with_back_off_and_writes_no_apply`.
- `caddy_unreachable_mid_flight_preserves_desired_state_pointer`.

### Acceptance command

`cargo test -p trilithon-cli --test failure_caddy_unreachable`

### Exit conditions

- Both tests pass.
- The 16-second back-off cap is observable.
- The mid-flight test asserts the typed error AND the unchanged pointer AND the single audit row.

### Audit kinds emitted

Asserted (not introduced):

- `config.apply-failed` (architecture §6.6).

### Tracing events emitted

Asserted:

- `caddy.disconnected` (architecture §12.1).
- `apply.failed`.

### Cross-references

- PRD T1.1.
- Architecture §8.1 (Caddy admin error handling), §10 (failure model), §12.1.

---

## Slice 16.2 [cross-cutting] — Failure-mode batch B: SQLite locked, SQLite corruption

### Goal

Two integration tests. With SQLite locked beyond the busy timeout, a mutation MUST return a typed retryable error and the user MUST see an actionable message. With simulated SQLite corruption (a `PRAGMA integrity_check` returning a non-`ok` result), the daemon MUST emit `storage.integrity-check.failed`, surface a banner, and document a recovery path.

### Entry conditions

- Phases 1–15 complete.
- The Phase 2 SQLite adapter exposes a configurable busy timeout (default 5 seconds) and a periodic integrity check.

### Files to create or modify

- `core/crates/cli/tests/failure_sqlite_locked.rs`.
- `core/crates/cli/tests/failure_sqlite_corruption.rs`.

### Signatures and shapes

No new public types. The tests use the existing `Storage` trait and inject contention via a second SQLite connection holding a `BEGIN IMMEDIATE` transaction.

### Algorithm — `sqlite_locked_beyond_busy_timeout_returns_typed_retryable_error`

1. Open the Trilithon SQLite database in two connections.
2. On connection B, issue `BEGIN IMMEDIATE` and hold the transaction.
3. On connection A (Trilithon's), submit a `CreateRoute` mutation.
4. Wait 6 seconds (busy timeout + 1 second slack).
5. Assert the mutation queue surfaces `StorageError::SqliteBusy { retries: 3 }` per trait-signatures.md §1.
6. Assert the API response carries an actionable message (the response body contains the substring `"database is busy; retry the operation"` or equivalent).
7. Release connection B's transaction. Assert a subsequent retry succeeds.

### Algorithm — `sqlite_corruption_emits_critical_event_and_surfaces_recovery_banner`

1. Start the daemon.
2. Inject an integrity-check failure by overriding the periodic check function to return `Err("not ok: page 1 corrupted")`.
3. Within 5 seconds, assert:
   1. A tracing event `storage.integrity-check.failed` is emitted (architecture §12.1).
   2. A startup audit row with `kind = system, outcome = error, error_kind = sqlite_corrupt` is present (architecture §10).
   3. The daemon enters maintenance mode: every mutation endpoint returns `503` with a body referencing the recovery path documented in architecture §10.
4. Assert the documented recovery path (restore from last backup; audit log preserved) is referenced in the response body.

### Tests

- `sqlite_locked_beyond_busy_timeout_returns_typed_retryable_error`.
- `sqlite_corruption_emits_critical_event_and_surfaces_recovery_banner`.

### Acceptance command

`cargo test -p trilithon-cli --test failure_sqlite_locked --test failure_sqlite_corruption`

### Exit conditions

- Both tests pass.
- The corruption path emits exactly one audit row at startup; mutation rows are not written while in maintenance mode.

### Audit kinds emitted

Asserted:

- `system` family with `error_kind = sqlite_corrupt` (one startup row).

### Tracing events emitted

Asserted:

- `storage.integrity-check.failed`.

### Cross-references

- PRD T1.6.
- Hazards H14.
- Architecture §10, §12.1, §14.

---

## Slice 16.3 [cross-cutting] — Failure-mode batch C: Docker socket gone, capability probe failure

### Goal

Two integration tests. With no Docker socket available, the daemon MUST emit "no Docker, no proposals" and proceed without panicking; this is the Tier 2 substrate verifier (no Tier 2 functionality is exercised, only the negative-space behaviour). With a failing capability probe, modules MUST be listed as "unknown" and mutations referencing them MUST fail validation with a clear message (hazard H5).

### Entry conditions

- Phases 1–15 complete.

### Files to create or modify

- `core/crates/cli/tests/failure_docker_socket_gone.rs`.
- `core/crates/cli/tests/failure_capability_probe.rs`.

### Signatures and shapes

No new public types.

### Algorithm — `daemon_runs_without_docker_socket_and_emits_no_proposals_banner`

1. Start the daemon with the Docker watcher enabled but pointed at a non-existent socket path.
2. Within 5 seconds, assert:
   1. No panic occurs.
   2. The daemon's structured log contains the message `"Docker discovery unavailable"` (architecture §10).
   3. No proposal rows are written.
3. Submit a `CreateRoute` mutation through the HTTP API; assert it succeeds (Docker absence does not block mutations).

### Algorithm — `capability_probe_failure_marks_modules_unknown_and_validates_against_unknown_module`

1. Start the daemon against a Caddy stub that responds `500 Internal Server Error` to `GET /config/apps`.
2. Within the configured probe timeout, assert:
   1. The probe records `state = "failed"` in `capability_probe_results`.
   2. The structured log contains `"Capability probe failed; using cached set"`.
3. Submit a mutation referencing a module not present in any prior cached capability set (use a fresh database with no cached probe).
4. Assert the validator returns a clear error message identifying the module by name and the probe failure as the reason.

### Tests

- `daemon_runs_without_docker_socket_and_emits_no_proposals_banner`.
- `capability_probe_failure_marks_modules_unknown_and_validates_against_unknown_module`.

### Acceptance command

`cargo test -p trilithon-cli --test failure_docker_socket_gone --test failure_capability_probe`

### Exit conditions

- Both tests pass.
- No panic occurs in either branch.

### Audit kinds emitted

Asserted:

- `caddy.capability-probe-completed` (with outcome reflecting failure).

### Tracing events emitted

Asserted:

- `caddy.capability-probe.completed`.

### Cross-references

- PRD T1.11, T2.1 substrate.
- Hazards H5.
- Architecture §10.

---

## Slice 16.4 [cross-cutting] — Failure-mode batch D: bootstrap credentials unwritable, master-key access denied

### Goal

Two integration tests. An unwritable bootstrap credentials path MUST cause the daemon to exit with code `3` and a structured error (hazard H13). When the OS keychain is locked / inaccessible, the file fallback MUST engage and an audit row MUST record the choice (T1.15).

### Entry conditions

- Phases 1–15 complete.
- The Phase 1 daemon's exit codes are documented and `3` is reserved for bootstrap-credential failure.
- The Phase 10 secrets vault implements both keychain and file-fallback master-key paths.

### Files to create or modify

- `core/crates/cli/tests/failure_bootstrap_credentials.rs`.
- `core/crates/cli/tests/failure_master_key_access.rs`.

### Signatures and shapes

No new public types.

### Algorithm — `unwritable_bootstrap_path_exits_with_code_3`

1. Create a temporary directory and `chmod` it to `0500` (read+execute only).
2. Configure the daemon to write its bootstrap credentials file at `<tmp>/credentials`.
3. Spawn the daemon. Wait for exit.
4. Assert the exit code is exactly `3`.
5. Assert stderr contains a structured error referencing the unwritable path; the structured error has the shape `{ kind: "bootstrap_credentials_unwritable", path: "<tmp>/credentials", detail: <io_error> }`.
6. Assert no plaintext credentials appear in stderr or in any log file.

### Algorithm — `keychain_locked_engages_file_fallback_and_writes_audit_row`

1. Configure the secrets vault with a mock keychain that returns `KeyringUnavailable` on every call.
2. Start the daemon. Wait for the secrets vault's lazy initialisation to fire (submit a mutation that touches a secret).
3. Assert:
   1. The vault transitions to file-fallback mode.
   2. The master-key file at the configured fallback path exists with permission `0600`.
   3. Exactly one audit row is written with `kind = system, outcome = ok, notes = "master-key fallback engaged"`.
4. Assert subsequent secret operations succeed using the file fallback.

### Tests

- `unwritable_bootstrap_path_exits_with_code_3`.
- `keychain_locked_engages_file_fallback_and_writes_audit_row`.

### Acceptance command

`cargo test -p trilithon-cli --test failure_bootstrap_credentials --test failure_master_key_access`

### Exit conditions

- Both tests pass.
- The bootstrap path test runs in a clean temporary directory and never touches the developer's keychain.

### Audit kinds emitted

Asserted:

- A `system` audit row referencing the master-key fallback. Architecture §6.6 lists `secrets.master-key-rotated` for explicit rotation; the file-fallback row uses the `system` family with structured `notes`. If a future audit kind such as `secrets.master-key-fallback-engaged` is desired, it MUST be added to architecture §6.6 in the same commit.

### Tracing events emitted

Asserted:

- A warning-level event with `error.kind = "KeyringUnavailable"`.

### Cross-references

- PRD T1.14, T1.15.
- Hazards H13.
- Architecture §11 (security posture), §10.

---

## Slice 16.5 [cross-cutting] — Performance verification

### Goal

Five performance assertions, gathered into one bench harness in `cli` and one Vitest performance test in `web`. Each target comes from architecture §13 and the phase reference:

1. Cold start to ready under 5 seconds with 1,000 routes loaded.
2. Single mutation apply under 1 second median, under 5 seconds at the 99th percentile.
3. Drift-check tick under 2 seconds.
4. Route list render under 500 milliseconds with 1,000 routes loaded.
5. Idle resident memory under 200 MiB with 1,000 routes loaded.

### Entry conditions

- Slices 16.1–16.4 complete.

### Files to create or modify

- `core/crates/cli/benches/performance_budget.rs` — bench harness using the `criterion` crate.
- `core/crates/cli/tests/performance_budget_assertions.rs` — wall-clock assertions runnable in CI.
- `web/src/features/routes/RoutesIndex.perf.test.tsx` — Vitest performance harness.

### Signatures and shapes

```rust
// core/crates/cli/tests/performance_budget_assertions.rs
#[tokio::test]
async fn cold_start_to_ready_under_5_seconds_with_1000_routes() { /* see Algorithm */ }

#[tokio::test]
async fn single_mutation_apply_p50_under_1_s_p99_under_5_s() { /* see Algorithm */ }

#[tokio::test]
async fn drift_check_tick_under_2_seconds_with_1000_routes() { /* see Algorithm */ }

#[tokio::test]
async fn idle_memory_under_200_mib_with_1000_routes() { /* see Algorithm */ }
```

```ts
// web/src/features/routes/RoutesIndex.perf.test.tsx
import { test, expect } from 'vitest';
test('route_list_renders_1000_routes_under_500_ms', async () => { /* see Algorithm */ });
```

### Algorithm — cold start

1. Seed a fresh SQLite database with 1,000 routes via direct `Storage` calls.
2. Record `started = Instant::now()`.
3. Spawn the daemon. Wait for the structured log line `"daemon ready"` (architecture event `daemon.started`).
4. Record `ready = Instant::now()`.
5. Assert `ready - started < Duration::from_secs(5)`.

### Algorithm — mutation apply percentiles

1. Seed 1,000 routes. Start the daemon against a real Caddy.
2. Submit 200 `UpdateRoute` mutations sequentially through the HTTP API. Record each end-to-end latency from request send to `200` response.
3. Compute median and 99th percentile.
4. Assert median < 1,000 ms and 99th percentile < 5,000 ms.

### Algorithm — drift check tick

1. Seed 1,000 routes. Start the daemon. Wait for the first drift-check tick.
2. Measure the duration of one tick via the `drift.detected` / "no drift" trace span.
3. Assert duration < 2,000 ms.

### Algorithm — idle memory

1. Seed 1,000 routes. Start the daemon.
2. Idle for 30 seconds.
3. Sample resident memory via `getrusage`.
4. Assert RSS < 200 MiB.

### Algorithm — route list render

1. Mount `<RoutesIndex routes={fixture1000}>` inside Vitest's React testing environment.
2. Use `performance.now()` before and after `act(() => render(...))`.
3. Assert duration < 500 ms.

### Tests

Five tests as listed above.

### Acceptance command

`cargo test -p trilithon-cli --test performance_budget_assertions && pnpm vitest run web/src/features/routes/RoutesIndex.perf.test.tsx`

### Exit conditions

- All five tests pass on the reference hardware (a four-core consumer machine).
- A failure files an open question against architecture §13 rather than silently relaxing the target (per architecture §13).

### Audit kinds emitted

None new.

### Tracing events emitted

Asserted:

- `daemon.started`.
- `drift.detected` (or its absence on a no-drift tick).

### Cross-references

- Architecture §13 (performance budget).
- PRD T1.1, T1.4, T1.8, T1.13.

---

## Slice 16.6 [cross-cutting] — Security review document covering H1–H17

### Goal

Author `docs/architecture/security-review.md` containing one paragraph per hazard from H1 through H17, each confirming the implementation against the hazard or filing an open question. The document is reviewed against ADR-0001 through ADR-0014.

### Entry conditions

- Slices 16.1–16.4 complete.

### Files to create or modify

- `docs/architecture/security-review.md` (new file).

### Signatures and shapes

The document follows this structure:

```
# Trilithon Tier 1 Security Review

## H1 — Caddy admin endpoint exposure
<one paragraph confirming the implementation against H1>

## H2 — Stale-upstream rollback
<paragraph>

... through H17 ...

## Open questions
<list>
```

### Algorithm

For each hazard H1 through H17:

1. Quote the hazard description from the meta-prompt §7.
2. Identify the implementing slice or phase (architecture §11 already maps every control to one or more hazards).
3. Write one paragraph stating the control, where it lives in the codebase, and which integration test exercises it.
4. If a hazard has no implementation, file an open question rather than silently passing the review.

### Tests

A linter test confirms every hazard identifier appears in the document:

`docs/architecture/security-review.md` test in `core/crates/cli/tests/security_review_completeness.rs`:

- `security_review_references_every_hazard_h1_through_h17` — read the document, assert that for each `n` in `1..=17`, the substring `H{n} —` appears exactly once in a heading.

### Acceptance command

`cargo test -p trilithon-cli --test security_review_completeness`

### Exit conditions

- The document exists and references every hazard.
- The completeness test passes.
- ADR-0001 through ADR-0014 are reviewed end-to-end (a section in the document references each by number).

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- Hazards H1–H17.
- ADR-0001 through ADR-0014.

---

## Slice 16.7 [cross-cutting] — Strict-mode `just check` upgrade

### Goal

Upgrade `just check` to run, in addition to the existing gate, four additional suites:

1. Property tests for the mutation algebra (`cargo test -p trilithon-core mutation::proptest`).
2. The round-trip Caddyfile corpus (`cargo test -p trilithon-adapters --test caddyfile_round_trip`).
3. The failure-mode tests from slices 16.1–16.4.
4. The secrets-vault leak simulation (a test confirming no plaintext secret appears in any audit row diff).

### Entry conditions

- Slices 16.1–16.4 complete.
- The Phase 13 round-trip harness is green.

### Files to create or modify

- `justfile` — add `check-strict` recipe and chain it into `check`.
- `.github/workflows/ci.yml` — invoke `just check` (no change if already invoked).

### Signatures and shapes

```just
# justfile (excerpt)
check: check-rust check-typescript check-strict

check-strict: check-strict-mutation-proptests check-strict-caddyfile-round-trip check-strict-failure-modes check-strict-secrets-leak

check-strict-mutation-proptests:
    cargo test -p trilithon-core mutation::proptest

check-strict-caddyfile-round-trip:
    cargo test -p trilithon-adapters --test caddyfile_round_trip

check-strict-failure-modes:
    cargo test -p trilithon-cli --test failure_caddy_unreachable --test failure_sqlite_locked --test failure_sqlite_corruption --test failure_docker_socket_gone --test failure_capability_probe --test failure_bootstrap_credentials --test failure_master_key_access

check-strict-secrets-leak:
    cargo test -p trilithon-core secrets::leak_simulation
```

The secrets-leak simulation is a new test added in this slice if not already present from Phase 10. It walks every audit row written during a representative test scenario and asserts no value matches a known secret pattern.

### Algorithm

`just check` runs each subcommand sequentially. CI invokes `just check`; failures in any sub-suite fail the build.

### Tests

The slice itself has no new test functions; the test is the gate. Acceptance is `just check` running every sub-suite to completion in CI.

### Acceptance command

`just check`

### Exit conditions

- `just check` runs all four strict-mode suites and they pass.
- CI enforces `just check`.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- All Tier 1 PRD T-numbers.

---

## Slice 16.8 [cross-cutting] — End-to-end demo script

### Goal

Author `docs/demos/tier-1.md` covering: fresh install, bootstrap, first route, second route via Caddyfile import, drift detection (induced by manual `curl` to Caddy admin), adopt running state, rollback to the first snapshot, and secrets reveal under step-up. A CI job exercises the script against a fresh Caddy 2.8 (the minimum supported version per the Phase 13 pre-flight checklist).

### Entry conditions

- Slices 16.1–16.7 complete.

### Files to create or modify

- `docs/demos/tier-1.md` — narrative walkthrough with copyable `curl` commands.
- `core/crates/cli/tests/demo_e2e.rs` — programmatic equivalent of the script.
- `.github/workflows/demo.yml` — CI job pinned to Caddy 2.8.

### Signatures and shapes

The integration test mirrors the demo step-by-step:

```rust
// core/crates/cli/tests/demo_e2e.rs
#[tokio::test]
async fn tier_1_demo_runs_cleanly_against_caddy_2_8() {
    // 1. Fresh install: spawn daemon with empty database.
    // 2. Bootstrap: read credentials file, log in, change password.
    // 3. Create first route via POST /api/v1/mutations.
    // 4. Import Caddyfile via POST /api/v1/imports/caddyfile/apply.
    // 5. Induce drift: curl Caddy admin directly with an unauthorised PATCH.
    // 6. Observe drift event; adopt running state.
    // 7. Rollback to snapshot 1; assert the first route is restored.
    // 8. Reveal a secret under step-up; assert the secrets.revealed audit row.
}
```

### Algorithm

1. Start a Caddy 2.8 container.
2. Spawn the daemon pointed at the container's admin socket.
3. Drive the eight scripted steps through HTTP API calls.
4. After each step, assert one or more invariants such as audit rows present, snapshot id advanced, and drift detected.
5. Report total wall-clock duration; the demo MUST complete in under 60 seconds against the reference hardware.

### Tests

- `tier_1_demo_runs_cleanly_against_caddy_2_8`.

### Acceptance command

`cargo test -p trilithon-cli --test demo_e2e`

### Exit conditions

- The script lives at `docs/demos/tier-1.md`.
- The CI job runs it cleanly against a fresh Caddy 2.8.
- The integration test asserts each scripted invariant.

### Audit kinds emitted

Asserted in this slice (already introduced by prior phases):

- `auth.bootstrap-credentials-rotated`.
- `mutation.submitted`, `config.applied`.
- `import.caddyfile`.
- `config.drift-detected`, `config.drift-resolved`.
- `config.rolled-back`.
- `secrets.revealed`.

### Tracing events emitted

Asserted:

- `daemon.started`, `apply.succeeded`, `drift.detected`, `drift.resolved`.

### Cross-references

- PRD T1.1 through T1.15.

---

## Slice 16.9 [cross-cutting] — Documentation pass

### Goal

Three documentation gates:

1. Every public Rust item has a doc comment. `cargo doc --no-deps -D rustdoc::missing_docs` passes.
2. Every web component file under `web/src/` has a header comment. A custom ESLint rule asserts presence.
3. `docs/README.md` is the user-facing README and covers installation, first-run, and recovery, referencing the demo script from slice 16.8.

### Entry conditions

- Slice 16.8 complete.

### Files to create or modify

- Doc comments throughout `core/crates/**/*.rs` (additive only, no semantic changes).
- Header comments throughout `web/src/**/*.tsx` and `web/src/**/*.ts` (additive only).
- `web/eslint-rules/header-comment.js` — local ESLint rule asserting the presence of a leading comment on every component file.
- `web/eslint.config.js` — register the local rule.
- `docs/README.md` — user-facing README.

### Signatures and shapes

```js
// web/eslint-rules/header-comment.js
module.exports = {
  meta: { type: 'problem', schema: [], messages: { missing: 'File MUST start with a header comment.' } },
  create(context) {
    return {
      Program(node) {
        const first = node.body[0];
        const sourceCode = context.getSourceCode();
        const leading = sourceCode.getCommentsBefore(first ?? node);
        if (!leading || leading.length === 0) {
          context.report({ node, messageId: 'missing' });
        }
      },
    };
  },
};
```

### Algorithm

1. Run `cargo doc --no-deps -D rustdoc::missing_docs` and add doc comments to every flagged item until the build passes.
2. For every file under `web/src/` matching `**/*.{ts,tsx}`, add a header comment (file purpose, one paragraph). The local ESLint rule enforces presence going forward.
3. Author `docs/README.md`. Sections: Installation, First-run, Updating, Recovery (point at the demo script and at architecture §10).

### Tests

- `cargo doc --no-deps -D rustdoc::missing_docs`.
- `pnpm lint` (now including the header-comment rule).
- `cargo test -p trilithon-cli --test docs_readme_present` — a small test asserting `docs/README.md` exists and references `docs/demos/tier-1.md`.

### Acceptance command

`cargo doc --no-deps -D rustdoc::missing_docs && pnpm lint && cargo test -p trilithon-cli --test docs_readme_present`

### Exit conditions

- The doc-coverage build passes.
- The ESLint rule is active and passes on every file.
- `docs/README.md` covers installation, first-run, and recovery.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- All Tier 1 PRD T-numbers (the README references the full set).

---

## Phase 16 exit checklist

- [ ] Every slice from 16.1 through 16.9 has shipped and its acceptance command passes.
- [ ] `just check` (now in strict mode per slice 16.7) passes locally and in continuous integration.
- [ ] Every failure-mode test passes.
- [ ] Every performance budget is met OR documented as a known regression with an open issue (architecture §13 rule).
- [ ] Every hazard from H1 through H17 has a written confirmation paragraph in `docs/architecture/security-review.md`.
- [ ] The end-to-end demo script runs cleanly in CI against a fresh Caddy 2.8 instance.
- [ ] Every public Rust item has a doc comment; every web component file has a header comment.
- [ ] `docs/README.md` is the user-facing README.

## Open questions

1. The performance reference hardware (architecture §13: "a four-core consumer machine") is not yet pinned to a specific CI runner SKU. Whether GitHub Actions' standard `ubuntu-latest` runners are stable enough across measurement runs to enforce the 500-millisecond and 5-second targets without flakiness is unresolved and should be measured during slice 16.5 implementation.
2. Slice 16.4's `secrets.master-key-fallback-engaged` audit kind is not currently in architecture §6.6. Whether to add a dedicated kind or to reuse the `system` family with structured `notes` is unresolved; the slice currently uses the `system` family.
3. The minimum-supported Caddy version is 2.8 per the Phase 13 pre-flight checklist; the demo script test (slice 16.8) targets exactly 2.8 to verify compatibility, while the round-trip harness pins 2.11.2 (the version the corpus golden files were generated against). Whether to run the demo against both versions in CI is filed for V1.1 planning.
