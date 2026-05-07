//! [`StdEnvProvider`] — reads environment variables from the real process
//! environment via [`std::env`].

use trilithon_core::config::{EnvError, EnvProvider};

/// An [`EnvProvider`] backed by the real process environment.
pub struct StdEnvProvider;

impl EnvProvider for StdEnvProvider {
    fn var(&self, key: &str) -> Result<String, EnvError> {
        match std::env::var(key) {
            Ok(v) => Ok(v),
            Err(std::env::VarError::NotPresent) => Err(EnvError::NotPresent { key: key.into() }),
            Err(std::env::VarError::NotUnicode(_)) => Err(EnvError::NotUnicode { key: key.into() }),
        }
    }

    fn vars_with_prefix(&self, prefix: &str) -> Vec<(String, String)> {
        // `std::env::vars()` silently skips non-Unicode env vars, so this method
        // will not return non-Unicode `TRILITHON_*` variables even though `var()`
        // returns `EnvError::NotUnicode` for them.  The discrepancy is intentional:
        // non-Unicode config overrides cannot be applied and are silently ignored.
        std::env::vars()
            .filter_map(|(k, v)| k.strip_prefix(prefix).map(|s| (s.to_string(), v)))
            .collect()
    }
}
