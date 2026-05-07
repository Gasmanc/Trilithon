# Phase 1 — Aggregate Review Plan

**Generated:** 2026-05-07T00:00:00Z
**Reviewers:** code_adversarial, codex, gemini, glm, kimi, learnings_match, minimax, qwen, scope_guardian, security
**Raw findings:** 87 across 10 reviewers (security reported clean)
**Unique findings:** 59 after clustering
**Consensus:** 1 unanimous · 8 majority · 50 single-reviewer
**Conflicts:** 0
**Superseded (already fixed):** 0

---

## How to use this document

Feed this file to `/review-remediate` to drive the fix cycle. Each finding has a
unique ID (F001, F002, …) that `review-remediate` uses to track state. Do not
renumber or delete findings — append `SUPERSEDED` status instead.

---

## Reviewer roster

| Reviewer | Size | CRITICAL | HIGH | WARNING | SUGGESTION |
|---|---|---|---|---|---|
| code_adversarial | 6.6 KB | 0 | 1 | 3 | 3 |
| codex | 11.3 KB | 0 | 2 | 7 | 8 |
| gemini | 4.5 KB | 1 | 1 | 2 | 2 |
| glm | 3.9 KB | 0 | 1 | 2 | 2 |
| kimi | 5.2 KB | 0 | 3 | 4 | 5 |
| learnings_match | 3.1 KB | 0 | 0 | 5 | 0 |
| minimax | 2.8 KB | 0 | 0 | 4 | 2 |
| qwen | 7.6 KB | 0 | 2 | 3 | 8 |
| scope_guardian | 10.8 KB | 2 | 3 | 7 | 4 |
| security | 0.2 KB | 0 | 0 | 0 | 0 |
| **Total raw** | | **3** | **13** | **37** | **34** |

---

## CRITICAL Findings

### F001 · [CRITICAL] Workspace root `core/Cargo.toml` polluted with I/O, async, and test dependencies
**Consensus:** UNANIMOUS · flagged by: scope_guardian[CRITICAL], codex[HIGH], kimi[SUGGESTION], qwen[WARNING×2]
**File:** `core/Cargo.toml` · **Lines:** 9–29
**Description:** The workspace-level `core/Cargo.toml` declares `tokio` (signal/sync/time), `tracing-subscriber` (json/fmt), `time`, `url`, `assert_cmd`, `predicates`, `nix`, `regex`, `toml`, and `insta` as workspace dependencies. Per the three-layer architecture, the `core/` workspace must not declare I/O, async-runtime, or test-helper dependencies. This makes it impossible to enforce the three-layer boundary by manifest review and risks accidental import of I/O crates in the `core` layer. Test-only crates (`assert_cmd`, `predicates`, `insta`, `regex`) also belong in `[dev-dependencies]`, not `[dependencies]`.
**Suggestion:** Strip `core/Cargo.toml` to workspace metadata + pure-logic crates (`serde`, `thiserror`) only. Move `tokio`, `tracing-subscriber`, `nix`, `time`, `toml`, `url` to `cli`/`adapters` crate manifests. Move `assert_cmd`, `predicates`, `regex`, `insta` to per-crate `[dev-dependencies]`.
**Claude's assessment:** Agree strongly. The workspace manifest is the first line of defence for architectural enforcement; polluting it defeats that entirely. This should be the first item fixed.

---

### F002 · [CRITICAL] Tracing subscriber initialized with hardcoded defaults before config file is loaded
**Consensus:** MAJORITY · flagged by: gemini[CRITICAL], kimi[HIGH]
**File:** `core/crates/cli/src/main.rs` · **Lines:** 36–50
**Description:** `main()` installs the global tracing subscriber with hardcoded config (`"info,trilithon=info"`, Pretty format) before CLI dispatch. When `run_daemon` later loads `config.toml`, calling `observability::init` a second time returns `ObsError::AlreadyInstalled` (which is silently ignored). This means `[tracing].log_filter` and `[tracing].format` from `config.toml` are silently ignored, violating slice 1.4 of the Phase 1 spec.
**Suggestion:** Move `observability::init` into `run_daemon` (and `config_show::run`) after `DaemonConfig` is successfully loaded from disk/env.
**Claude's assessment:** Agree. This is a spec violation. The hardcoded pre-tracing bootstrap is only appropriate until config is loaded; currently the real init never happens.

---

### F003 · [CRITICAL] `core` crate (`core/crates/core`) gains I/O-adjacent dependency (`url`)
**Consensus:** MAJORITY · flagged by: codex[HIGH], scope_guardian[CRITICAL-partial]
**File:** `core/crates/core/Cargo.toml` · **Lines:** 7–11
**Description:** `core/crates/core/Cargo.toml` adds `url = { workspace = true }`. The `url` crate is an I/O-adjacent network URL parser. Per the architecture, `core` must contain pure logic with no I/O, network, or FFI dependencies. This also adds `serde_json` and `toml` as dev-dependencies in the core crate, which are serialization/parsing crates.
**Suggestion:** Remove `url` from `core/crates/core/Cargo.toml`. Replace `Url` in `CaddyEndpoint::LoopbackTls` with a plain `String` or a thin newtype wrapper (e.g., `struct AdminUrl(String)`); validate URL format in the `adapters` layer.
**Claude's assessment:** Agree. `url` is not pure logic — it validates network address syntax. This should be deferred to the `adapters` layer per ADR-0003.

---

## HIGH Findings

### F004 · [HIGH] Background tasks not awaited during graceful shutdown
**Consensus:** SINGLE · flagged by: gemini
**File:** `core/crates/cli/src/run.rs` · **Lines:** 104–108, 146–154, 182–195
**Description:** `run_with_shutdown` spawns `integrity_loop` and `reconnect_loop` via `tokio::spawn` but only awaits `daemon_loop` during the shutdown phase. When `run_with_shutdown` returns after the `DRAIN_BUDGET` timeout, the Tokio runtime is dropped in `main.rs`, abruptly killing all other tasks at their last yield point. This may interrupt database writes or leave inconsistent state.
**Suggestion:** Use `tokio::task::JoinSet` or collect handles and `join_all` (with timeout) to drain all background tasks before dropping the runtime.
**Claude's assessment:** Agree. Phase 1's `daemon_loop` is a no-op so this doesn't bite yet, but this is a structural defect that will cause data loss once real background work lands.

---

### F005 · [HIGH] Build script `rerun-if-changed` paths resolve to non-existent files
**Consensus:** SINGLE · flagged by: glm
**File:** `core/crates/cli/build.rs` · **Lines:** 4–5
**Description:** `cargo:rerun-if-changed=.git/HEAD` is relative to the package directory (`core/crates/cli/`), resolving to `core/crates/cli/.git/HEAD` which does not exist. Cargo never re-runs the build script on git HEAD changes, so `TRILITHON_GIT_SHORT_HASH` becomes stale after commits that don't touch `core/crates/cli/` files.
**Suggestion:** Use `PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.git/HEAD")` or resolve the git root at build time via `git rev-parse --git-dir`.
**Claude's assessment:** Agree. This is a latent bug that causes stale version hashes in release builds. Easy fix with a real impact.

---

### F006 · [HIGH] `Cli::parse()` exit code 2 conflicts with `ExitCode::InvalidInvocation` (64)
**Consensus:** MAJORITY · flagged by: kimi[HIGH], gemini[SUGGESTION]
**File:** `core/crates/cli/src/main.rs` · **Lines:** 46
**Description:** `Cli::parse()` exits with code 2 on usage errors, but the project defines `ExitCode::ConfigError = 2` and `ExitCode::InvalidInvocation = 64` (matching `EX_USAGE`). This makes clap usage errors (wrong flags) indistinguishable from configuration errors, breaking the documented exit-code contract.
**Suggestion:** Use `Cli::try_parse()`, print clap's error to stderr, and return `ExitCode::InvalidInvocation` (64) for usage errors. Return 0 for `--help`/`--version`.
**Claude's assessment:** Agree. Exit code contracts are part of the public API for daemon software. The spec defines 64 for usage errors and this should be honoured.

---

### F007 · [HIGH] `web/` frontend scaffold entirely out of scope for Phase 1
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `web/` · **Lines:** general
**Description:** The diff includes `web/package.json` rename, `web/pnpm-lock.yaml` (4 400+ lines), `web/src/App.test.tsx`, `web/vite.config.ts`, `web/vitest.config.ts`, and `web/vitest.setup.ts`. No Phase 1 TODO slice references the frontend web layer. Phase 1 is daemon skeleton and configuration (Rust-only).
**Suggestion:** Either revert `web/` changes from this phase, or explicitly acknowledge them as bootstrap scaffolding and schedule a formal web phase. Keeping them inflates the diff and makes future diffs harder to review.
**Claude's assessment:** Agree with the scope finding. The web scaffold appears to be project-bootstrap leakage. Whether to revert or simply note is a product decision, but the code review burden of a 4 400-line lockfile is real.

---

### F008 · [HIGH] `ConfigError::BindAddressInvalid` variant missing from implementation
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/adapters/src/config_loader.rs` · **Lines:** 117–182
**Description:** Slice 1.3 specifies `ConfigError::BindAddressInvalid { value: String }` for invalid bind-address values. The implemented `ConfigError` enum omits this variant entirely. Bad bind addresses fall through to `MalformedToml`, which is semantically wrong and produces misleading diagnostics.
**Suggestion:** Add `BindAddressInvalid { value: String }` to `ConfigError` and map `SocketAddr` parse failures to it during env-override or deserialization.
**Claude's assessment:** Agree. Spec-required variant. Missing it breaks the error-type contract that callers (and tests) depend on.

---

### F009 · [HIGH] `ShutdownSignal::wait()` loops forever if signal handler panics before triggering
**Consensus:** SINGLE · flagged by: code_adversarial
**File:** `core/crates/cli/src/shutdown.rs` · **Lines:** ~1455–1469
**Description:** `daemon_loop` awaits `ShutdownSignal::wait()` as its only exit path. If `wait_for_signal()` returns an error (OS refuses handler installation) or the signal-handling task panics before triggering the controller, `daemon_loop` blocks indefinitely and the 10-second drain budget is always exhausted, producing a forced-shutdown warning on every clean run.
**Suggestion:** Add a timeout or secondary cancellation path in `daemon_loop`. Alternatively, ensure `run_with_shutdown` triggers the controller on all error paths from `wait_for_signal`.
**Claude's assessment:** Agree. This is a single-point-of-failure in the shutdown path. Worth addressing before adding real workloads.

---

### F010 · [HIGH] Test fixtures use hardcoded `/tmp` paths, causing inter-test races
**Consensus:** MAJORITY · flagged by: kimi[HIGH], glm[SUGGESTION]
**File:** `core/crates/adapters/tests/fixtures/minimal.toml`, `core/crates/cli/tests/fixtures/minimal.toml` · **Lines:** general
**Description:** Fixtures hard-code `data_dir = "/tmp/trilithon-test-data"` and `"/tmp/trilithon-cs-test"`. `load_config` creates probe files there, so concurrent test runs (e.g., multiple `cargo test` targets) race on directory creation and probe-file writes. The `data_dir_not_writable` test correctly uses `tempfile::tempdir()`; the happy-path tests do not.
**Suggestion:** Generate unique temporary directories per test via `tempfile` and inject via `TRILITHON_STORAGE__DATA_DIR` env override or a programmatically generated fixture file.
**Claude's assessment:** Agree. Shared `/tmp` paths in tests are a classic source of flaky CI failures, especially under parallel execution.

---

### F011 · [HIGH] `config_show.rs` writes errors to stderr with raw `writeln!` after tracing is active
**Consensus:** SINGLE · flagged by: qwen
**File:** `core/crates/cli/src/config_show.rs` · **Lines:** 20–22
**Description:** `run()` writes errors to stderr via `writeln!(stderr, "trilithon: config error: {e}")` after the tracing subscriber is already installed, creating a format inconsistency: users in JSON log mode see a raw unstructured line mixed into structured JSON output.
**Suggestion:** Replace with `tracing::error!(error = %e, "config.show.failed")` for consistent structured output.
**Claude's assessment:** Agree with the inconsistency point. However, this is a cosmetic issue — the error is still surfaced. Medium priority.

---

### F012 · [HIGH] `config show` exit-code-2 path — verify integration test coverage
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/cli/src/config_show.rs` · **Lines:** 836–857
**Description:** Slice 1.6 specifies "On `ConfigError`, exit `2`." The implementation returns `ExitCode::ConfigError` which appears correct, but scope_guardian notes the spec shows `Result<i32, anyhow::Error>` and the test for exit-2 covers `run` not `config show`. Integration test coverage for this error path may be absent.
**Suggestion:** Verify (or add) an integration test for `config show` with a missing/malformed config file asserting exit code 2 and appropriate stderr message.
**Claude's assessment:** This may be a false positive — the implementation looks correct. The primary action is adding missing test coverage (see also F044).

---

### F013 · [HIGH] Pre-tracing line always written even for fast-exit commands (`version`, `help`)
**Consensus:** MAJORITY · flagged by: qwen[HIGH], gemini[SUGGESTION]
**File:** `core/crates/cli/src/main.rs` · **Lines:** 39–44
**Description:** `"trilithon: starting (pre-tracing)"` is written to stderr unconditionally before argument parsing. For `version` — which exits immediately — this is unexpected noise. The `pre_tracing_line` integration test explicitly asserts this behavior, baking it in.
**Suggestion:** Move the pre-tracing write after `Cli::parse()` so it only fires for daemon/work paths, or explicitly document the behavior as intentional per spec and update the test name accordingly.
**Claude's assessment:** Agree with qwen's framing. Writing startup noise before even parsing args is non-standard CLI behavior. This is a spec question: if the spec intends it, document it; otherwise, move it.

---

## WARNING Findings

### F014 · [WARNING] Config double-loaded and discarded in `run_daemon` (TOCTOU gap)
**Consensus:** MAJORITY · flagged by: code_adversarial, glm, qwen
**File:** `core/crates/cli/src/main.rs` · **Lines:** 58–103
**Description:** `run_daemon` calls `load_config` for validation, discards the result, and then `run_with_shutdown` presumably loads it again. The double-load is wasteful and introduces a TOCTOU gap between validation and use. If a config-reload signal arrives between the two calls, the pre-runtime validation becomes a false promise.
**Suggestion:** Pass the loaded `DaemonConfig` into `run_with_shutdown(config)` so validation and use are a single operation.
**Claude's assessment:** Agree. Acceptable as-is for Phase 1 since the daemon loop is a no-op, but this must be fixed before real config-dependent code lands.

---

### F015 · [WARNING] Signal tests use fixed 1-second sleep instead of readiness probe
**Consensus:** MAJORITY · flagged by: codex, minimax
**File:** `core/crates/cli/tests/signals.rs` · **Lines:** ~79–80, ~1770–1774
**Description:** Tests sleep 1 second waiting for the daemon to install signal handlers before sending `SIGTERM`/`SIGINT`. On slow CI runners or under heavy load, the daemon may not be ready, causing the OS default to handle the signal (exit 130/143) rather than the graceful shutdown path. This is a latent flakiness source.
**Suggestion:** Replace the fixed sleep with a readiness probe — poll stderr for `"daemon.started"` or a specific structured log line with a timeout.
**Claude's assessment:** Agree. Fixed sleeps in signal tests are a classic CI flakiness trap. A readiness probe is the correct fix.

---

### F016 · [WARNING] `coerce_value` `| None` arm is unreachable dead code
**Consensus:** MAJORITY · flagged by: code_adversarial, kimi
**File:** `core/crates/adapters/src/config_loader.rs` · **Lines:** 284, 346–347
**Description:** The `| None` arm in `coerce_value` is dead code — `set_by_path` already rejects missing keys before calling it, so `existing` is never `None`. The arm silently degrades to returning a `String` type, which would mask invariant violations if the control flow changes.
**Suggestion:** Remove `| None` and add an `expect` documenting the invariant, or return `ParseFailed` as an internal error if invariant is violated.
**Claude's assessment:** Agree. Dead code arms in production error-handling paths are dangerous. Remove it.

---

### F017 · [WARNING] Config loader silently ignores unknown env override keys
**Consensus:** SINGLE · flagged by: codex
**File:** `core/crates/adapters/src/config_loader.rs` · **Lines:** 233–244
**Description:** Unknown env keys (e.g., `TRILITHON_SERVRE__BIND` from a typo) are silently discarded. Operators cannot detect misconfigured overrides, which can lead to production misconfiguration.
**Suggestion:** Emit `tracing::warn!` for each ignored unknown key, or collect them and surface via `ConfigError::EnvOverride`.
**Claude's assessment:** Agree. Silent unknown-key discard is a significant usability hazard in production operations.

---

### F018 · [WARNING] Data directory writability probe leaves artifacts and races across processes
**Consensus:** MAJORITY · flagged by: codex, minimax, qwen
**File:** `core/crates/adapters/src/config_loader.rs` · **Lines:** 126–133, 182, 267–286
**Description:** The `.trilithon-write-probe` file is created with `let _ = fs::remove_file(...)` on cleanup. If removal fails, or the process is killed between create and remove, the probe file remains. Two concurrent Trilithon processes sharing a `data_dir` (misconfiguration) will conflict on the fixed probe filename.
**Suggestion:** Use `tempfile::NamedTempFile` for the probe (auto-cleans on drop) or include the PID in the filename. The `let _ =` pattern on cleanup should be replaced with explicit handling or a documented rationale.
**Claude's assessment:** Agree. The `tempfile` approach is the cleanest fix; it handles kill/crash cleanup automatically.

---

### F019 · [WARNING] `byte_offset_to_line_col` reports byte offset as column — wrong for multi-byte UTF-8
**Consensus:** MAJORITY · flagged by: codex, kimi
**File:** `core/crates/adapters/src/config_loader.rs` · **Lines:** 264–304
**Description:** `col` is computed as `safe_offset - p` (byte distance from the last newline), not a character count. TOML files containing multi-byte UTF-8 characters produce incorrect column numbers in `ConfigError::MalformedToml`, confusing operators.
**Suggestion:** Count `char`s from the last newline to the error offset (`text[last_newline..safe_offset].chars().count()`), or document explicitly that `column` is a byte offset.
**Claude's assessment:** Agree. The fix is a one-liner and makes the diagnostic meaningfully more accurate.

---

### F020 · [WARNING] `TsWriterGuard` buffers JSON lines with no size cap
**Consensus:** SINGLE · flagged by: codex
**File:** `core/crates/cli/src/observability.rs` · **Lines:** 1189–1218
**Description:** `TsWriterGuard` accumulates all writes in `self.buf` until `flush()`. Extremely large structured log events (large span lists, deeply nested fields) could consume unbounded memory.
**Suggestion:** Add a maximum buffer size (e.g., 1 MB) and truncate/error if exceeded, or document this as an accepted limitation.
**Claude's assessment:** Low urgency for Phase 1 where log volume is minimal, but worth bounding before real workloads.

---

### F021 · [WARNING] `UtcSecondsLayer` thread-local is always `None` when `TsWriterGuard` flushes
**Consensus:** MAJORITY · flagged by: codex, glm
**File:** `core/crates/cli/src/observability.rs` · **Lines:** 100–175
**Description:** In the registry chain `.with(fmt_layer).with(UtcSecondsLayer)`, the fmt layer's `on_event` flushes `TsWriterGuard` first, then `UtcSecondsLayer::on_event` sets `LAST_TS`. So `get_or_now_unix_ts()` always falls back to `now_utc()` — `UtcSecondsLayer` never contributes its stored timestamp to the JSON output. On multi-threaded subscribers, the thread-local would also fail to transfer across thread boundaries.
**Suggestion:** Move `UtcSecondsLayer` before the fmt layer so `LAST_TS` is set before the writer flushes. Alternatively, remove the `LAST_TS` indirection and call `now_utc()` directly in `TsWriterGuard`, documenting the simplification.
**Claude's assessment:** Agree. The layer ordering bug means `UtcSecondsLayer` is entirely inert. The code complexity provides no benefit. Simplifying or fixing the order are both valid.

---

### F022 · [WARNING] `TsWriterGuard::drop` swallows I/O errors silently
**Consensus:** SINGLE · flagged by: code_adversarial
**File:** `core/crates/cli/src/observability.rs` · **Lines:** 1210–1218
**Description:** The `Drop` impl uses `let _ =` on both `write_all` and `flush`. If stderr is a closed pipe (e.g., `trilithon | head -n 1`), the final JSON log line is silently lost, complicating post-mortem analysis.
**Suggestion:** Document the silent-drop behavior, or use a `flush_on_drop` flag so `Drop` only writes if `flush` was never called, reducing double-write risk while making the error path explicit.
**Claude's assessment:** Agree with the documentation suggestion at minimum. Silently losing the last log line before crash/exit is a significant debugging hazard.

---

### F023 · [WARNING] `RedactedConfig` is a brittle manual mirror of `DaemonConfig`
**Consensus:** SINGLE · flagged by: gemini
**File:** `core/crates/core/src/config/types.rs` · **Lines:** 167–258
**Description:** `RedactedConfig` manually mirrors `DaemonConfig`. If a secret-bearing field is added to `StorageConfig` in a future phase (e.g., a DB password), it will be silently leaked by `config show` unless the developer also updates the `RedactedStorageConfig` mirror.
**Suggestion:** Implement redaction via a trait, use the `secrecy` crate for sensitive fields, or add a prominent warning comment in `DaemonConfig` noting the manual-mirror dependency.
**Claude's assessment:** Agree. At minimum, a comment. A trait-based approach would be more robust for Phase 2+ when real secrets are introduced.

---

### F024 · [WARNING] Log timestamp injection uses fragile string-level JSON manipulation
**Consensus:** MAJORITY · flagged by: gemini, qwen
**File:** `core/crates/cli/src/observability.rs` · **Lines:** 154–230
**Description:** `inject_ts_unix_seconds` assumes every buffer starts with `{` to identify JSON lines. Any change to the fmt layer's output format breaks this silently. Additionally, `buf.to_vec()` allocates on every non-JSON line (e.g., Pretty format), adding overhead in high-volume logging.
**Suggestion:** Use `tracing_subscriber::fmt::time::UtcTime` with a custom `FormatTime` for unix-second timestamps instead of string injection. Use `Cow<'a, [u8]>` to avoid allocations on non-matching lines.
**Claude's assessment:** Agree. The string-injection approach is fragile. A proper `FormatTime` implementation is the idiomatic solution and is supported by `tracing-subscriber`.

---

### F025 · [WARNING] `ShutdownController::trigger` silently drops send error
**Consensus:** MAJORITY · flagged by: minimax, qwen
**File:** `core/crates/cli/src/shutdown.rs` · **Lines:** 102–120
**Description:** `trigger()` uses `let _ = self.tx.send(true)`, ignoring the `Result`. If all `ShutdownSignal` receivers are dropped before `trigger()` is called, the shutdown signal is silently lost.
**Suggestion:** Add `tracing::debug!` or `tracing::warn!` logging when `send` returns `Err` to aid future debugging.
**Claude's assessment:** Agree. The current architecture makes this scenario unlikely, but a log line costs nothing and would be invaluable in a hung-shutdown post-mortem.

---

### F026 · [WARNING] Invalid `RUST_LOG` silently falls back to default without any warning
**Consensus:** SINGLE · flagged by: kimi
**File:** `core/crates/cli/src/observability.rs` · **Lines:** 115–122
**Description:** `build_filter` silently falls back to the hardcoded default when `RUST_LOG` is malformed. An operator with a typo in `RUST_LOG` gets no feedback and the daemon starts in an unexpected log level.
**Suggestion:** Distinguish `RUST_LOG` absent from `RUST_LOG` invalid; return `ObsError::BadFilter` when the variable is present but unparseable.
**Claude's assessment:** Agree. This is a usability issue for operators. The fix is a one-liner (`EnvFilter::try_from_default_env()` error handling).

---

### F027 · [WARNING] Runtime task panic returns `StartupPreconditionFailure` (exit 3) — wrong semantics
**Consensus:** MAJORITY · flagged by: kimi, scope_guardian
**File:** `core/crates/cli/src/run.rs` · **Lines:** 43–46
**Description:** When the daemon task panics, `run_with_shutdown` returns `ExitCode::StartupPreconditionFailure` (3). That code is documented for pre-startup failures. A runtime panic during normal operation is semantically different.
**Suggestion:** Introduce a dedicated `RuntimeFailure` exit code, or document the reuse explicitly until one is added.
**Claude's assessment:** Agree with the concern. For now, documenting the deviation is acceptable; a dedicated code should be added before Phase 2.

---

### F028 · [WARNING] `pretty_and_json_dispatch` subprocess test missing (spec-required by slice 1.4)
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/cli/src/observability.rs` · **Lines:** ~1240–1350
**Description:** Slice 1.4 requires a `pretty_and_json_dispatch` test that calls `init` twice in subprocesses — asserting JSON output on the second run and not on the first. Only `utc_seconds_field_present`, `init_ok_then_already_installed`, and `inject_ts_unix_seconds_prepends_field` are present.
**Suggestion:** Add the `pretty_and_json_dispatch` subprocess test as specified in slice 1.4.
**Claude's assessment:** Agree. Spec-required test coverage gap.

---

### F029 · [WARNING] `ObsError::BadFilter` has no unit test
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/cli/src/observability.rs` · **Lines:** 1033–1045
**Description:** `ObsError::BadFilter` is defined but no test passes an invalid filter directive and asserts the error variant is returned. `init_ok_then_already_installed` only exercises `AlreadyInstalled`.
**Suggestion:** Add a unit test passing `":::invalid"` as a filter and asserting `ObsError::BadFilter` is returned.
**Claude's assessment:** Agree. Trivial to add, and directly related to F026 (RUST_LOG not surfacing BadFilter).

---

### F030 · [WARNING] `ConfigError::ReadFailed` variant not in slice 1.3 spec
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/adapters/src/config_loader.rs` · **Lines:** 126–133
**Description:** Slice 1.3 lists `ConfigError` variants: `Missing`, `MalformedToml`, `EnvOverride`, `DataDirNotWritable`, `BindAddressInvalid`, `RebaseTtlOutOfBounds`. `ReadFailed` for non-`NotFound` I/O errors is an unspecified addition.
**Suggestion:** Either remove `ReadFailed` and map all non-`NotFound` read errors to `Missing`, or document the deviation as an intentional improvement.
**Claude's assessment:** Partial disagreement. `ReadFailed` is a defensively useful variant (distinguishes "file absent" from "permission denied"). I'd document it as an intentional improvement rather than remove it.

---

### F031 · [WARNING] `ConfigError::InternalSerialise` variant not in spec
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/adapters/src/config_loader.rs` · **Lines:** 173–181
**Description:** `InternalSerialise` is not in the slice 1.3 spec. It leaks an implementation detail (re-serialize-to-table failure) into the public error type. This variant can only occur if the internal TOML round-trip fails — an invariant violation.
**Suggestion:** Remove `InternalSerialise`. If re-serialization fails, `panic!` with an explanatory message (invariant violation). Do not expand the public error surface beyond spec.
**Claude's assessment:** Agree. Invariant violations should panic, not surface as public error variants.

---

### F032 · [WARNING] `nix` version bumped beyond spec with extra features
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/Cargo.toml` · **Lines:** 20
**Description:** Slice 1.7 specifies `nix = { version = "0.27", features = ["signal"] }`. The diff uses `nix = { version = "0.29", features = ["signal", "process", "fs", "user"] }`. Version bump and extra features are scope creep.
**Suggestion:** Pin `nix` to `"0.27"` with `features = ["signal"]` per the TODO, declared only in `crates/cli/Cargo.toml` as a dev-dependency.
**Claude's assessment:** Agree with scope framing. The extra features (process, fs, user) are not needed for Phase 1 signal handling.

---

### F033 · [WARNING] `daemon_loop` returns `Ok(())` unconditionally — misleading return type
**Consensus:** SINGLE · flagged by: minimax
**File:** `core/crates/cli/src/run.rs` · **Lines:** 12–14
**Description:** `daemon_loop` returns `Ok(())` unconditionally. The `anyhow::Result<ExitCode>` return type is misleading: errors added in later phases would be silently discarded by `run_with_shutdown` unless the caller is updated.
**Suggestion:** Keep the return type but make `run_with_shutdown` propagate the actual result, or document this as a Phase 1 stub explicitly.
**Claude's assessment:** Agree. A comment noting this is a Phase 1 stub with the intent to return real errors would be sufficient for now.

---

### F034 · [WARNING] `trigger_observable` test has no timeout on `handle.await`
**Consensus:** SINGLE · flagged by: minimax
**File:** `core/crates/cli/tests/shutdown.rs` · **Lines:** 53–62
**Description:** The test spawns a task and calls `controller.trigger()` immediately. Without a timeout on `handle.await`, the test hangs indefinitely under heavy load if the task is not scheduled in time.
**Suggestion:** Add a timeout around `handle.await`, consistent with other tests in the same file.
**Claude's assessment:** Agree. Consistent with F015 — tests that can hang are as bad as tests that flake.

---

### F035 · [WARNING] Version test regex is overly permissive
**Consensus:** SINGLE · flagged by: codex
**File:** `core/crates/cli/tests/version.rs` · **Lines:** ~1990–1995
**Description:** The regex `^trilithon \S+ \(\S+\) rustc \S.*$` accepts any non-whitespace in all positions. `"trilithon foo (bar) rustc baz"` passes. It provides little assurance that the version string has semantic structure.
**Suggestion:** Tighten to `^trilithon \d+\.\d+\.\d+ \([0-9a-f]{12}\) rustc \d+\.\d+\.\d+.*$` to validate the expected format.
**Claude's assessment:** Agree. Tighter regex is a one-line change with meaningful test value.

---

## SUGGESTION / LOW Findings

### F036 · [SUGGESTION] `ObsError` lacks `#[non_exhaustive]`
**Consensus:** SINGLE · flagged by: codex
**File:** `core/crates/cli/src/observability.rs` · **Lines:** 1032–1044
**Description:** `ObsError` is public but not `#[non_exhaustive]`. Adding variants in future phases is a breaking change for external match expressions.
**Suggestion:** Add `#[non_exhaustive]` to `ObsError`, consistent with the project's convention for public error enums.
**Claude's assessment:** Agree. Minor, one-line fix.

---

### F037 · [SUGGESTION] `BootError` is an empty, unconstructable enum
**Consensus:** SINGLE · flagged by: codex
**File:** `core/crates/adapters/src/lib.rs` · **Lines:** 424–435
**Description:** `BootError` is defined as `pub enum BootError {}` and `boot()` returns `Result<(), BootError>`. An empty enum can never be constructed; the `Result` is always `Ok(())`. This may trigger clippy lints and confuses readers.
**Suggestion:** Either remove `BootError` and return `()`, or add `#[non_exhaustive]` with a `#[doc(hidden)] Placeholder` variant to signal forward-compatibility intent.
**Claude's assessment:** Agree. Remove it and return `()` for now; re-introduce when Phase 2 adds real boot failures.

---

### F038 · [SUGGESTION] `resolve_format` silently falls back to `Pretty` for any non-`"json"` value
**Consensus:** SINGLE · flagged by: code_adversarial
**File:** `core/crates/cli/src/observability.rs` · **Lines:** 1108–1114
**Description:** `TRILITHON_LOG_FORMAT=jsonn` (typo) silently uses `Pretty`. Operators get no feedback about the invalid value.
**Suggestion:** Warn when `TRILITHON_LOG_FORMAT` is set to an unrecognized value (not exactly `"json"` or `"pretty"`).
**Claude's assessment:** Agree. A `tracing::warn!` is a one-liner improvement.

---

### F039 · [SUGGESTION] `StdEnvProvider::vars_with_prefix` inconsistency with non-Unicode vars
**Consensus:** SINGLE · flagged by: code_adversarial
**File:** `core/crates/adapters/src/env_provider.rs` · **Lines:** 404–408
**Description:** `vars_with_prefix` uses `std::env::vars()` which silently skips non-Unicode env vars, while `var()` returns `VarError::NotUnicode`. A caller using both methods on the same trait will see inconsistent results for non-Unicode `TRILITHON_`-prefixed vars.
**Suggestion:** Document the discrepancy, or switch `vars_with_prefix` to `std::env::vars_os()` and surface non-Unicode entries explicitly.
**Claude's assessment:** Agree. Document at minimum; fixing is better.

---

### F040 · [SUGGESTION] `config_show::run_inner` uses `anyhow::Error`, erasing structured `ConfigError`
**Consensus:** SINGLE · flagged by: code_adversarial
**File:** `core/crates/cli/src/config_show.rs` · **Lines:** 849–857
**Description:** `run_inner` returns `anyhow::Error`, erasing the rich `ConfigError` variants. A future caller wanting to emit different exit codes for `Missing` vs `MalformedToml` would need a rewrite.
**Suggestion:** Return `Result<(), ConfigError>` from `run_inner` and map `toml::to_string_pretty` failure to a new `ConfigError` variant. Preserves the typed error through the CLI boundary.
**Claude's assessment:** Agree. Low priority now, but worth doing while this code is fresh.

---

### F041 · [SUGGESTION] `UtcSecondsLayer` is inert in the Pretty log format path
**Consensus:** MAJORITY · flagged by: kimi, scope_guardian
**File:** `core/crates/cli/src/observability.rs` · **Lines:** 75–85, 1127–1144
**Description:** `UtcSecondsLayer` is added to the subscriber for both JSON and Pretty formats, but `TsWriter` is only used in the JSON path. `ts_unix_seconds` never appears in pretty output. The layer is a no-op in the pretty path.
**Suggestion:** Remove `UtcSecondsLayer` from the Pretty subscriber chain, or extend the pretty formatter to also emit `ts_unix_seconds`. Document the JSON-only behavior if intentional.
**Claude's assessment:** Agree. Dead layers add confusion. If `ts_unix_seconds` is JSON-only by design, document it and remove the layer from the Pretty chain.

---

### F042 · [SUGGESTION] Missing test for env override with non-Unicode value
**Consensus:** SINGLE · flagged by: codex
**File:** `core/crates/adapters/tests/config_loader.rs` · **Lines:** general
**Description:** No test covers `EnvOverrideReason::NotUnicode`. The `EnvProvider` trait explicitly defines this error case and `StdEnvProvider` maps `VarError::NotUnicode` to it.
**Suggestion:** Add a test using `MapEnvProvider` returning `EnvError::NotUnicode` and assert `ConfigError::EnvOverride { reason: NotUnicode }`.

---

### F043 · [SUGGESTION] Missing test for env override parse failure
**Consensus:** SINGLE · flagged by: codex
**File:** `core/crates/adapters/tests/config_loader.rs` · **Lines:** general
**Description:** No test covers `EnvOverrideReason::ParseFailed` (e.g., `TRILITHON_CONCURRENCY__REBASE_TOKEN_TTL_MINUTES=not_a_number`).
**Suggestion:** Add a test overriding an integer field with a non-numeric string and asserting `ConfigError::EnvOverride { reason: ParseFailed }`.

---

### F044 · [SUGGESTION] Missing integration test for `config show` with missing or malformed config
**Consensus:** SINGLE · flagged by: codex
**File:** `core/crates/cli/tests/config_show.rs` · **Lines:** general
**Description:** `config_show` integration tests only cover the happy path. No test asserts exit code 2 and appropriate stderr output for missing config or malformed TOML.
**Suggestion:** Add integration tests for both failure paths, asserting exit code 2 and relevant error message patterns.

---

### F045 · [SUGGESTION] `EnvOverrideReason` uses debug formatting in user-facing error messages
**Consensus:** SINGLE · flagged by: kimi
**File:** `core/crates/adapters/src/config_loader.rs` · **Lines:** 56
**Description:** `ConfigError::EnvOverride` formats `reason` with `{:?}`, printing debug output like `ParseFailed { detail: "..." }` to users rather than a readable message.
**Suggestion:** Implement `Display` for `EnvOverrideReason` and use `{reason}` in the error string.

---

### F046 · [SUGGESTION] Signal tests may deadlock if subprocess stdout/stderr pipe buffers fill
**Consensus:** SINGLE · flagged by: kimi
**File:** `core/crates/cli/tests/signals.rs`, `core/crates/cli/tests/utc_timestamps.rs` · **Lines:** general
**Description:** Tests spawn the binary with `Stdio::piped()` but never drain the pipes until exit. If the subprocess produces more output than the OS pipe buffer (typically 64 KB), the child blocks indefinitely.
**Suggestion:** Redirect child output to temporary files, or spawn threads to drain pipes concurrently.

---

### F047 · [SUGGESTION] `ShutdownController::signal` is dead code with a `#[expect(dead_code)]` suppression
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/cli/src/shutdown.rs` · **Lines:** ~1496–1501
**Description:** `ShutdownController::signal` is suppressed with `#[expect(dead_code)]`. Slice 1.5 does not require this method. Dead code with suppressions in committed code violates the zero-debt rules.
**Suggestion:** Remove `signal()` until a later slice needs it. Re-add it with that slice's commit.

---

### F048 · [SUGGESTION] `vitest.config.ts` duplicates `@vitejs/plugin-react` from `vite.config.ts`
**Consensus:** SINGLE · flagged by: glm
**File:** `web/vitest.config.ts` · **Lines:** general
**Description:** Both `vite.config.ts` and `vitest.config.ts` import `@vitejs/plugin-react`, potentially running the plugin twice during test compilation.
**Suggestion:** Remove the react plugin from `vitest.config.ts` and let Vitest inherit from `vite.config.ts`, or add a comment if the isolation is intentional.

---

### F049 · [SUGGESTION] `pnpm-lock.yaml` should be marked `linguist-generated` in `.gitattributes`
**Consensus:** SINGLE · flagged by: codex
**File:** `web/pnpm-lock.yaml` · **Lines:** general
**Description:** The 4 400-line lockfile dominates the diff and makes review harder. It is auto-generated and should be suppressed in GitHub's diff view.
**Suggestion:** Add `web/pnpm-lock.yaml linguist-generated=true` to `.gitattributes`.

---

### F050 · [SUGGESTION] `set_by_path` does not validate empty key segments
**Consensus:** SINGLE · flagged by: qwen
**File:** `core/crates/adapters/src/config_loader.rs` · **Lines:** 220–242
**Description:** An empty segment from a malformed env key (e.g., `TRILITHON_SERVER____BIND` doubling underscores) produces a misleading `UnknownKey` error rather than a `InvalidKeySegment`-style diagnostic.
**Suggestion:** Add an empty-segment check and return a descriptive error variant.

---

### F051 · [SUGGESTION] `build.rs` git command fails silently in non-git environments
**Consensus:** SINGLE · flagged by: qwen
**File:** `core/crates/cli/build.rs` · **Lines:** 10–14
**Description:** In minimal CI containers or vendored source tarballs where `git` is unavailable, the fallback `"unknown"` is used silently.
**Suggestion:** Emit `cargo:warning=` when `git` is unavailable so packagers know version info is incomplete.

---

### F052 · [SUGGESTION] `--version` flag and `version` subcommand emit different formats
**Consensus:** SINGLE · flagged by: kimi
**File:** `core/crates/cli/src/cli.rs` · **Lines:** 10
**Description:** Clap's built-in `--version` prints only the crate version; the `version` subcommand prints `"version (git) rustc"`. Users may be confused by the inconsistency.
**Suggestion:** Override clap's version formatter or remove the built-in `--version` flag to make the subcommand canonical.

---

### F053 · [SUGGESTION] `run_daemon` config error uses raw `writeln!` instead of tracing
**Consensus:** SINGLE · flagged by: qwen
**File:** `core/crates/cli/src/main.rs` · **Lines:** 98–100
**Description:** After the tracing subscriber is installed, config load errors in `run_daemon` are written via raw `writeln!`, inconsistent with other error paths.
**Suggestion:** Use `tracing::error!(error = %e, "config.load.failed")` before returning.

---

### F054 · [SUGGESTION] `coerce_value` falls back to TOML string parsing for unhandled variant types
**Consensus:** SINGLE · flagged by: minimax
**File:** `core/crates/adapters/src/config_loader.rs` · **Lines:** 226–227
**Description:** Unrecognized TOML variants (Array, Table, Datetime) fall through to TOML string parsing, which could silently produce unexpected coercions if future TOML types are added.
**Suggestion:** Match all variants explicitly and return an error for unhandled types rather than falling back to ambiguous TOML parsing.

---

### F055 · [SUGGESTION] Advisory: version counter overflow risk (learnings_match pattern)
**Consensus:** SINGLE · flagged by: learnings_match
**File:** general · **Lines:** general
**Description:** `docs/solutions/runtime-errors/version-counter-checked-add-overflow-2026-05-06.md` — version counters using unchecked integer addition panic at `i64::MAX`. Not present in Phase 1 code but relevant to future phases.
**Suggestion:** When version counters are introduced, use `checked_add` and map `None` to a domain error.

---

### F056 · [SUGGESTION] Advisory: SQLite transaction rollback on early exit (learnings_match pattern)
**Consensus:** SINGLE · flagged by: learnings_match
**File:** general · **Lines:** general
**Description:** `docs/solutions/runtime-errors/sqlite-manual-tx-rollback-early-exit-2026-05-05.md` — manual `BEGIN`/`COMMIT` must issue explicit `ROLLBACK` on every early-exit path. Not applicable to Phase 1.
**Suggestion:** Apply when SQLite transactions are introduced.

---

### F057 · [SUGGESTION] Advisory: schema version column at model creation (learnings_match pattern)
**Consensus:** SINGLE · flagged by: learnings_match
**File:** general · **Lines:** general
**Description:** `docs/solutions/best-practices/schema-version-column-at-creation-2026-05-05.md` — add DB columns for schema-version markers at model creation time rather than retrofitting later.
**Suggestion:** Apply when DB schema is introduced.

---

### F058 · [SUGGESTION] Advisory: CIDR validation at mutation boundary (learnings_match pattern)
**Consensus:** SINGLE · flagged by: learnings_match
**File:** general · **Lines:** general
**Description:** `docs/solutions/security-issues/cidr-validate-at-mutation-boundary-2026-05-06.md` — CIDR strings must be validated at mutation time, not at push/apply time.
**Suggestion:** Apply when CIDR config fields are introduced.

---

### F059 · [SUGGESTION] Advisory: Caddy admin API uses PUT not JSON Patch (learnings_match pattern)
**Consensus:** SINGLE · flagged by: learnings_match
**File:** general · **Lines:** general
**Description:** `docs/solutions/runtime-errors/caddy-admin-api-put-not-json-patch-2026-05-06.md` — Caddy's admin API expects the replacement value directly via PUT, not an RFC 6902 JSON Patch ops array.
**Suggestion:** Apply when Caddy integration is implemented.

---

## CONFLICTS (require human decision before fixing)

No conflicts identified. All multi-reviewer findings agreed on fix direction; severity disagreements were resolved by taking the highest severity.

---

## Out-of-scope / Superseded

No findings are superseded — this is the first aggregate run for Phase 1.

---

## Summary statistics

| Severity | Unanimous | Majority | Single | Total |
|---|---|---|---|---|
| CRITICAL | 1 | 2 | 0 | **3** |
| HIGH | 0 | 3 | 7 | **10** |
| WARNING | 0 | 5 | 17 | **22** |
| SUGGESTION | 0 | 1 | 23 | **24** |
| **Total** | **1** | **11** | **47** | **59** |
