# Phase 1 — Tagging Analysis
**Generated:** 2026-05-01
**Model:** opus (extended thinking)
**Documents read:**
- CLAUDE.md (184)
- architecture.md (1124) — §4.1, §4.3, §5, §9, §11, §12, §12.1
- trait-signatures.md (734) — §9 (EnvProvider), §10 (HttpServer)
- PRD.md (952) — sections covering T1.1–T1.2, T1.13, T1.15, hazard H6
- phase-01-daemon-skeleton.md (185)
- ADRs read: 0003 rust-three-layer-workspace (160), 0010 two-container-deployment (182), 0011 loopback-only-by-default (194), 0014 secrets-at-rest (209), 0015 instance-ownership-sentinel (217)
- phase-01 TODO (923)

**Slices analysed:** 7

---

## Proposed Tags

### Slice 1.1: Workspace skeleton, exit codes, `version` subcommand
**Proposed tag:** [cross-cutting]
**Reasoning:** Files span two crates (`crates/core/src/exit.rs` plus `crates/cli/src/{main,cli,exit}.rs` and `crates/cli/build.rs`) and explicitly cross the core ↔ cli layer boundary that ADR-0003 enforces. The `core::exit::ExitCode` enum it introduces is a shared type that every later slice in this and subsequent phases consumes (1.6 returns it from `run_config_show`, 1.5 returns it from main, 1.7 asserts its values at the binary edge). The `clap` command surface (`run`, `config show`, `version`) is also the foundational dispatch contract that 1.4–1.7 wire into. Mechanically simple per-file, but structurally load-bearing and multi-crate.
**Confidence:** high

### Slice 1.2: `DaemonConfig` typed records in `core`
**Proposed tag:** [standard]
**Reasoning:** All file changes are confined to one crate (`crates/core/src/config/{mod,types}.rs` and a `lib.rs` re-export); no I/O, no async, no trait introduced. The `DaemonConfig` data model and the `redacted()` accessor are non-trivial in size and are consumed by later slices, but they are concrete types rather than a shared trait surface. The slice cites ADR-0011/0014/0015 for field semantics, but the work itself is a single-layer type-only addition that fits the "self-contained but non-trivial" definition rather than a layer-crossing change.
**Confidence:** high

### Slice 1.3: `EnvProvider` trait, TOML loader, `ConfigError`
**Proposed tag:** [cross-cutting]
**Reasoning:** Files span two crates: the trait + `EnvError` land in `crates/core/src/config/env.rs` (matching trait-signatures.md §9 verbatim) while `StdEnvProvider`, `load_config`, and `ConfigError` land in `crates/adapters/src/{config_loader,env_provider}.rs`. This crosses the core ↔ adapters boundary and introduces a new shared trait (`EnvProvider`) that other adapters and the cli will program against. It is the slice that first introduces real I/O (file read, data-dir writability probe) into the workspace, and `ConfigError` is the typed error consumed by both 1.6 (`config show`) and 1.7 (exit-code tests).
**Confidence:** high

### Slice 1.4: Tracing subscriber, pre-tracing line, UTC-seconds layer
**Proposed tag:** [cross-cutting]
**Reasoning:** Although the file changes are nominally in one crate (`crates/cli/src/observability.rs` plus `main.rs`), this slice establishes the tracing convention that every subsequent slice and phase MUST follow: the `UtcSecondsLayer` stamps `ts_unix_seconds` on every event project-wide (satisfying H6), it fixes the JSON vs Pretty dispatch contract, and it emits `daemon.started` from the closed event vocabulary in architecture §12.1 that 1.5, 1.7, and all later phases consume. The rubric explicitly flags "introduces a tracing/audit/logging convention other slices must follow" as cross-cutting.
**Confidence:** high

### Slice 1.5: Signal handling and graceful shutdown
**Proposed tag:** [cross-cutting]
**Reasoning:** Introduces the `ShutdownController` / `ShutdownSignal` watch-channel pattern in `crates/cli/src/shutdown.rs` that every owned async task in this and later phases must subscribe to in order to drain cleanly — a structural convention other slices depend on. Emits two more events from the closed `daemon.*` vocabulary (`daemon.shutting-down`, `daemon.shutdown-complete`) and adds the `cfg(not(unix)) compile_error!` gate that operationalises ADR-0010's deployment scope. References ADR-0010 plus architecture §9 and §12.1; the shutdown contract is load-bearing for every future runtime task.
**Confidence:** high

### Slice 1.6: `config show` with redaction
**Proposed tag:** [standard]
**Reasoning:** Files are confined to the cli crate (`crates/cli/src/cli.rs` plus `tests/config_show.rs` and a snapshot fixture). It composes already-built pieces — `load_config` from 1.3 and `DaemonConfig::redacted` from 1.2 — without introducing a new trait, event, or layer-spanning type. It does dispatch through cli → adapters → core, but only as a consumer of established surfaces; no new shared abstraction is created. Insta snapshot test is local to this slice.
**Confidence:** high

### Slice 1.7: End-to-end exit-code and signal integration tests
**Proposed tag:** [standard]
**Reasoning:** Pure integration tests under `crates/cli/tests/` (`missing_config.rs`, extended `signals.rs`, `utc_timestamps.rs`) plus a `core/README.md` documentation update with the exit-code table. No new traits, no new layer crossings, no shared events introduced — every event and exit code asserted here was established by 1.1, 1.4, or 1.5. The README touch is documentation, not a structural dependency. Self-contained verification slice.
**Confidence:** high

---

## Summary
- 2 trivial → **0 trivial**
- 3 standard
- 4 cross-cutting
- 0 low-confidence

## Notes

- **No trivial slices in Phase 1.** This is expected for a foundational phase: every slice either spans crates or introduces a shared convention (exit codes, tracing layer, shutdown channel, EnvProvider trait, daemon.* event vocabulary) that downstream phases depend on. There is no pure "add an enum variant" change here.
- **Three of the four cross-cutting slices (1.1, 1.4, 1.5) are mechanically modest but structurally load-bearing.** Tagging them `[standard]` would under-reason the conventions they set: get the ExitCode repr wrong, the UtcSecondsLayer field name wrong, or the ShutdownSignal contract wrong, and every subsequent slice and phase has to be retro-fitted. The cross-cutting tag exists precisely to buy extra reasoning at these establishing points.
- **1.3 is the only cross-cutting slice that is also large in scope** — it introduces the first cross-crate trait (`EnvProvider`), the first I/O in the workspace, and the typed `ConfigError` that the cli edge translates to exit codes. It deserves the deepest reasoning budget of the seven.
- **1.6 and 1.7 are honest standards.** They consume established surfaces and verify behaviour; they do not introduce new conventions. Tagging them cross-cutting would waste compute.
- **1.2 is a judgement call.** It cites three ADRs (0011/0014/0015), which is one rubric trigger for cross-cutting, but the rubric's "3+ ADRs" criterion is intended to flag slices whose decisions are entangled across multiple architectural concerns; 1.2 only encodes the field shape that those ADRs already specify, in a single crate, with no I/O. Standard is the right call; the cross-cutting reasoning lives in the slices that act on these types (1.3, 1.6).

---

## User Decision
**Date:** 2026-05-01
**Decision:** accepted

### Modifications (if any)
None — all 7 tags accepted as proposed.

### Notes from user
First smoke test of the /tag-phase skill. Analysis was sound: model correctly distinguished mechanically-simple from structurally-load-bearing slices.
