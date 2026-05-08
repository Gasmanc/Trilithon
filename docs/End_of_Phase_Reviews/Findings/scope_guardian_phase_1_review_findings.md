---
id: security:area::phase-1-scope-guardian-review-findings:legacy-uncategorized
category: security
kind: process
location:
  area: phase-1-scope-guardian-review-findings
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

# Phase 1 — Scope Guardian Review Findings

**Reviewer:** scope_guardian
**Date:** 2026-05-06T13:02:00Z
**Diff range:** 3734e02^..be773df -- core/ web/
**Phase:** 1

---

```
[CRITICAL] Workspace-level Cargo.toml polluted with I/O dependencies
File: /Users/carter/Coding/Trilithon/core/Cargo.toml
Lines: 9-25
Description: The workspace root `core/Cargo.toml` adds `tokio` (with signal/sync/time), `tracing-subscriber` (json/fmt), `time`, `assert_cmd`, `predicates`, `nix`, `regex`, `toml`, `insta` — all I/O, async, or test-helper crates. Per architecture §4.1 and §5, `core/` must not declare I/O or async dependencies; these belong in `crates/cli/Cargo.toml` and `crates/adapters/Cargo.toml`. The workspace manifest is effectively a shared dependency list; polluting it with I/O crates makes it impossible to enforce the three-layer boundary by manifest review.
Question: Scope
TODO unit: 1.2 — `DaemonConfig` typed records in `core`
Suggestion: Remove all I/O/async/test dependencies from `core/Cargo.toml`. Add them only to the `cli` and `adapters` crate manifests where they are consumed. Keep `core/Cargo.toml` to `serde`, `thiserror`, `url` only.

---

[CRITICAL] `core` crate manifest gains I/O dependencies
File: /Users/carter/Coding/Trilithon/core/crates/core/Cargo.toml
Lines: 7-11
Description: `core/Cargo.toml` adds `url` (acceptable) but also pulls in `serde_json` and `toml` as dev-dependencies. While dev-dependencies are less severe, `toml` is a parser crate and `serde_json` is serialization; more importantly, the workspace-level `core/Cargo.toml` (root) now forces every workspace member to resolve against `tokio`, `tracing-subscriber`, `nix`, etc. The architectural rule is enforced by manifest dependencies; this violates the spirit of ADR-0003.
Question: Scope
TODO unit: 1.2 — `DaemonConfig` typed records in `core`
Suggestion: Strip `core/Cargo.toml` (root) back to workspace metadata only. Move all dependency declarations to member crates.

---

[HIGH] `web/` frontend changes are entirely out of scope
File: /Users/carter/Coding/Trilithon/web/
Lines: general
Description: The diff includes `web/package.json` rename, `web/pnpm-lock.yaml` (4400 lines), `web/src/App.test.tsx`, `web/vite.config.ts`, `web/vitest.config.ts`, and `web/vitest.setup.ts`. None of the Phase 1 TODO slices mention the frontend web layer. Phase 1 is exclusively "Daemon skeleton and configuration" — Rust CLI, core, adapters. The web changes appear to be scaffolding from a project bootstrap that leaked into this phase.
Question: Scope
TODO unit: general — no slice references web
Suggestion: Revert all `web/` changes from this phase. Web frontend work should be scheduled in a later phase (e.g., Phase 3 or 4) with its own TODO.

---

[HIGH] Missing `BindAddressInvalid` error variant
File: /Users/carter/Coding/Trilithon/core/crates/adapters/src/config_loader.rs
Lines: 117-182
Description: The TODO slice 1.3 specifies `ConfigError::BindAddressInvalid { value: String }`. The implemented `ConfigError` enum omits this variant entirely. The algorithm step 5 says "Validate the bind address... mistakes surface during deserialise as `ConfigError::BindAddressInvalid`". The diff instead relies on TOML deserialization to fail with `MalformedToml` for bad bind addresses.
Question: Scope
TODO unit: 1.3 — `EnvProvider` trait, TOML loader, `ConfigError`
Suggestion: Add `BindAddressInvalid { value: String }` to `ConfigError` and map `SocketAddr` parse failures to it during the env-override or raw-config stage.

---

[HIGH] `config show` does not exit code 2 on `ConfigError`
File: /Users/carter/Coding/Trilithon/core/crates/cli/src/config_show.rs
Lines: 836-857
Description: Slice 1.6 algorithm says: "On `ConfigError`, exit `2`." The `config_show::run` function returns `ExitCode::ConfigError` on error, which is correct, but the `dispatch` in `main.rs` simply returns this code. However, the slice 1.6 spec shows `run_config_show` returning `Result<i32, anyhow::Error>` and the caller exiting 2. The current implementation returns `ExitCode` directly. More importantly, slice 1.7's `missing_config_exits_2` test is for `run`, not `config show`. The TODO explicitly says `trilithon run` with missing config exits 2, but also implies `config show` should handle config errors correctly.
Question: Coherence
TODO unit: 1.6 — `config show` with redaction
Suggestion: Ensure `config show` propagates `ConfigError` as exit code 2. The current code appears to do this via `ExitCode::ConfigError`, but verify the integration test covers this path.

---

[WARNING] `nix` version mismatch
File: /Users/carter/Coding/Trilithon/core/Cargo.toml
Lines: 20
Description: Slice 1.7 specifies `nix = { version = "0.27", features = ["signal"] }` as dev-dependency. The diff uses `nix = { version = "0.29", features = ["signal", "process", "fs", "user"] }` in the workspace root. Version bump and extra features are scope creep.
Question: Scope
TODO unit: 1.7 — End-to-end exit-code and signal integration tests
Suggestion: Pin `nix` to `0.27` with `features = ["signal"]` per the TODO, and declare it only in `crates/cli/Cargo.toml` as a dev-dependency.

---

[WARNING] `ReadFailed` variant not in spec
File: /Users/carter/Coding/Trilithon/core/crates/adapters/src/config_loader.rs
Lines: 126-133
Description: The TODO slice 1.3 `ConfigError` enum does not include `ReadFailed`. It only lists `Missing`, `MalformedToml`, `EnvOverride`, `DataDirNotWritable`, `BindAddressInvalid`, and `RebaseTtlOutOfBounds`. The diff adds `ReadFailed` for non-NotFound I/O errors. While defensible, it is not what the TODO says.
Question: Scope
TODO unit: 1.3 — `EnvProvider` trait, TOML loader, `ConfigError`
Suggestion: Either remove `ReadFailed` and map all non-NotFound read errors to `Missing` (as the TODO implies), or document the deviation. Prefer alignment with the spec.

---

[WARNING] `InternalSerialise` variant not in spec
File: /Users/carter/Coding/Trilithon/core/crates/adapters/src/config_loader.rs
Lines: 173-181
Description: The TODO slice 1.3 does not list `InternalSerialise` in `ConfigError`. The diff adds it for the re-serialize-to-table step. This is an implementation detail leaking into the public error type.
Question: Scope
TODO unit: 1.3 — `EnvProvider` trait, TOML loader, `ConfigError`
Suggestion: Remove `InternalSerialise`. If re-serialization fails, panic with a message (it's an invariant violation) or map to a generic internal error. Do not expand the public error surface beyond the spec.

---

[WARNING] `pretty_and_json_dispatch` test missing
File: /Users/carter/Coding/Trilithon/core/crates/cli/src/observability.rs
Lines: 1240-1350
Description: Slice 1.4 requires a test `pretty_and_json_dispatch` that calls `init` twice in subprocesses with `LogFormat::Pretty` and `LogFormat::Json`, asserting JSON output on the second run and not on the first. The diff only contains `utc_seconds_field_present`, `init_ok_then_already_installed`, and `inject_ts_unix_seconds_prepends_field`. The subprocess dispatch test is absent.
Question: Scope
TODO unit: 1.4 — Tracing subscriber, pre-tracing line, UTC-seconds layer
Suggestion: Add the `pretty_and_json_dispatch` subprocess test as specified.

---

[WARNING] `ObsError` lacks `BadFilter` test coverage
File: /Users/carter/Coding/Trilithon/core/crates/cli/src/observability.rs
Lines: 1033-1045
Description: Slice 1.4's `ObsError` includes `BadFilter`, but there is no unit test asserting that an invalid filter string returns `ObsError::BadFilter`. The `init_ok_then_already_installed` test only covers `AlreadyInstalled`.
Question: Scope
TODO unit: 1.4 — Tracing subscriber, pre-tracing line, UTC-seconds layer
Suggestion: Add a test passing an invalid filter directive (e.g., ":::invalid") and asserting `BadFilter` is returned.

---

[SUGGESTION] `UtcSecondsLayer` only stores timestamp in thread-local
File: /Users/carter/Coding/Trilithon/core/crates/cli/src/observability.rs
Lines: 1127-1144
Description: The `UtcSecondsLayer` stores `ts_unix_seconds` in a thread-local but does not inject it into the event's structured fields for the pretty formatter. Only the JSON path gets the field via `TsWriter`. The TODO says "a custom layer that stamps every event with a `ts_unix_seconds` integer field." The pretty format path does not carry this field in the event metadata; it only appears in the side-channel thread-local.
Question: Coherence
TODO unit: 1.4 — Tracing subscriber, pre-tracing line, UTC-seconds layer
Suggestion: For coherence with the architecture's observability requirements (§12, H6), ensure both JSON and pretty outputs carry the timestamp. The current JSON injection via `TsWriter` is clever but the pretty path should also include the field, perhaps via a custom `FormatEvent` or by adding the field to the event's extensions.

---

[SUGGESTION] `ShutdownController::signal` is dead code
File: /Users/carter/Coding/Trilithon/core/crates/cli/src/shutdown.rs
Lines: 1496-1501
Description: `ShutdownController::signal` is marked `#[expect(dead_code)]` with a note that callers are added in later slices. This is a forward-looking stub. The TODO slice 1.5 does not mention this method being required now; it only requires `new()`, `trigger()`, and `wait()`. Keeping unused code with suppressions is minor scope creep.
Question: Scope
TODO unit: 1.5 — Signal handling and graceful shutdown
Suggestion: Remove `signal()` until a later slice needs it, or keep it but do not add dead-code suppressions in committed code.

---

[SUGGESTION] `run_with_shutdown` returns `StartupPreconditionFailure` on join error
File: /Users/carter/Coding/Trilithon/core/crates/cli/src/run.rs
Lines: 1397-1404
Description: On task panic (`Ok(Err(join_err))`), the code returns `ExitCode::StartupPreconditionFailure`. The TODO slice 1.5 does not specify this behavior; it only says the daemon exits 0 within budget. Returning 3 on panic is a reasonable choice but not spec'd.
Question: Scope
TODO unit: 1.5 — Signal handling and graceful shutdown
Suggestion: Document the deviation or return `CleanShutdown` if the spec intends all signal-driven exits to be 0. The current behavior is acceptable but off-pattern.
```

**Summary of findings:**

The diff is **too large** primarily because:
1. The entire `web/` frontend scaffold (package rename, lockfile, test config, Vitest setup) is included despite Phase 1 being daemon-only.
2. The workspace root `core/Cargo.toml` is bloated with I/O and async dependencies that violate the three-layer architecture.

The diff is **partially coherent** because:
1. The `core` crate's manifest boundary is violated by workspace-level I/O deps.
2. `ConfigError` is missing `BindAddressInvalid` and adds non-spec variants (`ReadFailed`, `InternalSerialise`).
3. The `UtcSecondsLayer` only injects `ts_unix_seconds` into JSON output, not pretty output.
4. A required test (`pretty_and_json_dispatch`) is missing.
5. `config show` error handling is slightly off-pattern from the spec's `Result<i32, anyhow::Error>` signature.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-07 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Workspace-level Cargo.toml polluted with I/O dependencies | ✅ Fixed | `ac07b47` | — | 2026-05-07 | F001 |
| 2 | core crate manifest gains I/O dependencies | ✅ Fixed | `b8fdbd0` | — | 2026-05-07 | F003 |
| 3 | web/ frontend changes are entirely out of scope | ✅ Fixed | `06c9c22` | — | 2026-05-07 | F007 — acknowledged as bootstrap scaffolding; CLAUDE.md updated |
| 4 | Missing BindAddressInvalid error variant | ✅ Fixed | `06c9c22` | — | 2026-05-07 | F008 |
| 5 | config show does not exit code 2 on ConfigError | ✅ Fixed | `06c9c22` | — | 2026-05-07 | F012 — integration test added |
| 6 | nix version mismatch | ✅ Fixed | `ac07b47` | — | 2026-05-07 | F032 |
| 7 | ReadFailed variant not in spec | ✅ Fixed | `263af98` | — | 2026-05-07 | F030 — documented as intentional improvement |
| 8 | InternalSerialise variant not in spec | 🚫 Won't Fix | — | — | 2026-05-07 | False positive — variant does not exist in the codebase (F031) |
| 9 | pretty_and_json_dispatch test missing | ✅ Fixed | `263af98` | — | 2026-05-07 | F028 |
| 10 | ObsError lacks BadFilter test coverage | ✅ Fixed | `263af98` | — | 2026-05-07 | F029 |
| 11 | UtcSecondsLayer only stores timestamp in thread-local (JSON-only) | ✅ Fixed | `c646fb8` | — | 2026-05-07 | F041 — removed from Pretty path; JSON-only behavior documented |
| 12 | ShutdownController::signal is dead code | ✅ Fixed | `c646fb8` | — | 2026-05-07 | F047 — removed dead method |
| 13 | run_with_shutdown returns StartupPreconditionFailure on join error | ✅ Fixed | `263af98` | — | 2026-05-07 | F027 — dedicated RuntimePanic exit code added |
