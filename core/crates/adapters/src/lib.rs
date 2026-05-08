//! .-adapters — outside-world wrappers (db, http, fs, env).
//!
//! Depends on `core`. Translates between core types and external systems.

#![forbid(unsafe_code)]

#[cfg(test)]
pub mod test_support;

pub mod caddy;
pub mod config_loader;
pub(crate) mod db_errors;
pub mod env_provider;
pub mod integrity_check;
pub mod lock;
pub mod migrate;
pub mod sqlite_storage;
pub mod storage_sqlite;

/// Initialize all adapters.
pub fn boot() {
    tracing::info!("adapters initialised");
}
