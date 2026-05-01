//! TOML configuration loader with environment-variable overlay.
//!
//! # Algorithm
//!
//! 1. Read the file at `path`; missing → [`ConfigError::Missing`].
//! 2. Parse into [`DaemonConfig`] via TOML; errors → [`ConfigError::MalformedToml`].
//! 3. Re-serialize to a [`toml::Table`] so env overrides can be applied
//!    as dotted-key mutations.
//! 4. Collect `TRILITHON_*` env vars, map keys (lowercase, `__` → `.`),
//!    apply as dotted-path overrides into the table.
//! 5. Re-deserialize the mutated table back to [`DaemonConfig`].
//! 6. Validate `concurrency.rebase_token_ttl_minutes` ∈ \[5, 1440\].
//! 7. Validate `storage.data_dir` is writable (create if absent; write probe).
//! 8. Return `Ok(DaemonConfig)`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use trilithon_core::config::{DaemonConfig, EnvProvider};

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

    /// The bind address string is not a valid `SocketAddr`.
    #[error("bind address {value} is invalid")]
    BindAddressInvalid {
        /// The raw string value.
        value: String,
    },

    /// The rebase TTL is outside the allowed range \[5, 1440\].
    #[error("rebase TTL {value} minutes is outside [5, 1440]")]
    RebaseTtlOutOfBounds {
        /// The out-of-range value.
        value: u32,
    },
}

/// Load and validate a [`DaemonConfig`] from a TOML file with env overlay.
///
/// # Errors
///
/// See [`ConfigError`] for all failure modes.
pub fn load_config(path: &Path, env: &dyn EnvProvider) -> Result<DaemonConfig, ConfigError> {
    // 1. Read file.
    // All read errors on the config file itself are reported as Missing; only
    // the data-dir probe (step 7) uses DataDirNotWritable.
    let text = fs::read_to_string(path).map_err(|_| ConfigError::Missing {
        path: path.to_owned(),
    })?;

    // 2. Parse TOML → DaemonConfig.
    let config: DaemonConfig = toml::from_str(&text).map_err(|e| {
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

    // 3. Re-serialize to a mutable toml::Table so we can apply env overrides.
    // Safety: DaemonConfig derives Serialize; serialising a just-deserialized
    // value cannot fail in practice.
    let mut table: toml::Table = toml::Value::try_from(&config)
        .unwrap_or_else(|_| toml::Value::Table(toml::Table::new()))
        .try_into()
        .unwrap_or_default();

    // 4. Apply env overrides.
    let overrides = env.vars_with_prefix("TRILITHON_");
    for (suffix, value) in overrides {
        // Map TRILITHON_SERVER__BIND → server.bind
        let dotted_key = suffix.to_lowercase().replace("__", ".");
        // Unknown keys are silently skipped: build-time or deployment env
        // vars with the TRILITHON_ prefix (e.g. TRILITHON_GIT_SHORT_HASH)
        // must not cause a config error.
        if let Err(reason) = set_by_path(&mut table, &dotted_key, &value) {
            if reason != EnvOverrideReason::UnknownKey {
                return Err(ConfigError::EnvOverride {
                    var: format!("TRILITHON_{suffix}"),
                    reason,
                });
            }
        }
    }

    // 5. Re-deserialize the (possibly mutated) table back to DaemonConfig.
    let config: DaemonConfig =
        toml::Value::Table(table)
            .try_into()
            .map_err(|e: toml::de::Error| {
                // A failed override (e.g. bad bind address) surfaces here.
                let msg = e.to_string();
                if msg.contains("bind") || msg.contains("SocketAddr") || msg.contains("address") {
                    // Extract the bad value from the error message if possible.
                    ConfigError::BindAddressInvalid { value: msg }
                } else {
                    let span = e.span();
                    let (line, column) =
                        span.map_or((1, 1), |r| byte_offset_to_line_col("", r.start));
                    ConfigError::MalformedToml {
                        line,
                        column,
                        source: e,
                    }
                }
            })?;

    // 6. Validate rebase TTL.
    let ttl = config.concurrency.rebase_token_ttl_minutes;
    if !(5..=1440).contains(&ttl) {
        return Err(ConfigError::RebaseTtlOutOfBounds { value: ttl });
    }

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
/// The value is inserted as a TOML string token, then the table entry is
/// re-parsed to the correct type (integer, bool, etc.) by trying TOML
/// value parsing. If the key path does not exist in the original table the
/// override is rejected as [`EnvOverrideReason::UnknownKey`].
fn set_by_path(
    table: &mut toml::Table,
    dotted_key: &str,
    value: &str,
) -> Result<(), EnvOverrideReason> {
    match dotted_key.split_once('.') {
        None => {
            // Leaf key — must already exist in the table.
            if !table.contains_key(dotted_key) {
                return Err(EnvOverrideReason::UnknownKey);
            }
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
/// If the existing value is a string we keep the new value as a string.
/// Otherwise we attempt to parse via `toml::from_str` using a dummy key.
fn coerce_value(
    existing: Option<&toml::Value>,
    raw: &str,
) -> Result<toml::Value, EnvOverrideReason> {
    match existing {
        // Keep as string if the current field is a string.
        Some(toml::Value::String(_)) | None => Ok(toml::Value::String(raw.to_string())),
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
