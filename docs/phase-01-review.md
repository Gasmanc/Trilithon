## Slice 1.3
**Status:** complete
**Summary:** Implemented `EnvProvider` trait and `EnvError` in `core/crates/core/src/config/env.rs`, with `StdEnvProvider` in `adapters`. Created `config_loader.rs` with `load_config` that reads a TOML file, overlays `TRILITHON_*` env vars via dotted-key mutation of a `toml::Table`, validates `rebase_token_ttl_minutes ∈ [5, 1440]`, and checks data-directory writability via a write probe. Nine integration tests all pass.

### Simplify Findings
- Removed redundant `if !data_dir.exists()` guard before `fs::create_dir_all` — `create_dir_all` is already idempotent and the guard introduced a TOCTOU window.
- Replaced `splitn(2, '.').collect::<Vec<_>>()` + slice pattern in `set_by_path` with `split_once('.')` — eliminates a `Vec` allocation per key segment.

### Fixes Applied
1. Gate (clippy): replaced duplicate `if/else` branches in file-read error mapping with a single `map_err` closure.
2. Gate (clippy): used `map_or` instead of `.map(...).unwrap_or(...)` in two TOML span conversions.
3. Gate (clippy): replaced `ttl < 5 || ttl > 1440` with `!(5..=1440).contains(&ttl)`.
4. Gate (clippy): added `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::disallowed_methods)]` to test file; made `MapEnvProvider::empty` a `const fn`; moved `use` imports to top of test to avoid "items after statements" warning.
5. Gate (nix): added `user` feature to nix dev-dep to expose `nix::unistd::getuid`.

## Slice 1.2
**Status:** complete
**Summary:** Defined all `DaemonConfig` typed records (`ServerConfig`, `CaddyConfig`, `CaddyEndpoint`, `StorageConfig`, `SecretsConfig`, `ConcurrencyConfig`, `TracingConfig`, `BootstrapConfig`) in `core/crates/core/src/config/types.rs` with full serde support and documented defaults. Implemented `DaemonConfig::redacted()` via a `RedactedConfig` mirror that replaces four secret-bearing paths with `"***"`. Both required tests pass.

### Simplify Findings
- Renamed `config/mod.rs` to `config.rs` (inline module file) to satisfy `mod_module_files` clippy lint — no other refactor needed.

### Fixes Applied
1. `cargo fmt` reformatted three blocks in `types.rs` (trailing brace placement, assert_eq argument width).
2. Default functions made `const fn` where return types are primitive (`u32`, `bool`).
3. `#[allow(clippy::disallowed_methods)]` added to `#[cfg(test)] mod tests` block to permit `.expect()` and `.parse().expect()` in test-only code; `panic!` in exhaustive match arm also allowed.
4. Doc comment `SQLite` wrapped in backticks to satisfy `doc_markdown` lint.
