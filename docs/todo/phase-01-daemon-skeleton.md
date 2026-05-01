# Phase 01 — Daemon skeleton and configuration — Implementation Slices

> Phase reference: [../phases/phase-01-daemon-skeleton.md](../phases/phase-01-daemon-skeleton.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md) §phase-1--daemon-skeleton-and-configuration
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference: [`../phases/phase-01-daemon-skeleton.md`](../phases/phase-01-daemon-skeleton.md).
- Architecture §4.1 (`core` crate), §4.3 (`cli` crate), §5 (layer rules), §12 (observability), §12.1 (tracing vocabulary).
- Trait signatures: `core::config::EnvProvider` (§9), `core::http::HttpServer` (§10) — bound but not implemented in this phase.
- ADR-0003 (three-layer Rust workspace), ADR-0010 (two-container deployment), ADR-0011 (loopback-only by default), ADR-0015 (instance ownership sentinel).

## Slice plan summary

| Slice | Title | Primary files | Effort (h) | Depends on |
|-------|-------|---------------|------------|------------|
| 1.1 | Workspace skeleton, exit codes, `version` subcommand | `crates/cli/src/{main,cli,exit}.rs`, `crates/core/src/exit.rs` | 3 | — |
| 1.2 | `DaemonConfig` typed records in `core` | `crates/core/src/config/{mod,types}.rs` | 4 | 1.1 |
| 1.3 | `EnvProvider` trait, TOML loader, `ConfigError` | `crates/adapters/src/{config_loader,env_provider}.rs`, `crates/core/src/config/env.rs` | 6 | 1.2 |
| 1.4 | Tracing subscriber, pre-tracing line, UTC-seconds layer | `crates/cli/src/observability.rs` | 4 | 1.1 |
| 1.5 | Signal handling and graceful shutdown | `crates/cli/src/shutdown.rs`, `crates/cli/src/main.rs` | 4 | 1.4 |
| 1.6 | `config show` with redaction | `crates/cli/src/cli.rs`, `crates/core/src/config/types.rs` | 3 | 1.3 |
| 1.7 | End-to-end exit-code and signal integration tests | `crates/cli/tests/{signals,missing_config,version}.rs` | 4 | 1.5, 1.6 |

Total: 7 slices.

---

## Slice 1.1 [cross-cutting] — Workspace skeleton, exit codes, `version` subcommand

### Goal

Stand up the three-crate workspace at `core/`, add the `clap` derive command surface (`run`, `config show`, `version`) in `crates/cli/`, and define the typed `ExitCode` enum in `crates/core/src/exit.rs`. Trilithon's `version` subcommand MUST print one line carrying the crate version, the git short hash, and the toolchain version.

### Entry conditions

- Phase 0 scaffolding has produced `core/Cargo.toml` declaring members `crates/core`, `crates/adapters`, `crates/cli`, `crates/ffi`.
- `crates/cli/src/main.rs` exists as a stub.
- `just check` recipe runs `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`.

### Files to create or modify

- `core/crates/core/src/exit.rs` — typed exit code enum (new).
- `core/crates/core/src/lib.rs` — re-export `pub mod exit;` (modify).
- `core/crates/cli/src/cli.rs` — `clap` derive command surface (new).
- `core/crates/cli/src/exit.rs` — adapter from typed exit codes to `std::process::ExitCode` (new).
- `core/crates/cli/src/main.rs` — wire `Cli::parse()` and dispatch (modify).
- `core/crates/cli/build.rs` — emit `GIT_SHORT_HASH` and `RUSTC_VERSION` to `cargo:rustc-env` (new).
- `core/crates/cli/Cargo.toml` — declare deps `clap = { version = "4", features = ["derive"] }`, `trilithon-core = { path = "../core" }` (modify).

### Signatures and shapes

```rust
// core/crates/core/src/exit.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    CleanShutdown = 0,
    ConfigError = 2,
    StartupPreconditionFailure = 3,
    InvalidInvocation = 64,
}

impl ExitCode {
    pub const fn as_u8(self) -> u8 { self as u8 }
}
```

```rust
// core/crates/cli/src/cli.rs
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "trilithon", version, about = "Trilithon daemon")]
pub struct Cli {
    /// Path to the daemon configuration file.
    #[arg(long, default_value = "/etc/trilithon/config.toml", global = true)]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the Trilithon daemon.
    Run,
    /// Configuration inspection subcommands.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Print the build version line and exit.
    Version,
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Resolve and print the configuration with secrets elided.
    Show,
}
```

```rust
// core/crates/cli/build.rs (excerpt)
fn main() {
    let git = std::process::Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=TRILITHON_GIT_SHORT_HASH={git}");
    let rustc = std::process::Command::new(env!("RUSTC"))
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "rustc-unknown".into());
    println!("cargo:rustc-env=TRILITHON_RUSTC_VERSION={rustc}");
}
```

The `version` subcommand MUST print exactly one line of the form:

```
trilithon <CARGO_PKG_VERSION> (<TRILITHON_GIT_SHORT_HASH>) <TRILITHON_RUSTC_VERSION>
```

### Algorithm

1. `main()` parses `Cli` via `clap::Parser::parse()`.
2. On `Command::Version`, write the formatted line to stdout, then return `ExitCode::CleanShutdown`.
3. On `Command::Run`, defer to a placeholder `run::placeholder()` returning `ExitCode::CleanShutdown` (slice 1.5 will replace).
4. On `Command::Config { action: ConfigAction::Show }`, defer to a placeholder returning `ExitCode::CleanShutdown` (slice 1.6 will replace).
5. The `cli/src/exit.rs` adapter converts `core::exit::ExitCode` into `std::process::ExitCode::from(code.as_u8())`.

### Tests

- `core/crates/core/src/exit.rs` `mod tests::values_are_stable` — asserts `ExitCode::CleanShutdown as u8 == 0`, `ConfigError == 2`, `StartupPreconditionFailure == 3`, `InvalidInvocation == 64`.
- `core/crates/cli/src/cli.rs` `mod tests::parses_three_subcommands` — uses `Cli::try_parse_from(["trilithon", "version"])` and asserts the matched variant; repeats for `run` and `config show`.
- `core/crates/cli/tests/version.rs` — invokes the binary via `assert_cmd`, asserts the line matches the regex `^trilithon \S+ \(\S+\) rustc \S.*$`.

### Acceptance command

```
cargo test -p trilithon-core exit::tests::values_are_stable && \
cargo test -p trilithon-cli cli::tests::parses_three_subcommands && \
cargo test -p trilithon-cli --test version
```

### Exit conditions

- `cargo build --workspace` succeeds.
- `cargo run -p trilithon-cli -- --help` lists `run`, `config`, `version`.
- `cargo run -p trilithon-cli -- version` prints the documented one-line format.
- The three named tests pass.
- `cargo check -p trilithon-cli --target x86_64-pc-windows-msvc` MAY succeed at this slice (Windows gating arrives in slice 1.5).

### Audit kinds emitted

None. Audit-row writes do not begin until Phase 6.

### Tracing events emitted

None. Tracing subscriber installation is slice 1.4.

### Cross-references

- ADR-0003 (three-layer Rust workspace).
- Architecture §4.3 (`cli` crate), §5 (layer rules).
- Phase reference: "Define the `clap` derive command surface", "Wire `version` to print build metadata", "Encode the typed exit-code enum".

---

## Slice 1.2 [standard] — `DaemonConfig` typed records in `core`

### Goal

Define the pure typed configuration model in `crates/core/src/config/types.rs` exactly as the phase reference dictates. The records carry no I/O, no async, no filesystem access; they are `serde::Deserialize` plus `Debug` and `Clone`. The `redacted` accessor for `DaemonConfig` is also defined here so the `config show` slice (1.6) can call it without reaching outside `core`.

### Entry conditions

- Slice 1.1 complete; `trilithon-core` builds.
- `crates/core/Cargo.toml` declares `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `url = { version = "2", features = ["serde"] }`, `thiserror = "1"`.
- `crates/core/Cargo.toml` MUST NOT declare `tokio`, `sqlx`, `reqwest`, `hyper`, or `axum` (architecture §5).

### Files to create or modify

- `core/crates/core/src/config/mod.rs` — re-exports `pub mod types; pub use types::*;` (new).
- `core/crates/core/src/config/types.rs` — typed records (new).
- `core/crates/core/src/lib.rs` — add `pub mod config;` (modify).

### Signatures and shapes

```rust
// core/crates/core/src/config/types.rs
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub server:      ServerConfig,
    pub caddy:       CaddyConfig,
    pub storage:     StorageConfig,
    pub secrets:     SecretsConfig,
    pub concurrency: ConcurrencyConfig,
    pub tracing:     TracingConfig,
    pub bootstrap:   BootstrapConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Default 127.0.0.1:7878.
    pub bind: SocketAddr,
    /// Default false. ADR-0011.
    #[serde(default)]
    pub allow_remote: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum CaddyEndpoint {
    Unix { path: PathBuf },
    LoopbackTls {
        url: Url,
        mtls_cert_path: PathBuf,
        mtls_key_path:  PathBuf,
        mtls_ca_path:   PathBuf,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaddyConfig {
    pub admin_endpoint: CaddyEndpoint,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_seconds: u32,
    #[serde(default = "default_apply_timeout")]
    pub apply_timeout_seconds: u32,
}
fn default_connect_timeout() -> u32 { 10 }
fn default_apply_timeout()   -> u32 { 60 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub data_dir: PathBuf,
    #[serde(default = "default_wal_pages")]
    pub wal_checkpoint_pages: u32,
}
fn default_wal_pages() -> u32 { 1000 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    pub master_key_backend: SecretsBackend,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum SecretsBackend {
    Keychain,
    File { path: PathBuf },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrencyConfig {
    /// Default 30, bounds [5, 1440].
    #[serde(default = "default_rebase_ttl")]
    pub rebase_token_ttl_minutes: u32,
}
fn default_rebase_ttl() -> u32 { 30 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    /// Default "info,trilithon=info".
    #[serde(default = "default_log_filter")]
    pub log_filter: String,
    #[serde(default)]
    pub format: LogFormat,
}
fn default_log_filter() -> String { "info,trilithon=info".into() }

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    #[default]
    Pretty,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapConfig {
    #[serde(default = "default_bootstrap_enabled")]
    pub enabled_on_first_run: bool,
    #[serde(default = "default_bootstrap_credentials")]
    pub credentials_file: PathBuf,
}
fn default_bootstrap_enabled() -> bool { true }
fn default_bootstrap_credentials() -> PathBuf {
    PathBuf::from("/var/lib/trilithon/bootstrap.json")
}

impl DaemonConfig {
    /// Return a redacted `Display`-ready view; every secret-like field is
    /// rendered as the literal string `***`.
    pub fn redacted(&self) -> RedactedConfig { RedactedConfig::from(self) }
}

#[derive(Debug, Clone, Serialize)]
pub struct RedactedConfig { /* mirror fields, secret-bearing fields as &'static str = "***" */ }
```

`RedactedConfig` field shape: every `PathBuf` whose semantic role is a secret (`mtls_key_path`, `mtls_cert_path`, `credentials_file`, `SecretsBackend::File { path }`) is replaced with the constant string `"***"`. Non-secret paths (`data_dir`, `mtls_ca_path` — public PEM) remain visible.

### Algorithm

1. `redacted()` constructs a `RedactedConfig` whose serde representation matches the input config but elides the following six paths:
   1. `caddy.admin_endpoint.mtls_cert_path`
   2. `caddy.admin_endpoint.mtls_key_path`
   3. `secrets.master_key_backend` (entire enum body when `File`)
   4. `bootstrap.credentials_file`
2. The replacement token is the literal three-character string `***`.
3. Defaults, when applied via `serde(default)`, MUST match the values shown in the phase reference and PRD verbatim.

### Tests

- `core/crates/core/src/config/types.rs` `mod tests::defaults_match_doc` — deserialises the minimal TOML fixture below and asserts every default equals the documented value.

  Fixture TOML (also stored as `core/crates/core/tests/fixtures/minimal.toml`):

  ```toml
  [server]
  bind = "127.0.0.1:7878"

  [caddy.admin_endpoint]
  transport = "unix"
  path = "/run/caddy/admin.sock"

  [storage]
  data_dir = "/var/lib/trilithon"

  [secrets.master_key_backend]
  backend = "keychain"

  [concurrency]

  [tracing]

  [bootstrap]
  ```

- `core/crates/core/src/config/types.rs` `mod tests::redacted_elides_secret_paths` — constructs a `DaemonConfig` whose `bootstrap.credentials_file = "/etc/secret"`, calls `redacted()`, serialises to JSON, and asserts the JSON contains `"***"` and does NOT contain `"/etc/secret"`.
- `core/crates/core/src/config/types.rs` `mod tests::core_has_no_io_deps` — compile-time guard via `cargo metadata` is OUT OF SCOPE here; instead, a `// zd:CONFIG-NO-IO expires:2027-04-30 reason:enforced by Cargo.toml manifest review` comment marks the boundary.

### Acceptance command

```
cargo test -p trilithon-core config::types::tests
```

### Exit conditions

- `crates/core/Cargo.toml` declares no I/O or async dependency.
- `DaemonConfig`, `ServerConfig`, `CaddyConfig`, `CaddyEndpoint`, `StorageConfig`, `SecretsConfig`, `SecretsBackend`, `ConcurrencyConfig`, `TracingConfig`, `LogFormat`, `BootstrapConfig` compile.
- `DaemonConfig::redacted()` exists and returns a value whose serde form contains the literal `***` at every secret-bearing path.
- The two named tests pass.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- Architecture §4.1 (`core` crate boundary), §5 (no I/O in `core`), §11 (security posture).
- ADR-0011 (loopback default), ADR-0014 (secrets at rest), ADR-0015 (ownership sentinel).
- PRD T1.15 (secrets metadata — precursor surface).

---

## Slice 1.3 [cross-cutting] — `EnvProvider` trait, TOML loader, `ConfigError`

### Goal

Implement the configuration loader at `crates/adapters/src/config_loader.rs`, the `EnvProvider` trait surface in `core` (matching trait-signatures.md §9 verbatim), and a `StdEnvProvider` adapter. The loader reads a TOML file, overlays environment variables prefixed `TRILITHON_`, validates the result, and returns either `DaemonConfig` or a typed `ConfigError`. The `RebaseTtlOutOfBounds` boundary `[5, 1440]` MUST be enforced here. Data-directory writability is enforced by attempting to open a temporary file under the resolved path.

### Entry conditions

- Slice 1.2 complete; `DaemonConfig` exists.
- `crates/adapters/Cargo.toml` declares `toml = "0.8"`, `serde = "1"`, `thiserror = "1"`, `trilithon-core = { path = "../core" }`.

### Files to create or modify

- `core/crates/core/src/config/env.rs` — `EnvProvider` trait, `EnvError` (new).
- `core/crates/core/src/config/mod.rs` — re-export `pub mod env; pub use env::*;` (modify).
- `core/crates/adapters/src/config_loader.rs` — TOML loader and `ConfigError` (new).
- `core/crates/adapters/src/env_provider.rs` — `StdEnvProvider` over `std::env` (new).
- `core/crates/adapters/src/lib.rs` — add `pub mod config_loader; pub mod env_provider;` (modify).
- `core/crates/adapters/tests/config_loader.rs` — integration tests (new).
- `core/crates/adapters/tests/fixtures/` — minimal, malformed, env-override, read-only-data-dir fixtures (new).

### Signatures and shapes

```rust
// core/crates/core/src/config/env.rs
pub trait EnvProvider: Send + Sync + 'static {
    fn var(&self, key: &str) -> Result<String, EnvError>;
    fn vars_with_prefix(&self, prefix: &str) -> Vec<(String, String)>;
}

#[derive(Debug, thiserror::Error)]
pub enum EnvError {
    #[error("environment variable {key} is not present")]
    NotPresent { key: String },
    #[error("environment variable {key} is not valid Unicode")]
    NotUnicode { key: String },
}
```

```rust
// core/crates/adapters/src/config_loader.rs
use std::path::{Path, PathBuf};
use std::io;
use trilithon_core::config::{DaemonConfig, EnvProvider};

pub fn load_config(
    path: &Path,
    env: &dyn EnvProvider,
) -> Result<DaemonConfig, ConfigError>;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("configuration file not found at {path}")]
    Missing { path: PathBuf },

    #[error("malformed TOML at line {line}, column {column}: {source}")]
    MalformedToml {
        line: usize,
        column: usize,
        #[source]
        source: toml::de::Error,
    },

    #[error("invalid environment override {var}: {reason:?}")]
    EnvOverride { var: String, reason: EnvOverrideReason },

    #[error("data directory not writable at {path}: {source}")]
    DataDirNotWritable {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("bind address {value} is invalid")]
    BindAddressInvalid { value: String },

    #[error("rebase TTL {value} minutes is outside [5, 1440]")]
    RebaseTtlOutOfBounds { value: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvOverrideReason {
    NotUnicode,
    UnknownKey,
    ParseFailed { detail: String },
}
```

```rust
// core/crates/adapters/src/env_provider.rs
pub struct StdEnvProvider;

impl trilithon_core::config::EnvProvider for StdEnvProvider {
    fn var(&self, key: &str) -> Result<String, trilithon_core::config::EnvError> {
        match std::env::var(key) {
            Ok(v)                              => Ok(v),
            Err(std::env::VarError::NotPresent) => Err(trilithon_core::config::EnvError::NotPresent { key: key.into() }),
            Err(std::env::VarError::NotUnicode(_)) => Err(trilithon_core::config::EnvError::NotUnicode { key: key.into() }),
        }
    }
    fn vars_with_prefix(&self, prefix: &str) -> Vec<(String, String)> {
        std::env::vars()
            .filter_map(|(k, v)| k.strip_prefix(prefix).map(|s| (s.to_string(), v)))
            .collect()
    }
}
```

### Algorithm

`load_config(path, env)` MUST:

1. Read the file at `path`. If `ErrorKind::NotFound`, return `ConfigError::Missing { path }`. Other I/O errors return `ConfigError::DataDirNotWritable` only when they originate from the data-directory check (step 6).
2. Parse the TOML into a `serde::Deserialize` `DaemonConfigRaw` mirror. Map `toml::de::Error` to `ConfigError::MalformedToml { line, column, source }` using the parser's span.
3. Collect env overrides via `env.vars_with_prefix("TRILITHON_")`. Map keys by lowercasing and replacing `__` with `.` (`TRILITHON_SERVER__BIND` -> `server.bind`).
4. For each override, locate the matching field by dotted path and apply via a `set_by_path(&mut DaemonConfigRaw, &str, &str) -> Result<(), EnvOverrideReason>` helper. Failure produces `ConfigError::EnvOverride { var, reason }`.
5. Convert the validated raw to `DaemonConfig`. Validate the bind address (already typed as `SocketAddr`; mistakes surface during deserialise as `ConfigError::BindAddressInvalid`).
6. Validate `concurrency.rebase_token_ttl_minutes`: if `< 5` or `> 1440`, return `ConfigError::RebaseTtlOutOfBounds { value }`.
7. Validate `storage.data_dir`: stat the directory; if it does not exist, attempt `fs::create_dir_all` once; then attempt to create and remove a temporary file at `data_dir/.trilithon-write-probe`. Failure returns `ConfigError::DataDirNotWritable { path, source }`.
8. Return `Ok(DaemonConfig)`.

### Tests

- `core/crates/adapters/tests/config_loader.rs::happy_path_minimal` — reads `tests/fixtures/minimal.toml`, asserts the resolved `DaemonConfig` has `server.bind == 127.0.0.1:7878`, `concurrency.rebase_token_ttl_minutes == 30`.
- `core/crates/adapters/tests/config_loader.rs::missing_file` — points at `/nonexistent.toml`, asserts `ConfigError::Missing`.
- `core/crates/adapters/tests/config_loader.rs::malformed_toml` — fixture with unclosed table, asserts `ConfigError::MalformedToml { line, column, .. }`.
- `core/crates/adapters/tests/config_loader.rs::env_override_applied` — uses an in-memory `EnvProvider` test double returning `[("server.bind", "127.0.0.1:9000")]`, asserts the override took effect.
- `core/crates/adapters/tests/config_loader.rs::rebase_ttl_boundary_low` — fixture sets `rebase_token_ttl_minutes = 4`, asserts `ConfigError::RebaseTtlOutOfBounds { value: 4 }`.
- `core/crates/adapters/tests/config_loader.rs::rebase_ttl_boundary_low_inclusive` — fixture sets `rebase_token_ttl_minutes = 5`, asserts `Ok`.
- `core/crates/adapters/tests/config_loader.rs::rebase_ttl_boundary_high_inclusive` — fixture sets `rebase_token_ttl_minutes = 1440`, asserts `Ok`.
- `core/crates/adapters/tests/config_loader.rs::rebase_ttl_boundary_high` — fixture sets `rebase_token_ttl_minutes = 1441`, asserts `ConfigError::RebaseTtlOutOfBounds { value: 1441 }`.
- `core/crates/adapters/tests/config_loader.rs::data_dir_not_writable` — fixture pointing at a `chmod 000` directory created in `tempfile::tempdir()`, asserts `ConfigError::DataDirNotWritable`.

### Acceptance command

```
cargo test -p trilithon-adapters --test config_loader
```

### Exit conditions

- `EnvProvider` trait in `core` matches trait-signatures.md §9 byte-for-byte.
- `StdEnvProvider` compiles in `adapters`.
- `load_config` returns `Ok` on the minimal fixture.
- All nine named tests pass.
- `core/Cargo.toml` (the `core` crate manifest) has not gained a runtime I/O dependency.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- Trait signatures §9 (`core::config::EnvProvider`).
- Architecture §5 (layer rules).
- Phase reference: "Implement the TOML configuration loader", "Reject configurations whose data directory is not writable".
- ADR-0011 (loopback default — `allow_remote` field carries the policy).

---

## Slice 1.4 [cross-cutting] — Tracing subscriber, pre-tracing line, UTC-seconds layer

### Goal

Initialise `tracing-subscriber` from `TracingConfig` with environment-variable filter support, JSON output when `TRILITHON_LOG_FORMAT=json` or `TracingConfig::format == LogFormat::Json`, and a custom layer that stamps every event with a `ts_unix_seconds` integer field. Before subscriber installation, Trilithon MUST write the literal line `trilithon: starting (pre-tracing)` to stderr.

### Entry conditions

- Slices 1.1, 1.2 complete.
- `crates/cli/Cargo.toml` declares `tracing = "0.1"`, `tracing-subscriber = { version = "0.3", features = ["env-filter", "json", "fmt"] }`, `time = { version = "0.3", features = ["formatting"] }`.

### Files to create or modify

- `core/crates/cli/src/observability.rs` — subscriber setup (new).
- `core/crates/cli/src/main.rs` — emit pre-tracing line, then call `observability::init` (modify).

### Signatures and shapes

```rust
// core/crates/cli/src/observability.rs
use tracing_subscriber::{prelude::*, EnvFilter};
use trilithon_core::config::{LogFormat, TracingConfig};

/// Install the global subscriber. Must be called exactly once per process.
///
/// # Errors
///
/// Returns `ObsError::AlreadyInstalled` if a subscriber is already global.
pub fn init(config: &TracingConfig) -> Result<(), ObsError>;

#[derive(Debug, thiserror::Error)]
pub enum ObsError {
    #[error("subscriber already installed")]
    AlreadyInstalled,
    #[error("invalid log filter {filter}: {detail}")]
    BadFilter { filter: String, detail: String },
}

/// Layer that injects a `ts_unix_seconds` integer field on every event.
struct UtcSecondsLayer;
```

The subscriber MUST be configured so that:

- `EnvFilter::try_new(&config.log_filter)` provides the directive set.
- The `RUST_LOG` env var, if present, takes precedence (via `EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new(&config.log_filter))`).
- When `config.format == LogFormat::Json`, the formatter is `tracing_subscriber::fmt::layer().json().with_current_span(true).with_span_list(true)`.
- When `config.format == LogFormat::Pretty`, the formatter is `tracing_subscriber::fmt::layer().compact()`.
- The `UtcSecondsLayer` adds an `i64` field named `ts_unix_seconds` derived from `time::OffsetDateTime::now_utc().unix_timestamp()`.

### Algorithm

1. `main()` writes the literal byte sequence `trilithon: starting (pre-tracing)\n` to `std::io::stderr().lock()` and flushes.
2. `main()` parses `Cli`, loads config (slice 1.3), then calls `observability::init(&config.tracing)`.
3. `observability::init` builds the registry: `tracing_subscriber::registry().with(env_filter).with(format_layer).with(UtcSecondsLayer).try_init()`.
4. On failure, return `ObsError::AlreadyInstalled` or `ObsError::BadFilter`.
5. The first event after init is `tracing::info!("daemon.started")` emitted by `main()` after subscriber install. (The `daemon.started` event from §12.1 is emitted here even though graceful-shutdown wiring lands in 1.5; the event name is in the closed vocabulary.)

### Tests

- `core/crates/cli/src/observability.rs` `mod tests::utc_seconds_field_present` — installs a `tracing_test::traced_test` registry with `UtcSecondsLayer`, emits one event, asserts the captured event's fields contain `ts_unix_seconds` as `i64`.
- `core/crates/cli/src/observability.rs` `mod tests::pretty_and_json_dispatch` — calls `init` twice in subprocesses (via `tempfile` + `std::process::Command`) with `LogFormat::Pretty` and `LogFormat::Json`, asserts the captured stderr contains a JSON object on the second run and not on the first. Implementation MAY use the `assert_cmd` crate.
- `core/crates/cli/tests/pre_tracing_line.rs` — runs `cargo run -p trilithon-cli -- run` against a temporary minimal config; reads stderr; asserts the very first line is exactly `trilithon: starting (pre-tracing)`.

### Acceptance command

```
cargo test -p trilithon-cli observability::tests && \
cargo test -p trilithon-cli --test pre_tracing_line
```

### Exit conditions

- `observability::init` is the single subscriber installation site.
- `main()` writes the pre-tracing line on stderr before any subscriber call.
- Setting `TRILITHON_LOG_FORMAT=json` produces JSON-formatted log lines on stderr.
- Every emitted event carries `ts_unix_seconds: i64` (satisfies H6 storage requirement).
- All three named tests pass.

### Audit kinds emitted

None.

### Tracing events emitted

- `daemon.started` (architecture §12.1) — emitted once, after subscriber installation, before entering the run loop.

### Cross-references

- Architecture §12 (observability), §12.1 (event names).
- Phase reference: "Initialise the tracing subscriber before any other startup work", "Emit a fixed pre-filter line on stderr before subscriber init", "Verify wall-clock timestamps are UTC Unix seconds".

---

## Slice 1.5 [cross-cutting] — Signal handling and graceful shutdown

### Goal

Wire `tokio::signal::unix` for `SIGINT` and `SIGTERM`, broadcast a shutdown notification via a `tokio::sync::watch::Sender<bool>`, await up to 10 seconds for owned tasks to drain, then exit `0`. Non-Unix targets MUST fail at compile time with a clear cfg-gated error.

### Entry conditions

- Slices 1.1 through 1.4 complete.
- `crates/cli/Cargo.toml` declares `tokio = { version = "1", features = ["rt-multi-thread", "macros", "signal", "sync", "time"] }`, `anyhow = "1"`.

### Files to create or modify

- `core/crates/cli/src/shutdown.rs` — shutdown plumbing (new).
- `core/crates/cli/src/main.rs` — Tokio runtime, signal listener, drain loop (modify).

### Signatures and shapes

```rust
// core/crates/cli/src/shutdown.rs
use tokio::sync::watch;
use std::time::Duration;

/// Maximum wall-clock budget between SIGINT/SIGTERM receipt and process exit.
pub const DRAIN_BUDGET: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct ShutdownSignal {
    rx: watch::Receiver<bool>,
}

impl ShutdownSignal {
    /// Resolve when shutdown has been requested.
    pub async fn wait(&mut self) -> () { /* await rx.changed() then check */ }
    pub fn is_shutting_down(&self) -> bool { *self.rx.borrow() }
}

pub struct ShutdownController {
    tx: watch::Sender<bool>,
}

impl ShutdownController {
    pub fn new() -> (Self, ShutdownSignal) { /* watch::channel(false) */ }
    pub fn signal(&self) -> ShutdownSignal { /* clone receiver */ }
    pub fn trigger(&self) { let _ = self.tx.send(true); }
}

/// Block on Unix signal handlers until SIGINT or SIGTERM arrives.
#[cfg(unix)]
pub async fn wait_for_signal() -> SignalKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind { Interrupt, Terminate }

#[cfg(not(unix))]
compile_error!("Trilithon V1 supports Unix targets only (Linux, macOS). See ADR-0010.");
```

### Algorithm

1. `main()` enters `#[tokio::main(flavor = "multi_thread")]` (or a hand-built runtime).
2. After `observability::init`, emit `tracing::info!("daemon.started")`.
3. Construct `(controller, signal) = ShutdownController::new()`.
4. Spawn the placeholder daemon work as `tokio::spawn(run_loop(signal.clone()))`. For Phase 1 the run loop simply awaits `signal.wait()` and returns.
5. Concurrently, `tokio::select!` on `wait_for_signal()`:
   - `SignalKind::Interrupt` → emit `tracing::info!("daemon.shutting-down", reason = "sigint")`, then `controller.trigger()`.
   - `SignalKind::Terminate` → emit `tracing::info!("daemon.shutting-down", reason = "sigterm")`, then `controller.trigger()`.
6. Await all spawned tasks under `tokio::time::timeout(DRAIN_BUDGET, future)`. On timeout, emit `tracing::warn!("daemon.shutdown-complete", forced = true)`. On drain, emit `tracing::info!("daemon.shutdown-complete", forced = false)`.
7. Return `ExitCode::CleanShutdown`.

### Tests

- `core/crates/cli/src/shutdown.rs` `mod tests::trigger_observable` — calls `ShutdownController::new()`, spawns a task awaiting `signal.wait()`, calls `trigger()`, asserts the task completes within 100 ms.
- `core/crates/cli/tests/signals.rs::sigterm_drains_within_budget` (gated `#[cfg(unix)]`) — spawns the binary, sends `SIGTERM` via `nix::sys::signal::kill`, captures stderr, asserts:
  1. The exit status is `0`.
  2. Stderr contains the literal substring `daemon.shutting-down`.
  3. The wall-clock time from kill to exit is `< 10` seconds.
- `core/crates/cli/tests/signals.rs::sigint_drains_within_budget` — same as above but `SIGINT`.

### Acceptance command

```
cargo test -p trilithon-cli shutdown::tests && \
cargo test -p trilithon-cli --test signals
```

### Exit conditions

- The daemon exits `0` within 10 seconds of `SIGTERM` or `SIGINT`.
- `daemon.shutting-down` and `daemon.shutdown-complete` events appear in stderr in that order.
- `cargo check -p trilithon-cli --target x86_64-pc-windows-msvc` fails with the literal string `Trilithon V1 supports Unix targets only`.
- The three named tests pass.

### Audit kinds emitted

None.

### Tracing events emitted

- `daemon.shutting-down` (architecture §12.1).
- `daemon.shutdown-complete` (architecture §12.1).

### Cross-references

- Architecture §9 (concurrency model), §12.1 (event vocabulary).
- ADR-0010 (Unix-only deployment scope).
- Phase reference: "Implement graceful shutdown on SIGINT and SIGTERM", "Gate Windows out of V1 explicitly".

---

## Slice 1.6 [standard] — `config show` with redaction

### Goal

Implement the `config show` subcommand. It MUST resolve the configuration via `load_config`, render `DaemonConfig::redacted()` as TOML, and print the result to stdout. The output is captured by `insta` as a snapshot.

### Entry conditions

- Slices 1.1 through 1.3 complete.
- `crates/cli/Cargo.toml` declares `insta = { version = "1", features = ["toml"] }` and `toml = "0.8"`.

### Files to create or modify

- `core/crates/cli/src/cli.rs` — implement `Command::Config { ConfigAction::Show }` handler (modify).
- `core/crates/cli/tests/config_show.rs` — snapshot test (new).
- `core/crates/cli/tests/snapshots/config_show__shows_redacted.snap` — `insta` snapshot fixture (created by first test run).

### Signatures and shapes

```rust
// core/crates/cli/src/cli.rs (excerpt)
fn run_config_show(cli: &Cli) -> Result<i32, anyhow::Error> {
    let env = trilithon_adapters::env_provider::StdEnvProvider;
    let config = trilithon_adapters::config_loader::load_config(&cli.config, &env)
        .map_err(|e| anyhow::anyhow!(e))?;
    let redacted = config.redacted();
    let rendered = toml::to_string_pretty(&redacted)?;
    println!("{rendered}");
    Ok(trilithon_core::exit::ExitCode::CleanShutdown.as_u8() as i32)
}
```

### Algorithm

1. Parse the CLI; on `Command::Config { action: ConfigAction::Show }`, call `run_config_show`.
2. Load config (slice 1.3). On `ConfigError`, exit `2`.
3. Call `config.redacted()`.
4. Serialise via `toml::to_string_pretty(&redacted)`.
5. Print to stdout. Exit `0`.

### Tests

- `core/crates/cli/tests/config_show.rs::shows_redacted` — runs the binary with `--config tests/fixtures/with_secrets.toml`, captures stdout, asserts via `insta::assert_snapshot!`. The fixture contains `bootstrap.credentials_file = "/etc/trilithon/secret-creds.json"`. The snapshot MUST contain `***` and MUST NOT contain `/etc/trilithon/secret-creds.json`.

### Acceptance command

```
cargo test -p trilithon-cli --test config_show
```

### Exit conditions

- `trilithon config show` against `tests/fixtures/with_secrets.toml` prints TOML with secret-bearing fields rendered as `***`.
- The `insta` snapshot is byte-stable across runs.

### Audit kinds emitted

None.

### Tracing events emitted

None for the show path; the daemon does not enter its run loop.

### Cross-references

- Phase reference: "Provide `DaemonConfig::redacted` for safe display", "Snapshot the resolved-configuration output of `config show`".
- Architecture §11 (security posture — secret elision).

---

## Slice 1.7 [standard] — End-to-end exit-code and signal integration tests

### Goal

Lock in the daemon's behaviour at the binary edge: missing-config exit code `2`, malformed-config exit code `2`, signal-driven exit code `0` within budget. This slice closes the phase by exercising every prior slice end-to-end through `assert_cmd` and `nix::sys::signal::kill`.

### Entry conditions

- Slices 1.1 through 1.6 complete.
- `crates/cli/Cargo.toml` declares `assert_cmd = "2"`, `predicates = "3"`, `nix = { version = "0.27", features = ["signal"] }` as dev-dependencies.

### Files to create or modify

- `core/crates/cli/tests/missing_config.rs` — missing/malformed config exit code (new).
- `core/crates/cli/tests/signals.rs` — extended from slice 1.5 (modify).
- `core/crates/cli/tests/utc_timestamps.rs` — assert event timestamps are UTC seconds (new).
- `core/README.md` — add "Running the daemon" and exit-code table (modify).

### Signatures and shapes

```rust
// core/crates/cli/tests/missing_config.rs
#[test]
fn missing_config_exits_2() {
    use assert_cmd::Command;
    let mut cmd = Command::cargo_bin("trilithon").unwrap();
    let assert = cmd.args(["--config", "/nonexistent.toml", "run"])
        .assert()
        .code(2);
    assert.stderr(predicates::str::contains("configuration file not found"));
}
```

The `core/README.md` "Running the daemon" section MUST contain the exit-code table:

| Code | Variant | Meaning |
|------|---------|---------|
| 0 | `CleanShutdown` | Normal exit. |
| 2 | `ConfigError` | Configuration missing, malformed, or invalid. |
| 3 | `StartupPreconditionFailure` | A startup precondition (storage, Caddy reachability) failed. |
| 64 | `InvalidInvocation` | Command-line invocation was malformed. |

### Algorithm

1. `missing_config_exits_2` — runs the binary with a non-existent path, asserts exit code `2`.
2. `malformed_config_exits_2` — uses a fixture with `[server` (truncated header), asserts exit code `2` and stderr contains `MalformedToml`.
3. `signals.rs::sigterm_drains_within_budget` — established in 1.5; extended here to also assert `daemon.shutdown-complete` follows `daemon.shutting-down`.
4. `utc_timestamps.rs::events_carry_unix_seconds` — runs the binary with `TRILITHON_LOG_FORMAT=json`, captures stderr, parses each JSON line, asserts every event carries `ts_unix_seconds` as integer and the value is within 5 seconds of `time::OffsetDateTime::now_utc().unix_timestamp()`.

### Tests

- `missing_config_exits_2`.
- `malformed_config_exits_2`.
- `signals::sigterm_drains_within_budget` (extended).
- `signals::sigint_drains_within_budget` (extended).
- `utc_timestamps::events_carry_unix_seconds`.

### Acceptance command

```
cargo test -p trilithon-cli --tests
```

### Exit conditions

- All five named tests pass on macOS and Linux.
- `core/README.md` contains the "Running the daemon" section, the exit-code table, and a reference to ADR-0011.
- The phase exit checklist (below) is satisfied.

### Audit kinds emitted

None.

### Tracing events emitted

- `daemon.started`, `daemon.shutting-down`, `daemon.shutdown-complete` (verified, not introduced).

### Cross-references

- Phase reference §"Sign-off checklist".
- ADR-0011, ADR-0003.

---

## Phase exit checklist

Tick each line when every slice in the phase has shipped.

- [ ] `just check` passes locally and in continuous integration.
- [ ] Running `trilithon run` against a valid configuration starts the daemon, emits `daemon.started`, and runs until a signal is received.
- [ ] `SIGINT` causes the daemon to emit `daemon.shutting-down`, drain, and exit `0` within 10 seconds.
- [ ] `trilithon run` with a missing configuration file exits with code `2` and a structured error pointing at the missing path.
- [ ] `trilithon config show` prints the resolved configuration with all secret-like fields elided.
- [ ] All wall-clock timestamps in logs are UTC Unix timestamps with timezone-aware rendering (H6).
- [ ] `cargo check -p trilithon-cli --target x86_64-pc-windows-msvc` fails with the documented compile-time error.
