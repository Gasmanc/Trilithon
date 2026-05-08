---
id: security:area::phase-1-qwen-review-findings:legacy-uncategorized
category: security
kind: process
location:
  area: phase-1-qwen-review-findings
  multi: false
finding_kind: legacy-uncategorized
phase_introduced: unknown
status: open
created_at: migration
created_by: legacy-migration
last_verified_at: 0a795583ea9c4266e7d9b0ae0f56fd47d2ecf574
severity: medium
do_not_autofix: false
---

# Phase 1 — Qwen Review Findings

**Reviewer:** qwen
**Date:** 2026-05-06T13:00:00Z
**Diff range:** 3734e02^..be773df -- core/ web/
**Phase:** 1

---

I've reviewed the Phase 1 diff. Here are the structured findings:

---

[WARNING] Unnecessary `nix` crate in workspace-level `[dependencies]`
File: core/Cargo.toml
Lines: 27
Description: `nix` (with signal, process, fs, user features) is added to the workspace-level `[dependencies]` section alongside test crates (`assert_cmd`, `predicates`, `insta`). The `nix` crate is Unix-specific and should not be a workspace dependency — it belongs in `adapters` or `cli` dev-dependencies (and indeed `adapters/Cargo.toml` already declares `nix` as a dev-dependency separately). Its presence here risks accidental use in `core`.
Suggestion: Remove `nix` from the workspace `[dependencies]` section; it is already correctly scoped as a dev-dependency in `core/crates/adapters/Cargo.toml`.

[WARNING] Test dependencies in workspace `[dependencies]` rather than `[dev-dependencies]`
File: core/Cargo.toml
Lines: 25-29
Description: `assert_cmd`, `predicates`, `regex`, and `insta` are test-only crates placed in the workspace `[dependencies]` section. These should live in `[dev-dependencies]` to prevent accidental production use and to avoid being pulled in during `cargo build --release`.
Suggestion: Move `assert_cmd`, `predicates`, `regex`, and `insta` to a `[dev-dependencies]` section in `core/Cargo.toml`, or remove them from the workspace entirely and rely on per-crate `[dev-dependencies]`.

[HIGH] `config_show.rs` writes to stderr after tracing is active — uses raw `writeln!` instead of `tracing::error!`
File: core/crates/cli/src/config_show.rs
Lines: 20-22
Description: `run()` writes errors to stderr via `writeln!(stderr, "trilithon: config error: {e}")`. At this point in `main.rs`, the tracing subscriber is already installed, so this error should go through `tracing::error!` for consistent structured logging. The raw write creates a format inconsistency with other error paths.
Suggestion: Replace the `writeln!` block with `tracing::error!(error = %e, "config.show.failed");` and rely on the tracing subscriber to emit to stderr.

[HIGH] Pre-tracing line always written even for commands that never need it
File: core/crates/cli/src/main.rs
Lines: 39-44
Description: The pre-tracing line `trilithon: starting (pre-tracing)` is written to stderr unconditionally before argument parsing. For `version` — which exits immediately — this line is noise. The `pre_tracing_line` integration test explicitly asserts this line appears for `version`, baking in the behavior, but it is still unnecessary output for a fast-exit command.
Suggestion: Move the pre-tracing write after `Cli::parse()` so it only fires for commands that proceed to daemon/work paths (or accept the current behavior as intentional if the spec requires it).

[SUGGESTION] `run_daemon` ignores loaded config — loads twice
File: core/crates/cli/src/main.rs
Lines: 92-103
Description: `run_daemon` calls `load_config` and discards the result, only to have `run::run_with_shutdown` presumably need the config later. This means the config is parsed and validated (including filesystem side effects) twice: once here for the validity check, and again inside the daemon loop.
Suggestion: Pass the loaded `DaemonConfig` through to `run::run_with_shutdown` instead of discarding it, or document that Phase 1 intentionally validates-before-start as a placeholder.

[SUGGESTION] `run_daemon` config error writes raw to stderr — should use tracing
File: core/crates/cli/src/main.rs
Lines: 98-100
Description: Same pattern as `config_show.rs` — after the tracing subscriber is installed, config errors are written via raw `writeln!` instead of `tracing::error!`.
Suggestion: Use `tracing::error!(error = %e, "config.load.failed")` before returning.

[SUGGESTION] `TsWriter` bypasses tracing's built-in JSON timestamping
File: core/crates/cli/src/observability.rs
Lines: 175-230
Description: The `TsWriter`/`UtcSecondsLayer`/`inject_ts_unix_seconds` machinery performs a string-level JSON injection to add `ts_unix_seconds`. This is fragile — if the fmt layer changes its JSON output format (e.g., pretty-printing, additional escaping), the injection point breaks silently. The `time` crate already provides formatting; `tracing-subscriber`'s JSON layer has `with_timer` for custom timestamps.
Suggestion: Consider using `tracing_subscriber::fmt::time::UtcTime` with a custom `FormatTime` implementation that writes unix-seconds instead of the string-injection approach.

[SUGGESTION] `ObsError::BadFilter` error message leaks potentially sensitive filter strings
File: core/crates/cli/src/observability.rs
Lines: 39-42
Description: `ObsError::BadFilter { filter, detail }` stores and displays the filter string in error messages. While log filters are not secrets, if `RUST_LOG` or `config.log_filter` were ever derived from user-controlled input in the future, this could log arbitrary strings. Not currently a concern, but worth noting.
Suggestion: No change needed now; this is informational for future-proofing.

[SUGGESTION] `config_loader.rs` — `set_by_path` does not validate dotted_key syntax
File: core/crates/adapters/src/config_loader.rs
Lines: 220-242
Description: `set_by_path` splits on `.` and traverses recursively. An empty key segment (e.g., `server..bind` from a malformed `TRILITHON_SERVER____BIND`) would silently fail with `UnknownKey` rather than surfacing a descriptive error. This is functionally correct but the error message is misleading.
Suggestion: Add an empty-segment check in `set_by_path` that returns a new `EnvOverrideReason` variant like `InvalidKeySegment`.

[SUGGESTION] `load_config` — probe file creation could leak on interrupted processes
File: core/crates/adapters/src/config_loader.rs
Lines: 126-133
Description: If the process is killed (SIGKILL, power loss) between creating `.trilithon-write-probe` and removing it, the probe file remains. This is benign but could be confusing for users inspecting the data directory.
Suggestion: Accept as a known minor artifact; or use `tempfile::NamedTempFile` for the probe which auto-cleans on drop.

[WARNING] `ShutdownController::trigger` uses `watch::Sender::send` but drop-handling is incomplete
File: core/crates/cli/src/shutdown.rs
Lines: 102-104
Description: `trigger()` calls `self.tx.send(true)` and ignores the `Result` (which only errors if no receivers remain). If all `ShutdownSignal`s are dropped before `trigger()` is called, the send silently drops the value. This is unlikely in the current architecture (the `daemon_loop` holds the signal until `trigger()` is called), but there is no defensive guard.
Suggestion: Add a `debug!` or `trace!` log when `send` returns `Err` to aid future debugging.

[SUGGESTION] `env_provider.rs` — `StdEnvProvider` does not handle `&amp;str` key lifetime on error
File: core/crates/adapters/src/env_provider.rs
Lines: 12-15
Description: `EnvError::NotPresent { key: key.into() }` and `EnvError::NotUnicode { key: key.into() }` allocate a `String` for every key. This is fine for correctness but a minor allocation on every env lookup.
Suggestion: No change needed; the allocation is negligible for env-variable access patterns.

[SUGGESTION] `build.rs` — git command may fail in non-git builds (e.g., vendored tarballs)
File: core/crates/cli/build.rs
Lines: 10-14
Description: `std::process::Command::new("git")` is invoked at build time. In environments where git is not available (e.g., minimal CI containers, vendored source distributions), the fallback `"unknown"` is used silently. This is acceptable but could be noisy in logs.
Suggestion: Consider emitting a `cargo:warning=` when git is unavailable so packagers know the version info is incomplete.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-07 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Unnecessary nix crate + test deps in workspace [dependencies] | ✅ Fixed | `ac07b47` | — | 2026-05-07 | F001 — workspace dep cleanup |
| 2 | config_show.rs writes to stderr after tracing active (raw writeln!) | ✅ Fixed | `06c9c22` | — | 2026-05-07 | F011 |
| 3 | Pre-tracing line always written even for fast-exit commands | ✅ Fixed | `aa8b2a9` | — | 2026-05-07 | F013 |
| 4 | run_daemon ignores loaded config — loads twice | 🚫 Won't Fix | — | — | 2026-05-07 | False positive — config IS passed to run_with_shutdown (F014) |
| 5 | run_daemon config error writes raw to stderr instead of tracing | ✅ Fixed | `c646fb8` | — | 2026-05-07 | F053 |
| 6 | TsWriter bypasses tracing's built-in JSON timestamping | 🚫 Won't Fix | — | — | 2026-05-07 | Addressed by F021 fix — timestamp captured at make_writer() time (F024) |
| 7 | ObsError::BadFilter error message leaks filter strings | 🔕 Excluded | — | — | — | Informational only — no action needed |
| 8 | set_by_path does not validate dotted_key syntax | ✅ Fixed | `c646fb8` | — | 2026-05-07 | F050 |
| 9 | Probe file could leak on interrupted processes | ✅ Fixed | `263af98` | — | 2026-05-07 | F018 — PID-scoped probe file |
| 10 | env_provider.rs allocation on every key lookup | 🔕 Excluded | — | — | — | Informational only — negligible allocation |
| 11 | ShutdownController::trigger ignores send errors | ✅ Fixed | `263af98` | — | 2026-05-07 | F025 |
| 12 | build.rs git command fails silently in non-git environments | ✅ Fixed | `c646fb8` | — | 2026-05-07 | F051 |
