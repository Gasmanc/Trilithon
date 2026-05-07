//! TOML configuration loader with environment-variable overlay.
//!
//! # Algorithm
//!
//! 1. Read the file at `path`; `NotFound` → [`ConfigError::Missing`],
//!    other I/O errors → [`ConfigError::ReadFailed`].
//! 2. Parse the TOML text directly into a [`toml::Table`]; errors →
//!    [`ConfigError::MalformedToml`].
//! 3. Collect `TRILITHON_*` env vars, map keys (lowercase, `__` → `.`),
//!    apply as dotted-path overrides into the table.
//!    3b. Pre-validate `server.bind` → [`ConfigError::BindAddressInvalid`].
//! 4. Deserialize the (possibly mutated) table into [`DaemonConfig`].
//! 5. Validate `concurrency.rebase_token_ttl_minutes` ∈ \[5, 1440\].
//! 6. Validate `caddy.admin_endpoint` is loopback-only (ADR-0011).
//! 7. Validate `storage.data_dir` is writable (create if absent; write probe).
//! 8. Return `Ok(DaemonConfig)`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use trilithon_core::config::{DaemonConfig, EnvProvider};

use crate::caddy::validate_endpoint::{EndpointPolicyError, validate_loopback_only};

/// Reason an env override could not be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvOverrideReason {
    /// The override value is not valid Unicode.
    NotUnicode,
    /// The key does not map to a known config field.
    UnknownKey,
    /// The value could not be parsed as the expected type.
    ParseFailed {
        /// Human-readable parse error.
        detail: String,
    },
}

/// Errors returned by [`load_config`].
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The configuration file was not found.
    #[error("configuration file not found at {path}")]
    Missing {
        /// The path that was searched.
        path: PathBuf,
    },

    /// The configuration file could not be read (e.g. permission denied).
    #[error("failed to read configuration file at {path}: {source}")]
    ReadFailed {
        /// The path that was attempted.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// The file contains invalid TOML.
    #[error("malformed TOML at line {line}, column {column}: {source}")]
    MalformedToml {
        /// 1-based line number reported by the parser.
        line: usize,
        /// 1-based column number reported by the parser.
        column: usize,
        /// The underlying parser error.
        #[source]
        source: toml::de::Error,
    },

    /// An environment variable override could not be applied.
    #[error("invalid environment override {var}: {reason:?}")]
    EnvOverride {
        /// The `TRILITHON_*` variable name.
        var: String,
        /// Why the override was rejected.
        reason: EnvOverrideReason,
    },

    /// The data directory is not writable.
    #[error("data directory not writable at {path}: {source}")]
    DataDirNotWritable {
        /// The data directory path.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// The rebase TTL is outside the allowed range \[5, 1440\].
    #[error("rebase TTL {value} minutes is outside [5, 1440]")]
    RebaseTtlOutOfBounds {
        /// The out-of-range value.
        value: u32,
    },

    /// The `server.bind` address is not a valid socket address.
    #[error(
        "invalid bind address {value:?}: expected a valid socket address (e.g. 127.0.0.1:7878)"
    )]
    BindAddressInvalid {
        /// The invalid value that was provided.
        value: String,
    },

    /// The Caddy admin endpoint violates the loopback-only policy (ADR-0011).
    #[error("admin endpoint policy violation: {source}")]
    AdminEndpointPolicy {
        /// The underlying policy error.
        #[source]
        source: EndpointPolicyError,
    },
}

/// Load and validate a [`DaemonConfig`] from a TOML file with env overlay.
///
/// # Errors
///
/// See [`ConfigError`] for all failure modes.
pub fn load_config(path: &Path, env: &dyn EnvProvider) -> Result<DaemonConfig, ConfigError> {
    // 1. Read file.
    // NotFound → Missing (caller can offer a first-run setup message).
    // Any other I/O error (EACCES, EIO, …) → ReadFailed (distinct from missing).
    let text = fs::read_to_string(path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            ConfigError::Missing {
                path: path.to_owned(),
            }
        } else {
            ConfigError::ReadFailed {
                path: path.to_owned(),
                source: e,
            }
        }
    })?;

    // 2. Parse TOML text directly into a mutable toml::Table.
    // Parsing to a Table (rather than directly to DaemonConfig) lets us apply
    // env-var overrides before the final deserialization, removing the need for
    // an intermediate DaemonConfig→Table re-serialization round-trip.
    let mut table: toml::Table = toml::from_str(&text).map_err(|e| {
        let span = e.span();
        // toml::de::Error span is a byte range; convert to line/col by
        // counting newlines up to the start of the span.
        let (line, column) = span.map_or((1, 1), |r| byte_offset_to_line_col(&text, r.start));
        ConfigError::MalformedToml {
            line,
            column,
            source: e,
        }
    })?;

    // 3. Apply env overrides.
    let overrides = env.vars_with_prefix("TRILITHON_");
    for (suffix, value) in overrides {
        let dotted_key = suffix.to_lowercase().replace("__", ".");
        if let Err(reason) = set_by_path(&mut table, &dotted_key, &value) {
            if reason != EnvOverrideReason::UnknownKey {
                return Err(ConfigError::EnvOverride {
                    var: format!("TRILITHON_{suffix}"),
                    reason,
                });
            }
        }
    }

    // 3b. Pre-validate server.bind so a bad address produces BindAddressInvalid
    // rather than a generic MalformedToml with no location information.
    if let Some(bind_val) = table
        .get("server")
        .and_then(toml::Value::as_table)
        .and_then(|t| t.get("bind"))
    {
        let bind_str = bind_val.as_str().unwrap_or("");
        bind_str
            .parse::<std::net::SocketAddr>()
            .map_err(|_| ConfigError::BindAddressInvalid {
                value: bind_str.to_owned(),
            })?;
    }

    // 4. Deserialize the (possibly mutated) table into DaemonConfig.
    // Any remaining type mismatch from env overrides surfaces here as
    // MalformedToml. Span information is not available at this stage because
    // toml::de::Error spans refer to the original text positions, not the
    // in-memory table, so we default to (1, 1).
    let config: DaemonConfig =
        toml::Value::Table(table)
            .try_into()
            .map_err(|e: toml::de::Error| ConfigError::MalformedToml {
                line: 1,
                column: 1,
                source: e,
            })?;

    // 5. Validate rebase TTL.
    let ttl = config.concurrency.rebase_token_ttl_minutes;
    if !(5..=1440).contains(&ttl) {
        return Err(ConfigError::RebaseTtlOutOfBounds { value: ttl });
    }

    // 6. Validate admin endpoint is loopback-only (ADR-0011).
    validate_loopback_only(&config.caddy.admin_endpoint)
        .map_err(|source| ConfigError::AdminEndpointPolicy { source })?;

    // 7. Validate data directory writability.
    // create_dir_all is idempotent: it succeeds when the directory already
    // exists, so no existence pre-check is needed (avoids a TOCTOU window).
    let data_dir = &config.storage.data_dir;
    fs::create_dir_all(data_dir).map_err(|e| ConfigError::DataDirNotWritable {
        path: data_dir.clone(),
        source: e,
    })?;
    let probe_path = data_dir.join(".trilithon-write-probe");
    let _ = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&probe_path)
        .map_err(|e| ConfigError::DataDirNotWritable {
            path: data_dir.clone(),
            source: e,
        })?;
    // Best-effort removal; ignore errors.
    let _ = fs::remove_file(&probe_path);

    Ok(config)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a byte offset within `text` to a (1-based line, 1-based column).
fn byte_offset_to_line_col(text: &str, offset: usize) -> (usize, usize) {
    let safe_offset = offset.min(text.len());
    let prefix = &text[..safe_offset];
    let line = prefix.chars().filter(|&c| c == '\n').count() + 1;
    let col = prefix
        .rfind('\n')
        .map_or(safe_offset + 1, |p| safe_offset - p);
    (line, col)
}

/// Apply a string value to a dotted key path inside a [`toml::Table`].
///
/// The parent section (e.g. `[concurrency]`) must already exist in the table;
/// `UnknownKey` is returned if it is absent.  The leaf key need not exist —
/// if absent it is inserted using type inference, so that Serde-defaulted
/// fields (not present in the TOML text) can still be overridden via env vars.
fn set_by_path(
    table: &mut toml::Table,
    dotted_key: &str,
    value: &str,
) -> Result<(), EnvOverrideReason> {
    match dotted_key.split_once('.') {
        None => {
            let parsed = coerce_value(table.get(dotted_key), value)?;
            table.insert(dotted_key.to_string(), parsed);
            Ok(())
        }
        Some((head, rest)) => {
            let subtable = table
                .get_mut(head)
                .and_then(toml::Value::as_table_mut)
                .ok_or(EnvOverrideReason::UnknownKey)?;
            set_by_path(subtable, rest, value)
        }
    }
}

/// Coerce a string to the same TOML variant as the existing value.
///
/// If there is no existing value (field absent from the TOML file), type
/// inference is used: bool → integer → float → string.  If the existing
/// value is a string the new value is kept as a string.  Otherwise the new
/// value is parsed to match the existing variant.
fn coerce_value(
    existing: Option<&toml::Value>,
    raw: &str,
) -> Result<toml::Value, EnvOverrideReason> {
    match existing {
        None => {
            // No existing value: infer type so that Serde-defaulted fields
            // (absent from the TOML text) can still be overridden correctly.
            if let Ok(b) = raw.parse::<bool>() {
                return Ok(toml::Value::Boolean(b));
            }
            if let Ok(i) = raw.parse::<i64>() {
                return Ok(toml::Value::Integer(i));
            }
            if let Ok(f) = raw.parse::<f64>() {
                return Ok(toml::Value::Float(f));
            }
            Ok(toml::Value::String(raw.to_string()))
        }
        // Keep as string if the current field is a string.
        Some(toml::Value::String(_)) => Ok(toml::Value::String(raw.to_string())),
        Some(toml::Value::Integer(_)) => {
            raw.parse::<i64>().map(toml::Value::Integer).map_err(|e| {
                EnvOverrideReason::ParseFailed {
                    detail: e.to_string(),
                }
            })
        }
        Some(toml::Value::Float(_)) => {
            raw.parse::<f64>()
                .map(toml::Value::Float)
                .map_err(|e| EnvOverrideReason::ParseFailed {
                    detail: e.to_string(),
                })
        }
        Some(toml::Value::Boolean(_)) => {
            raw.parse::<bool>().map(toml::Value::Boolean).map_err(|e| {
                EnvOverrideReason::ParseFailed {
                    detail: e.to_string(),
                }
            })
        }
        // For other variants (Array, Table, Datetime) fall back to TOML parse.
        Some(_) => toml::from_str::<toml::Table>(&format!("v = {raw}"))
            .map_err(|e| EnvOverrideReason::ParseFailed {
                detail: e.to_string(),
            })
            .and_then(|mut t| {
                t.remove("v").ok_or_else(|| EnvOverrideReason::ParseFailed {
                    detail: "empty parse result".into(),
                })
            }),
    }
}
