//! `EnvProvider` trait and associated error type.
//!
//! Abstracts environment-variable access so adapters and tests can supply
//! different implementations without coupling to `std::env`.

use thiserror::Error;

/// Provides access to environment variables.
///
/// The trait is object-safe; implementors must be `Send + Sync + 'static` so
/// they can be shared across threads and stored in long-lived structures.
pub trait EnvProvider: Send + Sync + 'static {
    /// Return the value of the environment variable `key`.
    ///
    /// # Errors
    ///
    /// Returns [`EnvError::NotPresent`] when the variable is absent, or
    /// [`EnvError::NotUnicode`] when its value is not valid UTF-8.
    fn var(&self, key: &str) -> Result<String, EnvError>;

    /// Return all environment variables whose names start with `prefix`,
    /// with the prefix stripped from the returned key.
    fn vars_with_prefix(&self, prefix: &str) -> Vec<(String, String)>;
}

/// Errors that can arise when querying an environment variable.
#[derive(Debug, Error)]
pub enum EnvError {
    /// The variable is not set.
    #[error("environment variable {key} is not present")]
    NotPresent {
        /// The variable name.
        key: String,
    },
    /// The variable is set but its value is not valid Unicode.
    #[error("environment variable {key} is not valid Unicode")]
    NotUnicode {
        /// The variable name.
        key: String,
    },
}
