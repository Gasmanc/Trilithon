//! The `CaddyClient` trait — async, object-safe, free of I/O crate types.

use async_trait::async_trait;

use crate::caddy::{
    error::CaddyError,
    types::{
        CaddyConfig, CaddyJsonPointer, HealthState, JsonPatch, LoadedModules, TlsCertificate,
        UpstreamHealth,
    },
};

/// Interface for interacting with the Caddy admin API.
///
/// The trait is object-safe: `dyn CaddyClient` is a valid type.  Implementations
/// live in `adapters` and are free to use HTTP clients; this interface is kept
/// free of `hyper` and `reqwest` types so that `core` stays pure.
#[async_trait]
pub trait CaddyClient: Send + Sync + 'static {
    /// Replace the entire running Caddy configuration.
    async fn load_config(&self, body: CaddyConfig) -> Result<(), CaddyError>;

    /// Apply a JSON Patch document to a sub-tree of the running config.
    async fn patch_config(
        &self,
        path: CaddyJsonPointer,
        patch: JsonPatch,
    ) -> Result<(), CaddyError>;

    /// Set the value at `path` using Caddy's `PUT /config/[path]` endpoint.
    ///
    /// This matches Caddy's native semantics: the body is the replacement JSON
    /// value for the addressed config sub-tree. Use this instead of
    /// `patch_config` when creating or replacing a known config path.
    async fn put_config(
        &self,
        path: CaddyJsonPointer,
        value: serde_json::Value,
    ) -> Result<(), CaddyError>;

    /// Retrieve the full running Caddy configuration.
    async fn get_running_config(&self) -> Result<CaddyConfig, CaddyError>;

    /// List all modules currently loaded by Caddy.
    async fn get_loaded_modules(&self) -> Result<LoadedModules, CaddyError>;

    /// Query the health of all configured upstreams.
    async fn get_upstream_health(&self) -> Result<Vec<UpstreamHealth>, CaddyError>;

    /// List all TLS certificates currently managed by Caddy.
    async fn get_certificates(&self) -> Result<Vec<TlsCertificate>, CaddyError>;

    /// Perform a lightweight health check against the Caddy admin endpoint.
    async fn health_check(&self) -> Result<HealthState, CaddyError>;
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::diverging_sub_expression
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use super::CaddyClient;

    /// Compile-time check: `CaddyClient` is object-safe and impls are `Send + Sync + 'static`.
    #[allow(unreachable_code)]
    fn _check() {
        let _: Box<dyn CaddyClient> = panic!("compile-only");
    }

    /// The real check is the compile-time `_check()` above; this test
    /// function exists so the test runner has a named result to report.
    #[test]
    fn trait_is_pure() {}
}
