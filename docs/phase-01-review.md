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
