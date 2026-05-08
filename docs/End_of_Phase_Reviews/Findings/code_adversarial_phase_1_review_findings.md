---
id: layer-leak:area::phase-1-code-adversarial-review-findings:legacy-uncategorized
category: layer-leak
kind: process
location:
  area: phase-1-code-adversarial-review-findings
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

# Phase 1 — Code Adversarial Review Findings

**Reviewer:** code_adversarial
**Date:** 2026-05-06T13:05:00Z
**Diff range:** 3734e02^..be773df -- core/ web/
**Phase:** 1

---

[WARNING] Config loaded twice — once before runtime, once inside runtime
File: core/crates/cli/src/main.rs
Lines: 968-976, 991-992
Description: `run_daemon` calls `load_config` to validate config before building the Tokio runtime, discarding the result. Then `run::run_with_shutdown()` will presumably need to load the config again to actually use it. This double-load is a structural seam: if a future slice changes `load_config` to have side effects (e.g., caching, file handles), the two calls will diverge in behaviour. Worse, if env vars change between the two calls (e.g., a config-reload signal arrives), the pre-runtime validation becomes a false promise.
Technique: Composition Failure
Trigger: Any future change that adds side effects to `load_config`, or an env var race between the two calls.
Suggestion: Load config once, pass the validated `DaemonConfig` into `run_with_shutdown(config)` so the runtime receives the exact same struct that was validated.

---

[HIGH] `ShutdownSignal::wait()` loops forever on spurious wakes if sender never triggers
File: core/crates/cli/src/shutdown.rs
Lines: 1455-1469
Description: The `wait()` method loops on `rx.changed().await`. If the sender is never dropped and never sends `true`, a task calling `wait()` will block forever. This is correct for the intended shutdown flow, but the `daemon_loop` in `run.rs` awaits this signal and nothing else. If the signal handler task panics before triggering shutdown, `daemon_loop` will never terminate and the 10-second drain budget will always be exhausted, producing a forced-shutdown warning on every run.
Technique: Assumption Violation
Trigger: `wait_for_signal()` returns an error (e.g., OS refuses handler install), causing `run_with_shutdown` to return early before triggering the controller; or the signal-handling task panics.
Suggestion: Add a timeout or a secondary cancellation path inside `daemon_loop` so it does not have a single point of failure. Alternatively, ensure `run_with_shutdown` triggers the controller even on the error path from `wait_for_signal`.

---

[WARNING] `coerce_value` silently falls back to string when `existing` is `None`
File: core/crates/adapters/src/config_loader.rs
Lines: 346-347
Description: In `coerce_value`, when `existing` is `None`, the raw value is kept as a string. This happens when `set_by_path` is called on a leaf key that exists in the table but whose value is somehow absent (the `contains_key` check passed, but `get` returns `None` — impossible with the current `toml` crate, but a latent assumption). More importantly, this fallback means that if an env override targets a table key that was removed by a prior override, the coercion silently degrades to string instead of failing.
Technique: Assumption Violation
Trigger: A caller chains multiple env overrides that mutate the same nested path, causing an intermediate `None` during traversal.
Suggestion: Remove the `| None` arm in `coerce_value`; if `existing` is `None` after `contains_key` returned true, treat it as an internal invariant violation and return `ParseFailed`.

---

[WARNING] `TsWriterGuard::drop` swallows I/O errors silently
File: core/crates/cli/src/observability.rs
Lines: 1210-1218
Description: The `Drop` impl for `TsWriterGuard` uses `let _ =` on both `write_all` and `flush`. If the underlying writer is a pipe or socket that has closed (e.g., stderr redirected to a broken pipe), the final partial JSON line is silently lost. In a log pipeline this means the last event before crash/exit may vanish, complicating post-mortem analysis.
Technique: Cascade
Trigger: stderr is a pipe that closes before the process exits (e.g., `trilithon | head -n 1`).
Suggestion: At minimum, document the silent-drop behaviour. Ideally, use a `flush_on_drop` flag set by `flush()` so that `drop` only writes if `flush` was never called, reducing the chance of double-write races and making the error path explicit.

---

[SUGGESTION] `resolve_format` accepts any non-empty env var as "json"
File: core/crates/cli/src/observability.rs
Lines: 1108-1114
Description: `TRILITHON_LOG_FORMAT=json` is case-insensitive, but any other value (e.g., `TRILITHON_LOG_FORMAT=prettypretty`) silently falls back to `Pretty`. This is a legitimate caller setting an invalid value and receiving no feedback.
Technique: Abuse Case
Trigger: Operator typo in the env var (e.g., `jsonn`, `jsob`) goes unnoticed because the daemon starts successfully in Pretty mode.
Suggestion: Log a warning when `TRILITHON_LOG_FORMAT` is set to an unrecognized value, or tighten the check to only accept exact `json`/`pretty` strings and warn otherwise.

---

[SUGGESTION] `StdEnvProvider` silently drops non-Unicode env var values in `vars_with_prefix`
File: core/crates/adapters/src/env_provider.rs
Lines: 404-408
Description: `vars_with_prefix` filters via `std::env::vars()`, which yields `String` (already Unicode). However, on some platforms `std::env::vars()` silently skips non-Unicode variables, while `var()` returns `VarError::NotUnicode`. A caller using `vars_with_prefix` will never see `NotUnicode` errors for prefixed vars; they simply disappear. This creates an inconsistency between the two methods on the same trait.
Technique: Composition Failure
Trigger: A non-Unicode env var with a `TRILITHON_` prefix exists on the system; `vars_with_prefix` omits it, but `var("TRILITHON_X")` would return `NotUnicode`.
Suggestion: Document the discrepancy or switch `vars_with_prefix` to use `std::env::vars_os()` and surface non-Unicode entries as `EnvError::NotUnicode` so the caller can decide.

---

[SUGGESTION] `config_show::run` uses `anyhow::Error` for a bounded error surface
File: core/crates/cli/src/config_show.rs
Lines: 849-857
Description: `run_inner` returns `anyhow::Error`, but the only two fallible operations are `load_config` (which has a rich `ConfigError` enum) and `toml::to_string_pretty` (which can only fail if the redacted struct contains invalid types, an invariant violation). Using `anyhow` here erases the structured error that `load_config` worked hard to produce, and the outer `run` function only distinguishes "ok" vs "any error" anyway.
Technique: Cascade
Trigger: A future slice wants to emit different exit codes for different config errors (e.g., `Missing` vs `MalformedToml`); the `anyhow` wrapping forces a rewrite.
Suggestion: Change `run_inner` to return `Result<(), ConfigError>` and map `toml::to_string_pretty` failure into `ConfigError::InternalSerialise`. Preserve the typed error through the CLI boundary.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-07 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Config loaded twice — TOCTOU gap in run_daemon | 🚫 Won't Fix | — | — | 2026-05-07 | False positive — config IS passed to run_with_shutdown (F014) |
| 2 | ShutdownSignal::wait() loops forever if sender never triggers | ✅ Fixed | `06c9c22` | — | 2026-05-07 | F009 |
| 3 | coerce_value silently falls back to string when existing is None | 🚫 Won't Fix | — | — | 2026-05-07 | False positive — None arm IS reachable for Serde-defaulted absent fields (F016) |
| 4 | TsWriterGuard::drop swallows I/O errors silently | ✅ Fixed | `263af98` | — | 2026-05-07 | F022 |
| 5 | resolve_format accepts any non-empty env var as "json" | ✅ Fixed | `c646fb8` | — | 2026-05-07 | F038 |
| 6 | StdEnvProvider silently drops non-Unicode env var values | ✅ Fixed | `c646fb8` | — | 2026-05-07 | F039 |
| 7 | config_show::run_inner uses anyhow::Error, erasing ConfigError | ✅ Fixed | `c646fb8` | — | 2026-05-07 | F040 |
