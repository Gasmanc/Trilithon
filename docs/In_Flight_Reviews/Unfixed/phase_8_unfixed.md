## Slice 8.1
- **Pre-existing gate failure: `caddy_sentinel_e2e`** — `trilithon-adapters` test `caddy_sentinel_e2e` fails to compile with `can't find crate for trilithon_core` / `can't find crate for tokio`. Added in Phase 3 (commit e55bb18); not caused by Slice 8.1. The test in `crates/adapters/Cargo.toml` lacks a `required-features` guard. Deferred to a cleanup slice.
