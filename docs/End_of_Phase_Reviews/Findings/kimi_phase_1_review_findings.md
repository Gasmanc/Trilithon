# Phase 1 — Kimi Review Findings

**Reviewer:** kimi
**Date:** 2026-05-06T13:15:00Z
**Diff range:** 3734e02^..be773df -- core/ web/
**Phase:** 1

---

[HIGH] Clap exit code conflicts with documented InvalidInvocation code
File: core/crates/cli/src/main.rs
Lines: 46
Description: `Cli::parse()` exits with code 2 on invalid arguments, but the project defines 2 as `ConfigError` and 64 as `InvalidInvocation`. This violates the documented exit-code contract.
Suggestion: Use `Cli::try_parse()`, print clap's error to stderr, and return `ExitCode::InvalidInvocation` for usage errors (exit 0 for `--help`/`--version`).

---

[HIGH] Tracing subscriber ignores config file [tracing] section
File: core/crates/cli/src/main.rs
Lines: 39-50
Description: `main()` installs the global tracing subscriber with a hardcoded `TracingConfig` before the config file is ever loaded. The `log_filter` and `format` values from `config.toml` are silently ignored.
Suggestion: Move tracing initialization after config is successfully loaded in `run_daemon`, or document that Phase 1 only respects `RUST_LOG`/`TRILITHON_LOG_FORMAT`.

---

[HIGH] Integration tests write to shared global /tmp paths
File: core/crates/adapters/tests/fixtures/minimal.toml, core/crates/cli/tests/fixtures/minimal.toml
Lines: general
Description: Test fixtures hard-code `data_dir = "/tmp/trilithon-test-data"` and `/tmp/trilithon-cs-test`. `load_config` creates probe files there, so concurrent or repeated test runs mutate shared global state and can race.
Suggestion: Generate unique temporary directories per test (e.g. with `tempfile`) and inject the path via an env override or a generated config file.

---

[WARNING] Invalid RUST_LOG is silently ignored
File: core/crates/cli/src/observability.rs
Lines: 115-122
Description: `build_filter` falls back to the hardcoded default whenever `EnvFilter::try_from_default_env()` fails, so a malformed `RUST_LOG` is swallowed without any warning.
Suggestion: Distinguish between `RUST_LOG` absent and `RUST_LOG` invalid; return `ObsError::BadFilter` when the variable is present but malformed.

---

[WARNING] Malformed TOML column is byte offset, not character offset
File: core/crates/adapters/src/config_loader.rs
Lines: 264-272
Description: `byte_offset_to_line_col` computes `col` as `safe_offset - p`, which is a byte distance. TOML files containing multi-byte UTF-8 characters report incorrect column numbers in `ConfigError::MalformedToml`.
Suggestion: Count `char`s between the last newline and the error offset instead of subtracting byte indices.

---

[WARNING] Runtime panic mapped to startup-precondition exit code
File: core/crates/cli/src/run.rs
Lines: 43-46
Description: When the daemon task panics, `run_with_shutdown` returns `ExitCode::StartupPreconditionFailure` (3). That code is documented for startup preconditions, not runtime panics.
Suggestion: Introduce a dedicated runtime-failure exit code or document the reuse until one is added.

---

[WARNING] Unreachable None arm in env override coercion
File: core/crates/adapters/src/config_loader.rs
Lines: 284
Description: `coerce_value` matches `None`, but `set_by_path` already rejects missing keys before calling it, making the arm dead code.
Suggestion: Remove `| None` and replace it with an `expect` that documents the invariant.

---

[SUGGESTION] `--version` flag and `version` subcommand emit different formats
File: core/crates/cli/src/cli.rs
Lines: 10
Description: Clap's built-in `--version` prints only the crate version, while the `version` subcommand prints `version (git) rustc`. This inconsistency may confuse users.
Suggestion: Override clap's version formatter or remove the built-in flag to make the subcommand canonical.

---

[SUGGESTION] UTC-seconds layer is inert in Pretty log format
File: core/crates/cli/src/observability.rs
Lines: 75-85
Description: `UtcSecondsLayer` is added to the subscriber for `LogFormat::Pretty`, but `TsWriter` is only used for JSON, so `ts_unix_seconds` never appears in pretty output.
Suggestion: Remove the layer from the Pretty path or extend `TsWriter` to support the compact format.

---

[SUGGESTION] Env override errors use debug formatting
File: core/crates/adapters/src/config_loader.rs
Lines: 56
Description: `ConfigError::EnvOverride` formats `reason` with `{:?}`, printing debug output (e.g. `ParseFailed { detail: "..." }`) to users.
Suggestion: Implement `Display` for `EnvOverrideReason` and use `{reason}` in the error string.

---

[SUGGESTION] Signal tests may deadlock on full stdout/stderr pipes
File: core/crates/cli/tests/signals.rs, core/crates/cli/tests/utc_timestamps.rs
Lines: general
Description: Tests spawn the binary with `Stdio::piped()` but never read the pipes until the process exits. If output exceeds the OS pipe buffer, the child blocks indefinitely.
Suggestion: Redirect child output to temporary files or spawn threads to drain the pipes concurrently.

---

[SUGGESTION] Workspace root manifest accumulates crate dependencies
File: core/Cargo.toml
Lines: 16-30
Description: Dependencies such as `tokio`, `tracing-subscriber`, `assert_cmd`, and `insta` are declared in the root `[dependencies]` section instead of `[workspace.dependencies]` or individual crate manifests.
Suggestion: Move crate-specific deps to their respective `Cargo.toml` files and keep the workspace root lean.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-07 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Clap exit code conflicts with documented InvalidInvocation code | ✅ Fixed | `28e7b24` | — | 2026-05-07 | F006 |
| 2 | Tracing subscriber ignores config file [tracing] section | ✅ Fixed | `aea27e8` | — | 2026-05-07 | F002 |
| 3 | Integration tests write to shared global /tmp paths | ✅ Fixed | `1bc3923` | — | 2026-05-07 | F010 |
| 4 | Invalid RUST_LOG is silently ignored | ✅ Fixed | `263af98` | — | 2026-05-07 | F026 |
| 5 | Malformed TOML column is byte offset, not character offset | ✅ Fixed | `263af98` | — | 2026-05-07 | F019 |
| 6 | Runtime panic mapped to startup-precondition exit code | ✅ Fixed | `263af98` | — | 2026-05-07 | F027 |
| 7 | Unreachable None arm in env override coercion | 🚫 Won't Fix | — | — | 2026-05-07 | False positive — None arm IS reachable for Serde-defaulted absent fields (F016) |
| 8 | --version flag and version subcommand emit different formats | ✅ Fixed | `c646fb8` | — | 2026-05-07 | F052 |
| 9 | UTC-seconds layer is inert in Pretty log format | ✅ Fixed | `c646fb8` | — | 2026-05-07 | F041 |
| 10 | Env override errors use debug formatting | ✅ Fixed | `c646fb8` | — | 2026-05-07 | F045 |
| 11 | Signal tests may deadlock on full stdout/stderr pipes | ✅ Fixed | `c646fb8` | — | 2026-05-07 | F046 |
| 12 | Workspace root manifest accumulates crate dependencies | ✅ Fixed | `ac07b47` | — | 2026-05-07 | Merged into F001 — workspace dep cleanup |
