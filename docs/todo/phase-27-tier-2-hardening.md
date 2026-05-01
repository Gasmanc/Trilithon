# Phase 27 — Tier 2 Hardening and V1 Release Readiness — Implementation Slices

> Phase reference: [../phases/phase-27-tier-2-hardening.md](../phases/phase-27-tier-2-hardening.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference (`docs/phases/phase-27-tier-2-hardening.md`).
- Architecture §6.6 (audit log kinds), §11 (security posture), §12.1 (tracing vocabulary), §13 (performance budget), §14 (upgrade and migration).
- Trait signatures: every trait surface from `docs/architecture/trait-signatures.md` is exercised by at least one Tier 2 flow test.
- ADRs: ADR-0001 through ADR-0016 (full Tier 1 + Tier 2 set reviewed end-to-end).
- PRD: T1.1–T1.15 (regression guard) and T2.1–T2.12 (Tier 2 acceptance).
- Hazards: H1, H4, H7, H9, H10, H11, H13, H16 (re-confirmed against the Tier 2 surface; H11 and H16 receive dedicated re-review).

## Slice plan summary

| # | Title | Primary files | Effort (ideal-eng-hours) | Depends on |
|---|-------|---------------|--------------------------|------------|
| 27.1 | T2.10 conflict + rebase end-to-end flow test | `core/crates/adapters/tests/e2e_concurrent_mutation.rs`, `.github/workflows/e2e-flows.yml` | 5 | Phase 17 |
| 27.2 | T2.2 policy preset capability degradation flow | `core/crates/adapters/tests/e2e_policy_preset_degradation.rs` | 4 | Phase 18 |
| 27.3 | T2.3 + T2.4 explain-then-propose end-to-end flow | `core/crates/adapters/tests/e2e_explain_then_propose.rs` | 6 | Phase 19, 20 |
| 27.4 | T2.1 + T2.11 Docker discovery wildcard-callout flow | `core/crates/adapters/tests/e2e_docker_discovery.rs` | 5 | Phase 21 |
| 27.5 | T2.5 + T2.6 access log viewer 10-million-line flow | `core/crates/adapters/tests/e2e_access_log_10m.rs` | 5 | Phase 22 |
| 27.6 | T2.9 + T2.12 native bundle round-trip flow | `core/crates/adapters/tests/e2e_bundle_round_trip.rs` | 4 | Phase 25, 26 |
| 27.7 | Performance verification at 5,000 routes (four budgets) | `core/crates/adapters/benches/perf_5000_routes.rs`, `.github/workflows/perf.yml` | 6 | Phase 16 |
| 27.8 | Install/upgrade matrix (Compose fresh, Compose upgrade, systemd Ubuntu, systemd Debian, Tier 1→2 schema upgrade) | `.github/workflows/install-upgrade-matrix.yml`, `docs/release/v1-matrix.md` | 6 | Phase 23, 24, 26 |
| 27.9 | Security review document and H11/H16 dedicated re-review | `docs/architecture/security-review.md` | 4 | All Tier 2 phases |
| 27.10 | V1 release readiness: release notes, doc audit, Tier 1 regression guard | `docs/release/v1-release-notes.md`, `.github/workflows/tier-1-regression.yml` | 5 | All prior slices |

---

## Slice 27.1 — T2.10 conflict + rebase end-to-end flow test

### Goal

A scripted CI flow exercises two concurrent mutations against the same `config_version`, observes the second receive a `mutation.conflicted` audit row, runs the rebase planner, and asserts the rebased mutation reaches `mutation.applied`.

### Entry conditions

- Phase 17 (concurrency control) complete.

### Files to create or modify

- `core/crates/adapters/tests/e2e_concurrent_mutation.rs` — the flow test.
- `.github/workflows/e2e-flows.yml` — CI workflow registering this and subsequent flow tests.

### Signatures and shapes

```rust
//! End-to-end flow: T2.10 concurrent mutation, conflict, rebase,
//! apply.
//!
//! Asserts: audit kinds `mutation.submitted`, `mutation.conflicted`,
//! `mutation.rebased.auto` (or `.manual`), `mutation.applied`,
//! `config.applied` (architecture §6.6) appear in order.

use trilithon_adapters::test_support::TestHarness;

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_mutation_conflicts_then_rebases_then_applies() {
    let harness = TestHarness::new().await;

    // Two actors read config_version = N.
    let cv = harness.current_config_version().await;
    let a = harness.submit_mutation_at(cv, "actor-a", make_create_route("a.invalid")).await;
    let b = harness.submit_mutation_at(cv, "actor-b", make_create_route("b.invalid")).await;

    // First wins; second conflicts.
    let a_result = harness.await_terminal(a).await;
    let b_result = harness.await_terminal(b).await;
    assert_eq!(a_result.kind, trilithon_core::audit::AuditEvent::MutationApplied { /* ... */ }.kind_str());
    assert_eq!(b_result.kind, "mutation.conflicted");

    // Rebase b onto the new config_version.
    let rebased = harness.rebase(b).await;
    let rebased_result = harness.await_terminal(rebased).await;
    let kind = &rebased_result.kind;
    assert!(kind == "mutation.applied" || kind == "mutation.rebased.auto" || kind == "mutation.rebased.manual",
        "unexpected terminal kind: {kind}");
}

fn make_create_route(_host: &str) -> trilithon_core::Mutation { todo!() }
```

`.github/workflows/e2e-flows.yml` (verbatim):

```yaml
name: e2e-flows

on:
  push:
    branches: [main]
  pull_request:
  workflow_dispatch:

jobs:
  flows:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Run end-to-end flow tests
        run: |
          cd core
          cargo test --test e2e_concurrent_mutation \
                     --test e2e_policy_preset_degradation \
                     --test e2e_explain_then_propose \
                     --test e2e_docker_discovery \
                     --test e2e_access_log_10m \
                     --test e2e_bundle_round_trip
```

### Algorithm

1. Boot `TestHarness` (an in-process daemon plus mock Caddy admin endpoint).
2. Read `config_version = N`.
3. Submit two mutations from distinct actors at version `N`.
4. Wait for both to reach a terminal state. Assert one is `applied`, the other `conflicted`.
5. Trigger the rebase planner on the conflicted mutation.
6. Assert the rebased mutation reaches `applied` (or one of the rebase audit kinds).

### Tests

- The named test above.
- A negative-path variant where the rebase planner cannot auto-merge and `mutation.rebased.manual` is required.

### Acceptance command

```
cargo test -p trilithon-adapters --test e2e_concurrent_mutation
```

### Exit conditions

- The flow test passes in CI.
- The audit kinds appear in the expected order.

### Audit kinds emitted

- `mutation.submitted`, `mutation.conflicted`, `mutation.rebased.auto`, `mutation.rebased.manual`, `mutation.applied`, `config.applied` (architecture §6.6).

### Tracing events emitted

- `apply.started`, `apply.succeeded`, `apply.failed` (architecture §12.1).

### Cross-references

- ADR-0012 (optimistic concurrency on monotonic config_version).
- PRD T2.10.

---

## Slice 27.2 — T2.2 policy preset capability degradation flow

### Goal

A scripted flow attaches `public-admin@1` to a route on stock Caddy (rate-limit module absent) and asserts the route applies with the rate-limit slot omitted and a warning surfaced; on enhanced Caddy it asserts the rate limit applies.

### Entry conditions

- Phase 18 complete.

### Files to create or modify

- `core/crates/adapters/tests/e2e_policy_preset_degradation.rs`.

### Signatures and shapes

```rust
#[tokio::test(flavor = "multi_thread")]
async fn public_admin_preset_degrades_on_stock_caddy() {
    let harness = TestHarness::new_with_caddy_modules(&[/* HSTS only, no ratelimit */]).await;
    let route = harness.create_route("admin.invalid").await;
    harness.attach_preset(&route, "public-admin", 1).await;
    let applied = harness.await_apply(&route).await;
    assert!(applied.warnings.iter().any(|w|
        w.contains("rate-limit unavailable on this Caddy build")));
    let kind = harness.last_audit_kind_for(&route).await;
    assert_eq!(kind, "policy-preset.attached");
}

#[tokio::test(flavor = "multi_thread")]
async fn public_admin_preset_applies_on_enhanced_caddy() {
    let harness = TestHarness::new_with_caddy_modules(&["http.handlers.rate_limit"]).await;
    let route = harness.create_route("admin.invalid").await;
    harness.attach_preset(&route, "public-admin", 1).await;
    let applied = harness.await_apply(&route).await;
    assert!(applied.warnings.is_empty());
}
```

### Algorithm

1. Boot harness with a configurable Caddy module set.
2. Create a route, attach `public-admin@1`.
3. Await the apply outcome.
4. On stock Caddy assert a warning lists the absent rate-limit module.
5. On enhanced Caddy assert no warnings.

### Tests

- The two named tests above.

### Acceptance command

```
cargo test -p trilithon-adapters --test e2e_policy_preset_degradation
```

### Exit conditions

- Both flows pass in CI.

### Audit kinds emitted

- `policy-preset.attached`, `caddy.capability-probe-completed` (architecture §6.6).

### Tracing events emitted

- `caddy.capability-probe.completed`, `apply.succeeded` (architecture §12.1).

### Cross-references

- ADR-0013, ADR-0016.
- PRD T2.2.

---

## Slice 27.3 — T2.3 + T2.4 explain-then-propose end-to-end flow

### Goal

A scripted flow exercises a language-model agent calling `explain` functions through the gateway, then proposing a route; the proposal is approved by a human; the route serves traffic.

### Entry conditions

- Phases 19 and 20 complete.

### Files to create or modify

- `core/crates/adapters/tests/e2e_explain_then_propose.rs`.

### Signatures and shapes

```rust
#[tokio::test(flavor = "multi_thread")]
async fn llm_explain_then_propose_then_human_approves_then_route_serves() {
    let harness = TestHarness::new().await;
    let token = harness.issue_tool_gateway_token(&[
        "explain.read_desired_state",
        "explain.read_audit_log",
        "propose.create_route",
    ]).await;

    let explain = harness.tool_gateway_call(&token, "explain.read_desired_state", json!({})).await;
    assert!(explain.is_ok());

    let proposal_id = harness.tool_gateway_call(&token, "propose.create_route", json!({
        "hostname": "llm-proposed.invalid", "upstream": "127.0.0.1:9999",
    })).await.unwrap().proposal_id;

    let approval = harness.approve_proposal_as_human(&proposal_id).await;
    assert_eq!(approval.audit_kind, "proposal.approved");
    let applied = harness.await_apply_for_proposal(&proposal_id).await;
    assert_eq!(applied.audit_kind, "config.applied");
}
```

### Algorithm

1. Boot harness, issue a tool-gateway token with explain + propose scopes.
2. Call `explain.read_desired_state` via the gateway.
3. Call `propose.create_route`; capture the proposal id.
4. Approve the proposal as a human actor.
5. Await `config.applied`.

### Tests

- The named test above.

### Acceptance command

```
cargo test -p trilithon-adapters --test e2e_explain_then_propose
```

### Exit conditions

- The flow passes in CI.

### Audit kinds emitted

- `tool-gateway.session-opened`, `tool-gateway.tool-invoked`, `mutation.proposed`, `proposal.approved`, `mutation.applied`, `config.applied`, `tool-gateway.session-closed` (architecture §6.6).

### Tracing events emitted

- `tool-gateway.invocation.started`, `tool-gateway.invocation.completed`, `proposal.received`, `proposal.approved`, `apply.started`, `apply.succeeded` (architecture §12.1).

### Cross-references

- ADR-0008.
- PRD T2.3, T2.4.

---

## Slice 27.4 — T2.1 + T2.11 Docker discovery wildcard-callout flow

### Goal

A scripted flow starts a labelled container, observes the proposal within 5 seconds, surfaces the wildcard-certificate banner appropriately, requires explicit acknowledgement, applies the route on approval.

### Entry conditions

- Phase 21 complete.

### Files to create or modify

- `core/crates/adapters/tests/e2e_docker_discovery.rs`.

### Signatures and shapes

```rust
#[tokio::test(flavor = "multi_thread")]
async fn labelled_container_produces_wildcard_callout_within_5_seconds() {
    let harness = TestHarness::new_with_docker().await;
    harness.import_wildcard_certificate("*.invalid").await;
    let started_at = std::time::Instant::now();
    harness.docker.start_container_with_labels(&[
        ("caddy", "host.invalid"),
        ("caddy.reverse_proxy", "127.0.0.1:9999"),
    ]).await;
    let proposal = harness.await_proposal_for_host("host.invalid").await;
    assert!(started_at.elapsed() < std::time::Duration::from_secs(5));
    assert!(proposal.wildcard_callout, "wildcard banner MUST be set");
    assert!(proposal.wildcard_ack_at.is_none(), "ack MUST be empty before approval");

    let approval = harness.approve_proposal_with_wildcard_ack(&proposal.id).await;
    assert!(approval.wildcard_ack_at.is_some());
    let applied = harness.await_apply(&proposal.id).await;
    assert_eq!(applied.audit_kind, "config.applied");
}
```

### Algorithm

1. Boot harness with the Docker watcher.
2. Import a wildcard certificate.
3. Start a labelled container.
4. Await the proposal; assert it appears within 5 seconds.
5. Assert `wildcard_callout = true`.
6. Approve with `wildcard_ack`; assert the ack is recorded; await apply.

### Tests

- The named test above.
- A variant: a non-wildcard match does not trigger the callout.

### Acceptance command

```
cargo test -p trilithon-adapters --test e2e_docker_discovery
```

### Exit conditions

- Both variants pass in CI.

### Audit kinds emitted

- `mutation.proposed`, `proposal.approved`, `config.applied` (architecture §6.6).

### Tracing events emitted

- `docker.event.received`, `proposal.received`, `proposal.approved`, `apply.succeeded` (architecture §12.1).

### Cross-references

- ADR-0007.
- PRD T2.1, T2.11.
- Hazards: H3 (wildcard-certificate over-match), H11.

---

## Slice 27.5 — T2.5 + T2.6 access log viewer 10-million-line flow

### Goal

A scripted flow ingests a synthetic 10-million-line corpus into the rolling access log store, asserts a representative filter completes in under 200 milliseconds, and traces a representative entry to its route.

### Entry conditions

- Phase 22 complete.

### Files to create or modify

- `core/crates/adapters/tests/e2e_access_log_10m.rs`.

### Signatures and shapes

```rust
#[tokio::test(flavor = "multi_thread")]
async fn ten_million_line_corpus_filters_under_200ms_and_traces_to_route() {
    let harness = TestHarness::new().await;
    harness.access_log.ingest_synthetic(10_000_000).await;

    let started = std::time::Instant::now();
    let filtered = harness.access_log.filter(&trilithon_core::access_log::Filter {
        host: Some("test-host-42.invalid".into()),
        status_code: Some(200),
        ..Default::default()
    }).await;
    let elapsed = started.elapsed();
    assert!(elapsed < std::time::Duration::from_millis(200),
        "filter took {elapsed:?}, budget 200ms");

    let entry = filtered.first().expect("at least one entry");
    let explanation = harness.access_log.explain(entry).await;
    assert!(explanation.route_id.is_some());
}
```

### Algorithm

1. Ingest 10 million synthetic lines.
2. Run a representative filter; measure wall-clock duration.
3. Assert duration < 200 ms.
4. Pick the first matched entry; call `explain`; assert a route id is returned.

### Tests

- The named test above.

### Acceptance command

```
cargo test -p trilithon-adapters --test e2e_access_log_10m --release
```

### Exit conditions

- Filter time stays under the 200 ms budget.
- Explanation traces to a route id.

### Audit kinds emitted

None new at this layer.

### Tracing events emitted

- `http.request.received`, `http.request.completed` (architecture §12.1) when the explanation is served via HTTP. The flow test calls the in-process API and does not exercise the HTTP boundary.

### Cross-references

- PRD T2.5, T2.6.
- Architecture §13 (performance budget).

---

## Slice 27.6 — T2.9 + T2.12 native bundle round-trip flow

### Goal

A scripted flow exports a native bundle, wipes the data directory, restores from the bundle, and asserts the resulting `DesiredState` byte-equals the original under canonical serialisation. Same machine and cross-machine variants both pass.

### Entry conditions

- Phases 25 and 26 complete.

### Files to create or modify

- `core/crates/adapters/tests/e2e_bundle_round_trip.rs`.

### Signatures and shapes

```rust
#[tokio::test(flavor = "multi_thread")]
async fn bundle_round_trip_same_machine_byte_equal() {
    let harness = TestHarness::new().await;
    harness.seed_representative_state().await;
    let original = harness.canonical_desired_state_bytes().await;

    let bundle = harness.export_bundle("passphrase").await;

    harness.wipe_data_dir().await;
    harness.restore_bundle(&bundle, "passphrase").await
        .expect("happy-path restore");

    let restored = harness.canonical_desired_state_bytes().await;
    assert_eq!(original, restored,
        "canonical desired-state bytes MUST match after round-trip");
}

#[tokio::test(flavor = "multi_thread")]
async fn bundle_round_trip_cross_machine_writes_cross_machine_audit_row() {
    let machine_a = TestHarness::new().await;
    machine_a.seed_representative_state().await;
    let bundle = machine_a.export_bundle("passphrase").await;

    let machine_b = TestHarness::new_with_distinct_installation_id().await;
    let outcome = machine_b.restore_bundle(&bundle, "passphrase").await
        .expect("happy-path cross-machine restore");
    assert_ne!(outcome.source_installation_id, outcome.new_installation_id);

    let last = machine_b.last_audit_kind().await;
    assert_eq!(last, "system.restore-cross-machine");
}
```

### Algorithm

`same machine`: export, wipe, restore, compare canonical byte streams.

`cross machine`: distinct installation id on the target; assert `RestoreCrossMachine` audit row.

### Tests

- The two named tests above.

### Acceptance command

```
cargo test -p trilithon-adapters --test e2e_bundle_round_trip
```

### Exit conditions

- Both flows pass.
- The canonical byte equality check holds.

### Audit kinds emitted

- `export.bundle`, `system.restore-applied`, `system.restore-cross-machine` (architecture §6.6).

### Tracing events emitted

- `http.request.received`, `http.request.completed` (architecture §12.1).

### Cross-references

- ADR-0009, ADR-0014.
- `bundle-format-v1.md`.
- PRD T2.9, T2.12.

---

## Slice 27.7 — Performance verification at 5,000 routes

### Goal

A criterion-based benchmark plus a CI workflow assert four performance budgets at 5,000 routes:

- Route list render < 1 second.
- Single mutation apply: median < 1.5 s, p99 < 7 s.
- Drift-check tick < 5 s.
- Memory ceiling < 400 MiB resident at idle.

### Entry conditions

- Phase 16 complete; Tier 2 phases complete.

### Files to create or modify

- `core/crates/adapters/benches/perf_5000_routes.rs` — criterion benchmarks.
- `.github/workflows/perf.yml` — CI workflow.

### Signatures and shapes

```rust
//! Performance verification at 5,000 routes (Phase 27).

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_route_list_render_5000(c: &mut Criterion) {
    c.bench_function("route_list_render_5000", |b| {
        let harness = test_support::seeded_harness_with_routes(5_000);
        b.iter(|| {
            let html = harness.render_route_list();
            black_box(html);
        });
    });
}

fn bench_single_mutation_apply_5000(c: &mut Criterion) {
    c.bench_function("single_mutation_apply_5000", |b| {
        let harness = test_support::seeded_harness_with_routes(5_000);
        b.iter(|| {
            let outcome = harness.submit_and_apply_one();
            black_box(outcome);
        });
    });
}

fn bench_drift_check_tick_5000(c: &mut Criterion) {
    c.bench_function("drift_check_tick_5000", |b| {
        let harness = test_support::seeded_harness_with_routes(5_000);
        b.iter(|| {
            let report = harness.drift_check_tick();
            black_box(report);
        });
    });
}

criterion_group!(perf,
    bench_route_list_render_5000,
    bench_single_mutation_apply_5000,
    bench_drift_check_tick_5000);
criterion_main!(perf);
```

`.github/workflows/perf.yml` (verbatim):

```yaml
name: perf

on:
  workflow_dispatch:
  schedule:
    - cron: '0 5 * * 1'

jobs:
  bench:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: |
          cd core
          cargo bench --bench perf_5000_routes -- --output-format=json | tee perf.json
      - name: Enforce budgets
        run: |
          set -euo pipefail
          jq -e '
            .[] |
            select(.id == "route_list_render_5000")     | .typical.estimate < 1000000000 and
            select(.id == "single_mutation_apply_5000") | .typical.estimate < 1500000000 and
            select(.id == "drift_check_tick_5000")      | .typical.estimate < 5000000000
          ' core/perf.json
      - name: Memory ceiling
        run: |
          cd core
          cargo run --release --bin perf-soak -- --routes 5000 --duration 60s | tee soak.log
          # `perf-soak` prints `peak_rss_bytes=<n>`. Budget: 400 MiB = 419430400.
          rss=$(awk -F= '/^peak_rss_bytes=/ {print $2}' soak.log)
          test "${rss}" -lt 419430400
```

### Algorithm

1. Seed a harness with 5,000 routes.
2. Run criterion benchmarks for the three latency budgets.
3. Parse criterion's JSON output; assert each budget.
4. Run a 60-second soak via `perf-soak`; assert peak RSS under 400 MiB.

### Tests

- The criterion benchmarks themselves.
- The CI workflow's `jq` predicate is the gate.

### Acceptance command

```
cd core && cargo bench --bench perf_5000_routes
```

### Exit conditions

- All four budgets met or filed as known regressions with open issues per architecture §13.

### Audit kinds emitted

None at the benchmark layer.

### Tracing events emitted

- `apply.started`, `apply.succeeded`, `drift.detected` (architecture §12.1) — emitted by the in-process daemon during benches.

### Cross-references

- Architecture §13.
- PRD T1.1, T1.4, T1.8.

---

## Slice 27.8 — Install/upgrade matrix

### Goal

A CI matrix exercises every supported V1 deployment cell:

1. Compose fresh on Linux.
2. Compose fresh on macOS.
3. Compose upgrade-from-prior on Linux.
4. systemd fresh on Ubuntu 24.04.
5. systemd fresh on Debian 12.
6. Tier 1 → Tier 2 SQLite schema upgrade.

The "downgrade" cell records the upgrade-only verdict per architecture §14.

### Entry conditions

- Phases 23 and 24 smoke flows exist (slices 23.6, 23.7, 24.7).
- Phase 26 backup/restore round-trip exists.

### Files to create or modify

- `.github/workflows/install-upgrade-matrix.yml`.
- `docs/release/v1-matrix.md`.

### Signatures and shapes

`.github/workflows/install-upgrade-matrix.yml` (verbatim):

```yaml
name: install-upgrade-matrix

on:
  workflow_dispatch:
  push:
    tags: ['v*']

jobs:
  compose-fresh-linux:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - run: bash deploy/compose/test/smoke.sh

  compose-fresh-macos:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4
      - run: brew install docker docker-compose
      - run: bash deploy/compose/test/smoke.sh

  compose-upgrade:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - run: bash deploy/compose/test/upgrade-from-prior.sh

  systemd-ubuntu:
    runs-on: ubuntu-24.04
    container: { image: 'ubuntu:24.04', options: '--privileged' }
    steps:
      - uses: actions/checkout@v4
      - run: bash deploy/systemd/test/smoke.sh

  systemd-debian:
    runs-on: ubuntu-24.04
    container: { image: 'debian:12', options: '--privileged' }
    steps:
      - uses: actions/checkout@v4
      - run: bash deploy/systemd/test/smoke.sh

  tier1-to-tier2-schema-upgrade:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: |
          cd core
          cargo test --test schema_upgrade_tier1_to_tier2

  matrix-summary:
    needs:
      - compose-fresh-linux
      - compose-fresh-macos
      - compose-upgrade
      - systemd-ubuntu
      - systemd-debian
      - tier1-to-tier2-schema-upgrade
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - run: |
          {
            echo "## V1 install/upgrade matrix"
            echo
            echo "| Cell | Status |"
            echo "| --- | --- |"
            echo "| compose-fresh-linux | passed |"
            echo "| compose-fresh-macos | passed |"
            echo "| compose-upgrade | passed |"
            echo "| systemd-ubuntu | passed |"
            echo "| systemd-debian | passed |"
            echo "| tier1-to-tier2-schema-upgrade | passed |"
            echo "| downgrade | upgrade-only (OUT OF SCOPE FOR V1) |"
          } >> "${GITHUB_STEP_SUMMARY}"
```

`docs/release/v1-matrix.md` outline:

```markdown
# V1 install and upgrade matrix

Recorded by `.github/workflows/install-upgrade-matrix.yml` on every
release tag.

| Cell | Verdict | Source |
| --- | --- | --- |
| Compose fresh on Linux | covered | slice 23.6 (`smoke.sh`) |
| Compose fresh on macOS | covered | slice 23.6 + macos-14 runner |
| Compose upgrade-from-prior | covered | slice 23.7 (`upgrade-from-prior.sh`) |
| systemd fresh on Ubuntu 24.04 | covered | slice 24.7 (`smoke.sh`) |
| systemd fresh on Debian 12 | covered | slice 24.7 (`smoke.sh`) |
| Tier 1 → Tier 2 schema upgrade | covered | `schema_upgrade_tier1_to_tier2` integration test |
| Downgrade | upgrade-only (OUT OF SCOPE FOR V1) | architecture §14 |
```

### Algorithm

1. Run each cell as an independent CI job.
2. The `matrix-summary` job runs after all cells, writing the table to the workflow summary.
3. Record the upgrade-only verdict for downgrade per architecture §14.

### Tests

- The matrix workflow itself.
- A separate `schema_upgrade_tier1_to_tier2` integration test asserting a Phase 16 database upgrades cleanly to Phase 27 schema.

### Acceptance command

```
gh workflow run install-upgrade-matrix.yml
```

### Exit conditions

- Every cell runs cleanly in CI.
- The downgrade verdict is recorded.
- `docs/release/v1-matrix.md` exists with the table.

### Audit kinds emitted

- `storage.migrations.applied` audit row from the schema upgrade test (architecture §14).

### Tracing events emitted

- `storage.migrations.applied` (architecture §12.1).

### Cross-references

- Architecture §14.
- PRD T2.7, T2.8, T2.12.

---

## Slice 27.9 — Security review document and H11/H16 dedicated re-review

### Goal

`docs/architecture/security-review.md` is updated for Tier 2: every hazard H1 through H17 receives a written confirmation paragraph against the Tier 2 surface; H11 (Docker socket trust boundary) and H16 (language-model prompt injection) receive dedicated re-review sections.

### Entry conditions

- Every Tier 2 phase complete.

### Files to create or modify

- `docs/architecture/security-review.md` — full document.
- `docs/architecture/test/lint-security-review.sh` — heading lint enforcing one section per hazard.

### Signatures and shapes

`docs/architecture/security-review.md` outline:

```markdown
# Security review — V1 (Tier 1 + Tier 2)

This document confirms every hazard in
`docs/prompts/PROMPT-spec-generation.md` §7 against the V1 surface.

## H1 — Caddy admin endpoint exposure
…

## H2 — Stale-upstream rollback
…

## H3 — Wildcard-certificate over-match
…

## H4 — Hot-reload connection eviction
…

## H5 — Capability mismatch
…

## H6 — Time-zone confusion in audit logs
…

## H7 — Caddyfile escape lock-in
…

## H8 — Concurrent modification
…

## H9 — Caddy version skew across snapshots
…

## H10 — Secrets in audit diffs
…

## H11 — Docker socket trust boundary (Tier 2 dedicated re-review)
…

## H12 — Multi-instance leak via fat-finger
…

## H13 — Bootstrap account credential leak
…

## H14 — Database corruption
…

## H15 — Configuration import that hangs the proxy
…

## H16 — Language-model prompt injection through user data (Tier 2 dedicated re-review)
…

## H17 — Apply-time TLS provisioning
…
```

`docs/architecture/test/lint-security-review.sh` (verbatim):

```bash
#!/usr/bin/env bash
set -euo pipefail
for n in $(seq 1 17); do
    if ! grep -Eq "^## H${n} — " docs/architecture/security-review.md; then
        echo "lint-security-review: missing H${n} section" >&2
        exit 1
    fi
done
echo "lint-security-review: ok (17 hazards confirmed)"
```

### Algorithm

1. Author one section per hazard with a written confirmation paragraph against the Tier 2 surface.
2. H11 and H16 each receive an extended dedicated re-review section.
3. The lint asserts every section's heading prefix.

### Tests

- The heading lint passes.
- A peer review (the V1 release-readiness check) reads the document.

### Acceptance command

```
bash docs/architecture/test/lint-security-review.sh
```

### Exit conditions

- The document covers H1 through H17.
- H11 and H16 sections are clearly labelled "Tier 2 dedicated re-review".

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- Hazards: H1–H17 (PROMPT-spec-generation.md §7).

---

## Slice 27.10 — V1 release readiness: release notes, doc audit, Tier 1 regression guard

### Goal

`docs/release/v1-release-notes.md` lists every T1.x and T2.x feature with its acceptance status linked to the corresponding test. A `cargo doc -D rustdoc::missing_docs` build asserts every public Rust item has a doc comment. A user-facing documentation index covers installation, bootstrap, first route, drift, rollback, secrets reveal, language-model setup, Docker discovery, backup, restore, and uninstall. A CI job re-runs every Phase 16 sign-off check.

### Entry conditions

- Slices 27.1 through 27.9 complete.

### Files to create or modify

- `docs/release/v1-release-notes.md` — release notes.
- `.github/workflows/tier-1-regression.yml` — re-runs every Phase 16 acceptance check.
- `docs/index.md` — user-facing documentation index.
- `core/.cargo/config.toml` (or workspace lints in `core/Cargo.toml`) — enable `rustdoc::missing_docs`.

### Signatures and shapes

`docs/release/v1-release-notes.md` outline:

```markdown
# Trilithon V1 — Release notes

## Tier 1 features

| ID | Feature | Acceptance | Test reference |
| --- | --- | --- | --- |
| T1.1 | Configuration ownership loop | passed | core/crates/adapters/tests/apply_path_*.rs |
| T1.2 | Snapshot history with content addressing | passed | core/crates/adapters/tests/snapshot_writer_*.rs |
| T1.3 | One-click rollback with preflight | passed | core/crates/adapters/tests/rollback_preflight_*.rs |
| ... | ... | ... | ... |
| T1.15 | Secrets abstraction | passed | core/crates/adapters/tests/secrets_vault_*.rs |

## Tier 2 features

| ID | Feature | Acceptance | Test reference |
| --- | --- | --- | --- |
| T2.1 | Docker container discovery | passed | core/crates/adapters/tests/e2e_docker_discovery.rs |
| T2.2 | Policy presets | passed | core/crates/adapters/tests/e2e_policy_preset_degradation.rs |
| T2.3 | Language-model "explain" mode | passed | core/crates/adapters/tests/e2e_explain_then_propose.rs |
| T2.4 | Language-model "propose" mode | passed | core/crates/adapters/tests/e2e_explain_then_propose.rs |
| T2.5 | Access log viewer | passed | core/crates/adapters/tests/e2e_access_log_10m.rs |
| T2.6 | Caddy access log explanation | passed | core/crates/adapters/tests/e2e_access_log_10m.rs |
| T2.7 | Bare-metal systemd | passed | deploy/systemd/test/smoke.sh |
| T2.8 | Two-container Docker Compose | passed | deploy/compose/test/smoke.sh |
| T2.9 | Configuration export | passed | core/crates/adapters/tests/export_*.rs |
| T2.10 | Concurrency control | passed | core/crates/adapters/tests/e2e_concurrent_mutation.rs |
| T2.11 | Wildcard-certificate proposal callout | passed | core/crates/adapters/tests/e2e_docker_discovery.rs |
| T2.12 | Backup and restore | passed | core/crates/adapters/tests/e2e_bundle_round_trip.rs |
```

`.github/workflows/tier-1-regression.yml` (verbatim):

```yaml
name: tier-1-regression

on:
  push:
    branches: [main]
  pull_request:

jobs:
  re-run-phase-16:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: |
          cd core
          # Every Phase 16 sign-off test, re-run on every push/PR.
          cargo test --test failure_modes_tier_1
          cargo test --test perf_budgets_tier_1
          cargo test --test security_review_tier_1
```

`core/Cargo.toml` (workspace lints):

```toml
[workspace.lints.rust]

[workspace.lints.rustdoc]
missing_docs = "deny"
```

### Algorithm

1. Author the release notes table linking every Tier 1/Tier 2 feature to its acceptance test.
2. Author the user-facing documentation index covering the eleven topics.
3. Enable `rustdoc::missing_docs = "deny"` in the workspace lints.
4. Run `cargo doc --workspace --no-deps -D rustdoc::missing_docs` in CI; fail on any missing public-item doc.
5. Add the Tier 1 regression workflow that re-runs every Phase 16 acceptance test on every push and pull request.

### Tests

- `cargo doc --workspace --no-deps` (with `RUSTDOCFLAGS="-D rustdoc::missing_docs"`) passes.
- The Tier 1 regression workflow passes.
- A heading lint asserts every required topic heading in the documentation index.

### Acceptance command

```
RUSTDOCFLAGS="-D rustdoc::missing_docs" cargo doc --workspace --no-deps \
  && cargo test --test failure_modes_tier_1 \
  && cargo test --test perf_budgets_tier_1 \
  && cargo test --test security_review_tier_1
```

### Exit conditions

- Release notes list every Tier 1/2 feature with passing acceptance.
- The Tier 1 regression workflow passes.
- `cargo doc -D rustdoc::missing_docs` succeeds.
- The user-facing documentation index covers every required topic.

### Audit kinds emitted

None at this layer.

### Tracing events emitted

None at this layer.

### Cross-references

- Architecture §13, §14.
- ADRs 0001–0016.
- PRD T1.1–T1.15, T2.1–T2.12.

---

## Phase exit checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] Every Tier 2 end-to-end flow test passes in CI (slices 27.1 through 27.6).
- [ ] Every Tier 2 performance budget is met or recorded as a known regression with an open issue (slice 27.7).
- [ ] Every hazard has an updated written confirmation paragraph; H11 and H16 have dedicated re-review sections (slice 27.9).
- [ ] The install-and-upgrade matrix is exercised in CI for every supported target (slice 27.8).
- [ ] V1 release notes are published, listing every T1.x and T2.x feature and its acceptance status (slice 27.10).
- [ ] `cargo doc -D rustdoc::missing_docs` passes (slice 27.10).
- [ ] The Tier 1 regression workflow re-runs every Phase 16 acceptance check on every push and pull request (slice 27.10).
