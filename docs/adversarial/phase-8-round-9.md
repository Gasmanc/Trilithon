# Phase 8 Adversarial Review — Round 9

**Date:** 2026-05-08
**Severity summary:** 1 critical · 0 high · 0 medium · 0 low

---

## New Findings (Round 9)

### F087 — Slice 8.5 exit conditions contain no spawn-verification criterion; `DriftDetector` is never wired into `run_with_shutdown`; the entire feature is silently dead [CRITICAL]

**Category:** Single points of failure

**Attack:** The design specifies that `core/crates/cli/src/main.rs` must be modified to "spawn the task during daemon bootstrap," but the slice 8.5 exit conditions contain only three verifiable criteria: (1) default interval is 60 seconds, (2) apply-in-flight tick is skipped without audit, (3) clean state writes no audit row. None require that the task is actually spawned in `run_with_shutdown`. An implementer who writes `DriftDetector` and all its methods satisfies every exit condition and acceptance command without ever adding the spawn call to the daemon entry point.

The existing `core/crates/cli/src/run.rs` currently spawns exactly two tasks (`run_integrity_loop`, `reconnect_loop`) followed by `daemon_loop`. No `DriftDetector` construction or spawn is present. The design gives no method signature for the wiring, no pattern for constructing the `Arc<DriftDetector>` (required by F062's fix), no position in the startup sequence, and no shutdown-channel wiring.

**Scenario:**
1. Implementer writes `DriftDetector`, `tick_once`, `record`, `mark_resolved` in `adapters/src/drift.rs`.
2. All `drift_*` unit and integration tests pass. All slice 8.5 and 8.6 acceptance tests pass.
3. No modification is made to `run_with_shutdown` — the spec provides nothing to drive this wiring.
4. `just check` passes. CI is green.
5. Daemon starts. Every health check returns 200. Integrity loop and reconnect loop run normally.
6. Zero `DriftDetector` ticks ever occur. Zero `config.drift-detected` rows are ever written.
7. Caddy configuration drifts silently in production forever. The drift feature is live in the binary but completely inert.

**Design gap:** Slice 8.5 must add a fourth exit condition: the drift-detector task is registered in `run_with_shutdown`'s `JoinSet` before the daemon emits the `daemon.started` tracing event. An integration test (`drift_task_registered_at_startup`) must verify this. Additionally, the design must specify: (a) the `watch::channel(false)` sender held by `ShutdownController` and how the `watch::Receiver` reaches `DriftDetector::run`; (b) the `Arc<DriftDetector>` construction pattern; (c) the position in the startup sequence (after sentinel check per F025, after capability probe per §7.4).

---

## Surfaces examined and cleared

- **`sha2` crate absence**: The workspace `Cargo.toml` already declares `sha2 = "0.10"` and `core/crates/core/Cargo.toml` includes it. SHA-256 computation compiles. Not a blocker.
- **`sqlx::query!` compile-time schema check**: The existing `SqliteStorage` consistently uses the dynamic `sqlx::query()` form, not the compile-time `sqlx::query!()` macro. The missing `drift_events` table (F048) is a runtime failure already documented, not a compile-time blocker. Not a new finding.
- **`with_correlation_span` discarding `TickError`**: Phase 6 specifies the signature as `-> impl Future<Output = F::Output>`, preserving the inner future's return type. The premise is false; `tick_once`'s `Result` is not discarded.
- **`mark_resolved` unreachable after `run(self)`**: Fully subsumed by F062. Fixing F062 (change `run` to take `Arc<Self>`) makes `mark_resolved` reachable. Not independently critical.

---

## Summary

**Critical:** 1 (F087)
**High:** 0
**Medium:** 0
**Low:** 0

**Top concern:** F087 is the most dangerous class of feature failure — undetectable by any test that exercises the detector's internal logic, satisfying all stated acceptance criteria, and resulting in drift detection being permanently inert in every production deployment with no observable signal.

**Verdict:** One critical surface remains. All other examined surfaces are clear.
