---
id: security:area::phase-1-gemini-review-findings:legacy-uncategorized
category: security
kind: process
location:
  area: phase-1-gemini-review-findings
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

# Phase 1 — Gemini Review Findings

**Reviewer:** gemini
**Date:** 2026-05-06T13:40:00Z
**Diff range:** 3734e02^..be773df -- core/ web/
**Phase:** 1

---

[CRITICAL] Tracing initialized with hardcoded defaults before config loading
File: core/crates/cli/src/main.rs
Lines: 36-49
Description: The tracing subscriber is initialized with hardcoded defaults ("info,trilithon=info", Pretty format) before the CLI subcommands are dispatched. When `run_daemon` or `config_show::run` later load the actual configuration file, the subscriber is not updated, and a second call to `observability::init` is ignored because a global subscriber is already installed. This prevents users from configuring log levels or formats via `config.toml`, violating the Phase 1 specification (Slice 1.4).
Suggestion: Move `observability::init` call into `run_daemon` and `config_show::run` (or a shared loader) after the `DaemonConfig` is successfully loaded from disk/env.

---

[HIGH] Background tasks not awaited during graceful shutdown
File: core/crates/cli/src/run.rs
Lines: 104-108, 146-154, 182-195
Description: `run_with_shutdown` spawns several background tasks (`integrity_loop`, `reconnect_loop`) via `tokio::spawn` but only awaits the handle for `daemon_loop` during the shutdown phase. When `run_with_shutdown` returns after the `DRAIN_BUDGET` timeout (or `daemon_loop` completion), the Tokio runtime is dropped in `main.rs`, abruptly killing all other tasks at their last yield point. This may lead to interrupted database writes (e.g., in `run_initial_probe` called by the reconnect loop) or inconsistent state.
Suggestion: Use a `tokio::task::JoinSet` or collect task handles into a `Vec` and use `futures::future::join_all` (with timeout) to ensure all background loops have processed the shutdown signal and exited cleanly before dropping the runtime.

---

[WARNING] Brittle manual mirror for redacted configuration
File: core/crates/core/src/config/types.rs
Lines: 167-183, 225-258
Description: `RedactedConfig` is a manual mirror of `DaemonConfig`. While some fields use redacted variants, others like `StorageConfig`, `ConcurrencyConfig`, and `TracingConfig` are included directly via `Clone`. If a secret-bearing field is added to `StorageConfig` in a future phase (e.g., a DB password), it will be silently leaked by `config show` unless the developer remembers to also create a `RedactedStorageConfig` mirror.
Suggestion: Implement redaction via a trait or use a crate like `secrecy` for sensitive fields, or at least add a warning comment in `DaemonConfig` noting the dependency on the manual `RedactedConfig` mirror.

---

[WARNING] Inefficient and fragile log timestamp injection
File: core/crates/cli/src/observability.rs
Lines: 154-167
Description: `inject_ts_unix_seconds` assumes every buffer passed to the writer starts with `{` to identify JSON lines. This is fragile if other layers are added or if the formatter output changes. Furthermore, the function returns `buf.to_vec()` (an allocation) for every non-JSON log line (e.g., when using `Pretty` format), which is inefficient for high-volume logging.
Suggestion: Use `Cow&lt;'a, [u8]&gt;` to avoid allocations for non-matching lines, and consider a more robust check or a custom `FormatEvent` implementation to inject the timestamp correctly within the `tracing-subscriber` pipeline.

---

[SUGGESTION] Pre-tracing line noise on non-daemon commands
File: core/crates/cli/src/main.rs
Lines: 23-30
Description: The `trilithon: starting (pre-tracing)` line is written to stderr at the very start of `main`, before command dispatch. This means even simple commands like `version` or `help` produce this noise, which violates standard CLI expectations for "clean" output.
Suggestion: Move the pre-tracing line emission into `run_daemon` and `config_show::run` so it only appears when the full application logic is actually being initialized.

---

[SUGGESTION] Inconsistent CLI exit codes for usage errors
File: core/crates/cli/src/main.rs
Lines: 32
Description: `Cli::parse()` uses `clap`'s default behavior, which exits with code 2 on usage errors. However, `trilithon_core::exit::ExitCode` defines `InvalidInvocation` as 64 (matching `EX_USAGE` from `sysexits.h`). This creates an inconsistency where some invocation errors exit with code 2 and others (like `--allow-remote-admin`) exit with code 2 via `ConfigError` or potentially 64 elsewhere.
Suggestion: Use `Cli::try_parse()` and map `clap::Error` to `ExitCode::InvalidInvocation` to maintain the stable exit code contract defined in `core`.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-07 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Tracing initialized with hardcoded defaults before config loading | ✅ Fixed | `aea27e8` | — | 2026-05-07 | F002 |
| 2 | Background tasks not awaited during graceful shutdown | ✅ Fixed | `06c9c22` | — | 2026-05-07 | F004 |
| 3 | Brittle manual mirror for redacted configuration | ✅ Fixed | `263af98` | — | 2026-05-07 | F023 |
| 4 | Inefficient and fragile log timestamp injection | 🚫 Won't Fix | — | — | 2026-05-07 | Addressed by F021 fix — timestamp now captured at make_writer() time (F024) |
| 5 | Pre-tracing line noise on non-daemon commands | ✅ Fixed | `aa8b2a9` | — | 2026-05-07 | F013 |
| 6 | Inconsistent CLI exit codes for usage errors | ✅ Fixed | `28e7b24` | — | 2026-05-07 | F006 |
