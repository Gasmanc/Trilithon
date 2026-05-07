# Phase 1 — Fixed Findings

**Run date:** 2026-05-07T00:00:00Z
**Total fixed:** 48

| ID | Severity | Title | File | Commit | PR | Date |
|----|----------|-------|------|--------|----|------|
| F001 | CRITICAL | Workspace root Cargo.toml polluted with I/O, async, and test dependencies | `core/Cargo.toml` | `ac07b47` | — | 2026-05-07 |
| F002 | CRITICAL | Tracing subscriber initialized with hardcoded defaults before config file is loaded | `core/crates/cli/src/main.rs` | `aea27e8` | — | 2026-05-07 |
| F003 | CRITICAL | core crate gains I/O-adjacent dependency (url) | `core/crates/core/Cargo.toml` | `b8fdbd0` | — | 2026-05-07 |
| F004 | HIGH | Background tasks not awaited during graceful shutdown | `core/crates/cli/src/run.rs` | `06c9c22` | — | 2026-05-07 |
| F005 | HIGH | Build script rerun-if-changed paths resolve to non-existent files | `core/crates/cli/build.rs` | `06c9c22` | — | 2026-05-07 |
| F006 | HIGH | Cli::parse() exit code 2 conflicts with ExitCode::InvalidInvocation (64) | `core/crates/cli/src/main.rs` | `28e7b24` | — | 2026-05-07 |
| F007 | HIGH | web/ frontend scaffold out of scope for Phase 1 | `web/` | `06c9c22` | — | 2026-05-07 |
| F008 | HIGH | ConfigError::BindAddressInvalid variant missing from implementation | `core/crates/adapters/src/config_loader.rs` | `06c9c22` | — | 2026-05-07 |
| F009 | HIGH | ShutdownSignal::wait() loops forever if signal handler panics before triggering | `core/crates/cli/src/shutdown.rs` | `06c9c22` | — | 2026-05-07 |
| F010 | HIGH | Test fixtures use hardcoded /tmp paths causing inter-test races | `core/crates/adapters/tests/fixtures/minimal.toml` | `1bc3923` | — | 2026-05-07 |
| F011 | HIGH | config_show.rs writes errors to stderr with raw writeln! after tracing is active | `core/crates/cli/src/config_show.rs` | `06c9c22` | — | 2026-05-07 |
| F012 | HIGH | config show exit-code-2 path lacks integration test coverage | `core/crates/cli/tests/config_show.rs` | `06c9c22` | — | 2026-05-07 |
| F013 | HIGH | Pre-tracing line always written even for fast-exit commands (version, help) | `core/crates/cli/src/main.rs` | `aa8b2a9` | — | 2026-05-07 |
| F015 | WARNING | Signal tests use fixed 2-second sleep instead of readiness probe | `core/crates/cli/tests/signals.rs` | `263af98` | — | 2026-05-07 |
| F017 | WARNING | Unknown TRILITHON_* env overrides are silently dropped | `core/crates/adapters/src/config_loader.rs` | `263af98` | — | 2026-05-07 |
| F018 | WARNING | Write probe uses a fixed filename — concurrent processes collide | `core/crates/adapters/src/config_loader.rs` | `263af98` | — | 2026-05-07 |
| F019 | WARNING | byte_offset_to_line_col uses byte arithmetic for column — wrong for multi-byte chars | `core/crates/adapters/src/config_loader.rs` | `263af98` | — | 2026-05-07 |
| F020 | WARNING | TsWriterGuard buffer has no upper bound — unbounded memory growth | `core/crates/cli/src/observability.rs` | `263af98` | — | 2026-05-07 |
| F021 | WARNING | TsWriterGuard captures timestamp at flush time, not at event creation time | `core/crates/cli/src/observability.rs` | `263af98` | — | 2026-05-07 |
| F022 | WARNING | TsWriterGuard Drop silently swallows flush errors | `core/crates/cli/src/observability.rs` | `263af98` | — | 2026-05-07 |
| F023 | WARNING | Redaction tests only cover Unix endpoint and Keychain backend | `core/crates/core/src/config/types.rs` | `263af98` | — | 2026-05-07 |
| F025 | WARNING | ShutdownController::trigger silently ignores send errors | `core/crates/cli/src/shutdown.rs` | `263af98` | — | 2026-05-07 |
| F026 | WARNING | Invalid RUST_LOG silently falls back without user-visible warning | `core/crates/cli/src/observability.rs` | `263af98` | — | 2026-05-07 |
| F027 | WARNING | Task panics exit with CleanShutdown=0 instead of a distinct error code | `core/crates/cli/src/run.rs` | `263af98` | — | 2026-05-07 |
| F028 | WARNING | No test covers the JSON vs Pretty format dispatch path | `core/crates/cli/src/observability.rs` | `263af98` | — | 2026-05-07 |
| F029 | WARNING | ObsError::BadFilter variant has no unit test | `core/crates/cli/src/observability.rs` | `263af98` | — | 2026-05-07 |
| F030 | WARNING | ReadFailed doc comment does not note intentional POSIX distinction from Missing | `core/crates/adapters/src/config_loader.rs` | `263af98` | — | 2026-05-07 |
| F032 | WARNING | nix version bumped beyond spec with extra features | `core/Cargo.toml` | `ac07b47` | — | 2026-05-07 |
| F035 | WARNING | Version test regex too permissive — matches garbage output | `core/crates/cli/tests/version.rs` | `263af98` | — | 2026-05-07 |
| F036 | SUGGESTION | ObsError lacks #[non_exhaustive] | `core/crates/cli/src/observability.rs` | `c646fb8` | — | 2026-05-07 |
| F037 | SUGGESTION | BootError is an empty, unconstructable enum | `core/crates/adapters/src/lib.rs` | `c646fb8` | — | 2026-05-07 |
| F038 | SUGGESTION | resolve_format silently falls back to Pretty for any non-json value | `core/crates/cli/src/observability.rs` | `c646fb8` | — | 2026-05-07 |
| F039 | SUGGESTION | StdEnvProvider::vars_with_prefix inconsistency with non-Unicode vars | `core/crates/adapters/src/env_provider.rs` | `c646fb8` | — | 2026-05-07 |
| F040 | SUGGESTION | config_show::run_inner uses anyhow::Error, erasing structured ConfigError | `core/crates/cli/src/config_show.rs` | `c646fb8` | — | 2026-05-07 |
| F041 | SUGGESTION | UtcSecondsLayer is inert in the Pretty log format path | `core/crates/cli/src/observability.rs` | `c646fb8` | — | 2026-05-07 |
| F042 | SUGGESTION | Missing test for env override with non-Unicode value | `core/crates/adapters/tests/config_loader.rs` | `c646fb8` | — | 2026-05-07 |
| F043 | SUGGESTION | Missing test for env override parse failure | `core/crates/adapters/tests/config_loader.rs` | `c646fb8` | — | 2026-05-07 |
| F044 | SUGGESTION | Missing integration test for config show with missing or malformed config | `core/crates/cli/tests/config_show.rs` | `c646fb8` | — | 2026-05-07 |
| F045 | SUGGESTION | EnvOverrideReason uses debug formatting in user-facing error messages | `core/crates/adapters/src/config_loader.rs` | `c646fb8` | — | 2026-05-07 |
| F046 | SUGGESTION | Signal tests may deadlock if subprocess stdout/stderr pipe buffers fill | `core/crates/cli/tests/utc_timestamps.rs` | `c646fb8` | — | 2026-05-07 |
| F047 | SUGGESTION | ShutdownController::signal is dead code with #[expect(dead_code)] suppression | `core/crates/cli/src/shutdown.rs` | `c646fb8` | — | 2026-05-07 |
| F048 | SUGGESTION | vitest.config.ts duplicates @vitejs/plugin-react from vite.config.ts | `web/vitest.config.ts` | `c646fb8` | — | 2026-05-07 |
| F049 | SUGGESTION | pnpm-lock.yaml should be marked linguist-generated in .gitattributes | `.gitattributes` | `c646fb8` | — | 2026-05-07 |
| F050 | SUGGESTION | set_by_path does not validate empty key segments | `core/crates/adapters/src/config_loader.rs` | `c646fb8` | — | 2026-05-07 |
| F051 | SUGGESTION | build.rs git command fails silently in non-git environments | `core/crates/cli/build.rs` | `c646fb8` | — | 2026-05-07 |
| F052 | SUGGESTION | --version flag and version subcommand emit different formats | `core/crates/cli/src/cli.rs` | `c646fb8` | — | 2026-05-07 |
| F053 | SUGGESTION | run_daemon config error uses raw writeln! instead of tracing | `core/crates/cli/src/main.rs` | `c646fb8` | — | 2026-05-07 |
| F054 | SUGGESTION | coerce_value falls back to TOML string parsing for unhandled variant types | `core/crates/adapters/src/config_loader.rs` | `c646fb8` | — | 2026-05-07 |
