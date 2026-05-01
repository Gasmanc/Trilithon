//! Typed configuration records for the Trilithon daemon.
//!
//! These types carry no I/O, no async, and no filesystem access.
//! They implement `serde::Deserialize` + `Debug` + `Clone` so they can be
//! parsed from TOML/JSON and logged safely via [`DaemonConfig::redacted`].

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use url::Url;

/// Top-level daemon configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// HTTP/TCP listener settings.
    pub server: ServerConfig,
    /// Caddy admin API connection settings.
    pub caddy: CaddyConfig,
    /// Persistent storage settings.
    pub storage: StorageConfig,
    /// Secrets backend settings.
    pub secrets: SecretsConfig,
    /// Concurrency and token settings.
    pub concurrency: ConcurrencyConfig,
    /// Tracing and log settings.
    pub tracing: TracingConfig,
    /// First-run bootstrap settings.
    pub bootstrap: BootstrapConfig,
}

/// HTTP listener configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Default `127.0.0.1:7878`.
    pub bind: SocketAddr,
    /// Default `false`. ADR-0011.
    #[serde(default)]
    pub allow_remote: bool,
}

/// Transport endpoint for the Caddy admin API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum CaddyEndpoint {
    /// Unix-domain socket.
    Unix {
        /// Path to the socket file.
        path: PathBuf,
    },
    /// Loopback HTTPS with mutual TLS.
    LoopbackTls {
        /// Base URL of the admin API.
        url: Url,
        /// Path to the client certificate (secret).
        mtls_cert_path: PathBuf,
        /// Path to the client private key (secret).
        mtls_key_path: PathBuf,
        /// Path to the CA certificate (public PEM).
        mtls_ca_path: PathBuf,
    },
}

/// Caddy admin API connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaddyConfig {
    /// Transport endpoint for the Caddy admin API.
    pub admin_endpoint: CaddyEndpoint,
    /// Seconds to wait for a connection. Default 10.
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_seconds: u32,
    /// Seconds to wait for a config apply. Default 60.
    #[serde(default = "default_apply_timeout")]
    pub apply_timeout_seconds: u32,
}

const fn default_connect_timeout() -> u32 {
    10
}

const fn default_apply_timeout() -> u32 {
    60
}

/// Persistent storage configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Directory for `SQLite` database and related files.
    pub data_dir: PathBuf,
    /// WAL checkpoint threshold in pages. Default 1000.
    #[serde(default = "default_wal_pages")]
    pub wal_checkpoint_pages: u32,
}

const fn default_wal_pages() -> u32 {
    1000
}

/// Secrets backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    /// Backend used to store the master encryption key.
    pub master_key_backend: SecretsBackend,
}

/// Supported secrets backends.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum SecretsBackend {
    /// OS keychain (macOS Keychain, etc.).
    Keychain,
    /// Plaintext file (secret path).
    File {
        /// Path to the key file (secret).
        path: PathBuf,
    },
}

/// Concurrency and token configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrencyConfig {
    /// Rebase token TTL in minutes. Default 30; bounds \[5, 1440\].
    #[serde(default = "default_rebase_ttl")]
    pub rebase_token_ttl_minutes: u32,
}

const fn default_rebase_ttl() -> u32 {
    30
}

/// Tracing and logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    /// `tracing-subscriber` filter string. Default `"info,trilithon=info"`.
    #[serde(default = "default_log_filter")]
    pub log_filter: String,
    /// Log output format. Default [`LogFormat::Pretty`].
    #[serde(default)]
    pub format: LogFormat,
}

fn default_log_filter() -> String {
    "info,trilithon=info".into()
}

/// Log output format.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Human-readable multi-line format.
    #[default]
    Pretty,
    /// Structured JSON lines.
    Json,
}

/// First-run bootstrap configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapConfig {
    /// Run bootstrap on first start. Default `true`.
    #[serde(default = "default_bootstrap_enabled")]
    pub enabled_on_first_run: bool,
    /// Path to the bootstrap credentials file (secret).
    /// Default `/var/lib/trilithon/bootstrap.json`.
    #[serde(default = "default_bootstrap_credentials")]
    pub credentials_file: PathBuf,
}

const fn default_bootstrap_enabled() -> bool {
    true
}

fn default_bootstrap_credentials() -> PathBuf {
    PathBuf::from("/var/lib/trilithon/bootstrap.json")
}

impl DaemonConfig {
    /// Return a redacted `Display`-ready view; every secret-like field is
    /// rendered as the literal string `***`.
    pub fn redacted(&self) -> RedactedConfig {
        RedactedConfig::from(self)
    }
}

// ---------------------------------------------------------------------------
// Redacted mirror
// ---------------------------------------------------------------------------

/// Redacted view of [`DaemonConfig`] safe for display and logging.
///
/// Secret-bearing paths are replaced with `"***"`.
#[derive(Debug, Clone, Serialize)]
pub struct RedactedConfig {
    /// Redacted server config.
    pub server: RedactedServerConfig,
    /// Redacted Caddy config.
    pub caddy: RedactedCaddyConfig,
    /// Storage config (no secrets).
    pub storage: StorageConfig,
    /// Redacted secrets config.
    pub secrets: RedactedSecretsConfig,
    /// Concurrency config (no secrets).
    pub concurrency: ConcurrencyConfig,
    /// Tracing config (no secrets).
    pub tracing: TracingConfig,
    /// Redacted bootstrap config.
    pub bootstrap: RedactedBootstrapConfig,
}

/// Redacted mirror of [`ServerConfig`].
#[derive(Debug, Clone, Serialize)]
pub struct RedactedServerConfig {
    /// Bind address.
    pub bind: SocketAddr,
    /// Whether remote access is allowed.
    pub allow_remote: bool,
}

/// Redacted mirror of [`CaddyConfig`].
#[derive(Debug, Clone, Serialize)]
pub struct RedactedCaddyConfig {
    /// Redacted transport endpoint.
    pub admin_endpoint: RedactedCaddyEndpoint,
    /// Connect timeout.
    pub connect_timeout_seconds: u32,
    /// Apply timeout.
    pub apply_timeout_seconds: u32,
}

/// Redacted mirror of [`CaddyEndpoint`].
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum RedactedCaddyEndpoint {
    /// Unix socket — no secrets.
    Unix {
        /// Path to the socket file.
        path: PathBuf,
    },
    /// Loopback TLS — cert and key paths redacted.
    LoopbackTls {
        /// Base URL.
        url: Url,
        /// Redacted client certificate path.
        mtls_cert_path: &'static str,
        /// Redacted client private key path.
        mtls_key_path: &'static str,
        /// CA certificate path (public PEM, not redacted).
        mtls_ca_path: PathBuf,
    },
}

/// Redacted mirror of [`SecretsConfig`].
#[derive(Debug, Clone, Serialize)]
pub struct RedactedSecretsConfig {
    /// Redacted backend.
    pub master_key_backend: RedactedSecretsBackend,
}

/// Redacted mirror of [`SecretsBackend`].
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum RedactedSecretsBackend {
    /// Keychain — no secrets exposed.
    Keychain,
    /// File backend — path redacted.
    File {
        /// Redacted path.
        path: &'static str,
    },
}

/// Redacted mirror of [`BootstrapConfig`].
#[derive(Debug, Clone, Serialize)]
pub struct RedactedBootstrapConfig {
    /// Whether bootstrap runs on first start.
    pub enabled_on_first_run: bool,
    /// Credentials file path — redacted.
    pub credentials_file: &'static str,
}

impl From<&DaemonConfig> for RedactedConfig {
    fn from(cfg: &DaemonConfig) -> Self {
        Self {
            server: RedactedServerConfig {
                bind: cfg.server.bind,
                allow_remote: cfg.server.allow_remote,
            },
            caddy: RedactedCaddyConfig {
                admin_endpoint: match &cfg.caddy.admin_endpoint {
                    CaddyEndpoint::Unix { path } => {
                        RedactedCaddyEndpoint::Unix { path: path.clone() }
                    }
                    CaddyEndpoint::LoopbackTls {
                        url, mtls_ca_path, ..
                    } => RedactedCaddyEndpoint::LoopbackTls {
                        url: url.clone(),
                        mtls_cert_path: "***",
                        mtls_key_path: "***",
                        mtls_ca_path: mtls_ca_path.clone(),
                    },
                },
                connect_timeout_seconds: cfg.caddy.connect_timeout_seconds,
                apply_timeout_seconds: cfg.caddy.apply_timeout_seconds,
            },
            storage: cfg.storage.clone(),
            secrets: RedactedSecretsConfig {
                master_key_backend: match &cfg.secrets.master_key_backend {
                    SecretsBackend::Keychain => RedactedSecretsBackend::Keychain,
                    SecretsBackend::File { .. } => RedactedSecretsBackend::File { path: "***" },
                },
            },
            concurrency: cfg.concurrency.clone(),
            tracing: cfg.tracing.clone(),
            bootstrap: RedactedBootstrapConfig {
                enabled_on_first_run: cfg.bootstrap.enabled_on_first_run,
                credentials_file: "***",
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use super::*;

    fn minimal_toml() -> &'static str {
        include_str!("../../tests/fixtures/minimal.toml")
    }

    #[test]
    fn defaults_match_doc() {
        let cfg: DaemonConfig = toml::from_str(minimal_toml()).expect("minimal.toml must parse");

        // ServerConfig defaults
        assert_eq!(cfg.server.bind.to_string(), "127.0.0.1:7878", "server.bind");
        assert!(
            !cfg.server.allow_remote,
            "server.allow_remote default false"
        );

        // CaddyConfig defaults
        assert_eq!(
            cfg.caddy.connect_timeout_seconds, 10,
            "caddy.connect_timeout_seconds"
        );
        assert_eq!(
            cfg.caddy.apply_timeout_seconds, 60,
            "caddy.apply_timeout_seconds"
        );

        // CaddyEndpoint::Unix path
        match &cfg.caddy.admin_endpoint {
            CaddyEndpoint::Unix { path } => {
                assert_eq!(
                    path.to_string_lossy(),
                    "/run/caddy/admin.sock",
                    "caddy.admin_endpoint.path"
                );
            }
            CaddyEndpoint::LoopbackTls { .. } => {
                panic!("expected Unix endpoint, got LoopbackTls");
            }
        }

        // StorageConfig defaults
        assert_eq!(
            cfg.storage.wal_checkpoint_pages, 1000,
            "storage.wal_checkpoint_pages"
        );

        // ConcurrencyConfig defaults
        assert_eq!(
            cfg.concurrency.rebase_token_ttl_minutes, 30,
            "concurrency.rebase_token_ttl_minutes"
        );

        // TracingConfig defaults
        assert_eq!(
            cfg.tracing.log_filter, "info,trilithon=info",
            "tracing.log_filter"
        );

        // BootstrapConfig defaults
        assert!(
            cfg.bootstrap.enabled_on_first_run,
            "bootstrap.enabled_on_first_run"
        );
        assert_eq!(
            cfg.bootstrap.credentials_file,
            std::path::PathBuf::from("/var/lib/trilithon/bootstrap.json"),
            "bootstrap.credentials_file"
        );
    }

    #[test]
    fn redacted_elides_secret_paths() {
        let cfg = DaemonConfig {
            server: ServerConfig {
                bind: "127.0.0.1:7878".parse().expect("valid addr"),
                allow_remote: false,
            },
            caddy: CaddyConfig {
                admin_endpoint: CaddyEndpoint::Unix {
                    path: PathBuf::from("/run/caddy/admin.sock"),
                },
                connect_timeout_seconds: 10,
                apply_timeout_seconds: 60,
            },
            storage: StorageConfig {
                data_dir: PathBuf::from("/var/lib/trilithon"),
                wal_checkpoint_pages: 1000,
            },
            secrets: SecretsConfig {
                master_key_backend: SecretsBackend::Keychain,
            },
            concurrency: ConcurrencyConfig {
                rebase_token_ttl_minutes: 30,
            },
            tracing: TracingConfig {
                log_filter: default_log_filter(),
                format: LogFormat::Pretty,
            },
            bootstrap: BootstrapConfig {
                enabled_on_first_run: true,
                credentials_file: PathBuf::from("/etc/secret"),
            },
        };

        let redacted = cfg.redacted();
        let json = serde_json::to_string(&redacted).expect("redacted must serialise");

        assert!(json.contains("***"), "redacted JSON must contain ***");
        assert!(
            !json.contains("/etc/secret"),
            "redacted JSON must not contain /etc/secret"
        );
    }
}
