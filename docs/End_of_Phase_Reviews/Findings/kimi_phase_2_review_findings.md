# Phase 2 — Kimi Review Findings

**Reviewer:** kimi
**Date:** 2026-05-07
**Diff range:** HEAD
**Phase:** 2

---

[WARNING] Daemon CLI no longer exposes `--version`
File: core/crates/cli/src/cli.rs, core/crates/cli/src/main.rs
Lines: general
Description: `cli.rs` removes `version` from the `#[command(...)]` derive and adds `disable_version_flag = true`. `main.rs` deletes the `ErrorKind::DisplayVersion` arm, so any future attempt to invoke `--version` falls through to the usage-error path and exits 64. No custom `--version` argument or subcommand replaces it, yet `build.rs` still compiles in `TRILITHON_GIT_SHORT_HASH`.
Suggestion: Re-enable `#[command(version)]` so the standard `-V` / `--version` flag works, or add a dedicated `--version` argument that prints the version plus git hash.

[WARNING] Spec-required shutdown APIs removed despite planned later use
File: core/crates/cli/src/shutdown.rs
Lines: general
Description: `ShutdownSignal::is_shutting_down()` and `ShutdownController::signal()` are deleted. Both functions were explicitly annotated `#[expect(dead_code, reason = "spec-required API, callers added in later slices")]`, indicating they were reserved by design for future slices. Phase 2.7 involves wiring startup and integration tests that may need to poll shutdown state or hand signals to late-spawned tasks.
Suggestion: Retain these two APIs if the spec still requires them; otherwise update the architecture docs to reflect that the shutdown API surface has been deliberately narrowed.

[SUGGESTION] Trailing dot in env key produces cryptic error message
File: core/crates/adapters/src/config_loader.rs
Lines: ~277
Description: The empty-segment guard catches `..` and leading dots, but a trailing dot such as `TRILITHON_SERVER_BIND_` (which becomes `server.bind.`) is only caught one recursion level later when `dotted_key` is `""`. The resulting error says `key contains an empty segment: ""` instead of the original key.
Suggestion: Add `dotted_key.ends_with('.')` to the guard so the error message preserves the full offending key.

[SUGGESTION] Non-Unicode `TRILITHON_*` env vars silently ignored
File: core/crates/adapters/src/env_provider.rs
Lines: ~16
Description: The new comment correctly notes that `vars()` silently skips non-Unicode entries, so a non-UTF-8 `TRILITHON_*` variable is dropped without feedback. A user who sets such a variable may never realise the override is being ignored.
Suggestion: During config loading, scan `std::env::vars_os()` for `TRILITHON_*` keys that fail UTF-8 validation and emit a `tracing::warn!` so the omission is visible.

[SUGGESTION] User-facing log-format warning uses debug quoting
File: core/crates/cli/src/observability.rs
Lines: ~125
Description: The unknown-format warning writes `TRILITHON_LOG_FORMAT={v:?}`, which renders the value with quotes and escapes (e.g., `"json"` instead of `json`).
Suggestion: Use `{v}` instead of `{v:?}` in the `writeln!` call so the warning reads naturally.
