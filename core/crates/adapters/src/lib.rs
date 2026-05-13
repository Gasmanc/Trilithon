//! .-adapters — outside-world wrappers (db, http, fs, env).
//!
//! Depends on `core`. Translates between core types and external systems.

#![forbid(unsafe_code)]

#[cfg(test)]
pub mod test_support;

pub mod applier_caddy;
pub mod audit_notes;
pub use applier_caddy::CaddyApplier;
pub mod audit_writer;
pub use audit_writer::AuditWriter;
pub mod sha256_hasher;
pub use sha256_hasher::Sha256AuditHasher;
pub mod tracing_correlation;
pub use tracing_correlation::{
    CORRELATION_ID_FIELD, CorrelationSpan, correlation_id_from_header, correlation_layer,
    current_correlation_id, with_correlation_span,
};
pub mod caddy;
pub mod config_loader;
pub(crate) mod db_errors;
pub mod drift;
pub mod env_provider;
pub mod integrity_check;
pub mod lock;
pub mod migrate;
pub mod sqlite_storage;
pub mod storage_sqlite;
pub mod tls_observer;
pub use tls_observer::TlsIssuanceObserver;
pub mod http_axum;
pub mod rng;

/// Initialize all adapters.
pub fn boot() {
    tracing::info!("adapters initialised");
}
