# Phase 1 — Daemon skeleton and configuration

Source of truth: [`../phases/phased-plan.md#phase-1--daemon-skeleton-and-configuration`](../phases/phased-plan.md#phase-1--daemon-skeleton-and-configuration).

> **Path-form note.** All `crates/<name>/...` paths are workspace-relative; rooted at `core/` on disk. So `crates/cli/src/main.rs` resolves to `core/crates/cli/src/main.rs`. See [`README.md`](README.md) "Path conventions". The Phase 1 file split is: `crates/cli/src/main.rs`, `crates/cli/src/cli.rs`, `crates/cli/src/exit.rs`, `crates/cli/src/shutdown.rs`, `crates/adapters/src/config_loader.rs`, `crates/adapters/src/env_provider.rs`, `crates/core/src/config/mod.rs`, `crates/core/src/config/types.rs`.

> **Authoritative cross-references.** `core::config::EnvProvider` and `core::http::HttpServer` trait surfaces consumed here are documented in [`../architecture/trait-signatures.md`](../architecture/trait-signatures.md). Tracing event names emitted by Phase 1 (`daemon.started`, `daemon.shutting-down`, `daemon.shutdown-complete`) are bound by architecture §12.1.

## Pre-flight checklist

- [ ] The Rust workspace at `core/` compiles cleanly under `cargo build --workspace`.
- [ ] The repository has a working `just check` recipe that runs `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace`.
- [ ] `crates/cli/src/main.rs` exists as scaffolding from Phase 0.
- [ ] The web application at `web/` is unchanged for this phase.

## Tasks

### Backend / cli crate

- [ ] **Define the `clap` derive command surface.**
  - Acceptance: Trilithon MUST expose subcommands `run`, `config show`, and `version` via a `clap` derive struct in `crates/cli/src/cli.rs`.
  - Done when: `cargo run -p trilithon-cli -- --help` lists the three subcommands and `cargo test -p trilithon-cli cli::tests` passes.
  - Feature: foundational (unblocks T1.1).
- [ ] **Wire `version` to print build metadata.**
  - Acceptance: `version` MUST print the crate version, the git short hash if available, and the Rust toolchain version on a single line.
  - Done when: `cargo run -p trilithon-cli -- version` matches the snapshot in `crates/cli/tests/version.rs`.
  - Feature: foundational.
- [ ] **Initialise the tracing subscriber before any other startup work.**
  - Acceptance: `crates/cli/src/observability.rs` MUST initialise `tracing-subscriber` with environment-variable filter support and JSON output when `TRILITHON_LOG_FORMAT=json`.
  - Done when: starting the daemon with and without `TRILITHON_LOG_FORMAT=json` produces the documented format and the unit test in `crates/cli/src/observability.rs` passes.
  - Feature: foundational (cross-cuts T1.7).
- [ ] **Emit a fixed pre-filter line on stderr before subscriber init.**
  - Acceptance: A single line `trilithon: starting (pre-tracing)` MUST appear on stderr before the subscriber is installed.
  - Done when: `cargo run -p trilithon-cli -- run 2>&1 1>/dev/null` shows the pre-tracing line on the first line.
  - Feature: foundational.
- [ ] **Implement graceful shutdown on SIGINT and SIGTERM.**
  - Acceptance: The daemon MUST listen for `SIGINT` and `SIGTERM` via `tokio::signal::unix`, broadcast a shutdown notification to owned tasks, and wait up to 10 seconds before forcing exit.
  - Done when: an integration test sends `SIGINT` to a spawned daemon and observes `daemon.shutting-down` followed by exit code `0` within 10 seconds.
  - Feature: foundational.
- [ ] **Gate Windows out of V1 explicitly.**
  - Acceptance: `crates/cli/src/main.rs` MUST `#[cfg]`-gate signal handling for Unix targets and produce a typed compile-time error or a `cfg(not(unix))` panic-with-message for non-Unix targets.
  - Done when: `cargo check -p trilithon-cli --target x86_64-pc-windows-msvc` fails with a clear message and `cargo check -p trilithon-cli` succeeds on macOS and Linux.
  - Feature: foundational.

### Backend / core crate

- [ ] **Define the typed `DaemonConfig` struct.**
  - Path: `crates/core/src/config/types.rs`.
  - Acceptance: The Rust definitions MUST appear verbatim:

    ```rust
    pub struct DaemonConfig {
        pub server:       ServerConfig,
        pub caddy:        CaddyConfig,
        pub storage:      StorageConfig,
        pub secrets:      SecretsConfig,
        pub concurrency:  ConcurrencyConfig,
        pub tracing:      TracingConfig,
        pub bootstrap:    BootstrapConfig,
    }
    pub struct ServerConfig {
        pub bind: SocketAddr,                      // default 127.0.0.1:7878
        pub allow_remote: bool,                    // default false
    }
    pub enum CaddyEndpoint {
        Unix(PathBuf),                             // unix:///run/caddy/admin.sock
        LoopbackTls {
            url: Url,                              // https://127.0.0.1:2019
            mtls_cert_path: PathBuf,
            mtls_key_path:  PathBuf,
            mtls_ca_path:   PathBuf,
        },
    }
    pub struct CaddyConfig {
        pub admin_endpoint: CaddyEndpoint,
        pub connect_timeout_seconds: u32,          // default 10
        pub apply_timeout_seconds:   u32,          // default 60
    }
    pub struct StorageConfig {
        pub data_dir: PathBuf,
        pub wal_checkpoint_pages: u32,             // default 1000
    }
    pub struct SecretsConfig {
        pub master_key_backend: SecretsBackend,    // Keychain | File { path }
    }
    pub struct ConcurrencyConfig {
        pub rebase_token_ttl_minutes: u32,         // default 30, bounds [5, 1440]
    }
    pub struct TracingConfig {
        pub log_filter: String,                    // default "info,trilithon=info"
        pub format: LogFormat,                     // Json | Pretty
    }
    pub struct BootstrapConfig {
        pub enabled_on_first_run: bool,            // default true
        pub credentials_file: PathBuf,             // default /var/lib/trilithon/bootstrap.json (mode 0600)
    }
    ```

    The disambiguation between the Unix socket and the loopback-mTLS endpoint MUST live in the `CaddyEndpoint` enum, not in stringly-typed configuration. The `caddy_admin_endpoint` config key is canonical (architecture §8.1, ADR-0010, ADR-0011, ADR-0015).
  - Done when: `cargo test -p trilithon-core config::tests::defaults_match_doc` passes and the struct contains no I/O or async dependencies.
  - Feature: foundational.
- [ ] **Encode the typed exit-code enum.**
  - Acceptance: `crates/core/src/exit.rs` MUST define `ExitCode` with variants `CleanShutdown = 0`, `ConfigError = 2`, `StartupPreconditionFailure = 3`, `InvalidInvocation = 64`.
  - Done when: `cargo test -p trilithon-core exit::tests::values_are_stable` passes.
  - Feature: foundational.
- [ ] **Provide `DaemonConfig::redacted` for safe display.**
  - Acceptance: A `redacted` accessor MUST elide every secret-like field (any path or value the schema marks as sensitive) and MUST be the only public path used by `config show`.
  - Done when: a unit test confirms a configuration containing a sensitive value renders the value as `***`.
  - Feature: foundational (precursor to T1.15).

### Backend / adapters crate

- [ ] **Implement the TOML configuration loader.**
  - Path: `crates/adapters/src/config_loader.rs`.
  - Acceptance: The loader MUST expose exactly:

    ```rust
    pub fn load_config(
        path: &Path,
        env: &dyn EnvProvider,
    ) -> Result<DaemonConfig, ConfigError>;
    ```

    `ConfigError` MUST enumerate verbatim:

    ```rust
    pub enum ConfigError {
        Missing               { path: PathBuf },
        MalformedToml         { line: usize, column: usize, source: toml::de::Error },
        EnvOverride           { var: String, reason: EnvOverrideReason },
        DataDirNotWritable    { path: PathBuf, source: io::Error },
        BindAddressInvalid    { value: String },
        RebaseTtlOutOfBounds  { value: u32 },
    }
    ```

    The loader reads the TOML file, overlays environment variables prefixed `TRILITHON_` (consumed via the supplied `EnvProvider`; see `core::config::EnvProvider` in [`../architecture/trait-signatures.md`](../architecture/trait-signatures.md)), and returns either `DaemonConfig` or a typed `ConfigError`.
  - Done when: integration tests in `crates/adapters/tests/config_loader.rs` cover the happy path, missing file, malformed TOML, environment-variable override, and `RebaseTtlOutOfBounds` boundary cases (4, 5, 1440, 1441).
  - Feature: foundational.
- [ ] **Reject configurations whose data directory is not writable.**
  - Acceptance: The loader MUST surface a typed `ConfigError::DataDirNotWritable` when the resolved `data_dir` is not a writable directory.
  - Done when: an integration test using a read-only fixture directory observes the typed error.
  - Feature: foundational (precursor to T1.2).

### Documentation

- [ ] **Document run and configuration in `core/README.md`.**
  - Acceptance: `core/README.md` MUST include a "Running the daemon" section, the configuration file path, the environment-variable override convention, and the exit-code table.
  - Done when: the section exists and the exit-code table matches the `ExitCode` enum verbatim.
  - Feature: foundational.

### Tests

- [ ] **Snapshot the resolved-configuration output of `config show`.**
  - Acceptance: `config show` against a fixture configuration MUST print the resolved configuration with secret-like fields elided, byte-identical to a snapshot.
  - Done when: `cargo test -p trilithon-cli config_show::shows_redacted` passes via `insta`.
  - Feature: foundational.
- [ ] **End-to-end exit-code test for missing configuration.**
  - Acceptance: Running `trilithon run --config /nonexistent.toml` MUST exit with code `2` and emit a structured error pointing at the missing path.
  - Done when: an integration test asserts the exit code and the structured error.
  - Feature: foundational.
- [ ] **End-to-end signal handling test.**
  - Acceptance: Running `trilithon run` and sending `SIGTERM` MUST cause the process to emit `daemon.shutting-down`, drain owned tasks, and exit `0` within 10 seconds.
  - Done when: the integration test in `crates/cli/tests/signals.rs` passes on macOS and Linux.
  - Feature: foundational.
- [ ] **Verify wall-clock timestamps are UTC Unix seconds.**
  - Acceptance: Every tracing event MUST carry a UTC Unix timestamp field; rendering MAY add timezone-aware presentation but storage MUST be UTC seconds, satisfying H6.
  - Done when: a unit test on the tracing layer asserts a stable epoch field.
  - Feature: foundational.

## Cross-references

- ADR-0003 (three-layer Rust workspace).
- ADR-0011 (loopback-only by default).
- PRD T1.13 (web UI delivery — daemon must be runnable for the UI to attach).
- Architecture: "Process model and threading," "Observability — tracing and correlation."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] Running `trilithon run` against a valid configuration starts the daemon, emits `daemon.started`, and runs until a signal is received.
- [ ] `SIGINT` causes the daemon to emit `daemon.shutting-down`, drain, and exit `0` within 10 seconds.
- [ ] `trilithon run` with a missing configuration file exits with code `2` and a structured error pointing at the missing path.
- [ ] `trilithon config show` prints the resolved configuration with all secret-like fields elided.
- [ ] All wall-clock timestamps in logs are UTC Unix timestamps with timezone-aware rendering, satisfying H6.
