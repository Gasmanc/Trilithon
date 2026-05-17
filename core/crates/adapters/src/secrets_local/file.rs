//! File-based master-key backend.
//!
//! Used when the OS keychain is unavailable. The master key is stored in
//! `<data_dir>/master-key` with mode `0600`. A startup warning recommends
//! backing up the file out-of-band.
//!
//! # File format
//!
//! Each entry is two lines:
//! ```text
//! version=N
//! key=<base64>
//! ```
//! Multiple entries may be appended by `rotate`. On read, the entry with
//! the highest version number is returned; earlier entries are retained for
//! re-encryption.

use std::io::Write as _;
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::PathBuf;

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use trilithon_core::secrets::CryptoError;

use super::MasterKeyBackend;

// ── FileBackend ───────────────────────────────────────────────────────────────

/// File-based master-key backend.
///
/// Stores the 256-bit master key in `<data_dir>/master-key` with mode
/// `0600`.
pub struct FileBackend {
    /// Path to the key file; typically `<data_dir>/master-key`.
    pub path: PathBuf,
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Parse all `(version, base64_key)` entries from the file content.
fn parse_entries(content: &str) -> Result<Vec<(u32, String)>, CryptoError> {
    let mut entries = Vec::new();
    let mut lines = content.lines().peekable();
    while lines.peek().is_some() {
        let version_line = match lines.next() {
            Some(l) if !l.is_empty() => l,
            _ => continue,
        };
        let key_line = lines.next().ok_or_else(|| CryptoError::Decryption {
            detail: "key file is truncated: missing key= line".to_string(),
        })?;

        let version: u32 = version_line
            .strip_prefix("version=")
            .ok_or_else(|| CryptoError::Decryption {
                detail: format!("unexpected line in key file: {version_line:?}"),
            })?
            .parse()
            .map_err(|e| CryptoError::Decryption {
                detail: format!("cannot parse version number: {e}"),
            })?;

        let b64 = key_line
            .strip_prefix("key=")
            .ok_or_else(|| CryptoError::Decryption {
                detail: format!("unexpected line in key file: {key_line:?}"),
            })?
            .to_owned();

        entries.push((version, b64));
    }
    Ok(entries)
}

/// Decode a base64-encoded 32-byte key.
fn decode_key(b64: &str) -> Result<[u8; 32], CryptoError> {
    let bytes = BASE64
        .decode(b64.trim())
        .map_err(|e| CryptoError::Decryption {
            detail: format!("base64 decode failed: {e}"),
        })?;
    bytes.try_into().map_err(|_| CryptoError::Decryption {
        detail: "stored key is not 32 bytes".to_string(),
    })
}

/// Generate 32 random bytes.
fn generate_key() -> Result<[u8; 32], CryptoError> {
    let mut key = [0u8; 32];
    getrandom::getrandom(&mut key).map_err(|e| CryptoError::KeyringUnavailable {
        detail: format!("getrandom failed: {e}"),
    })?;
    Ok(key)
}

// ── MasterKeyBackend impl ─────────────────────────────────────────────────────

#[async_trait]
impl MasterKeyBackend for FileBackend {
    /// Load the master key from the file, or generate and persist a new one.
    ///
    /// If the file exists but has permissions more permissive than `0600`, the
    /// permissions are tightened and a warning is emitted.
    async fn load_or_generate(&self) -> Result<[u8; 32], CryptoError> {
        // Always emit the startup warning so callers know the key is on disk.
        tracing::warn!(
            target: "secrets.file-backend.startup",
            path = %self.path.display(),
            "the master key is on disk; back up {} out-of-band",
            self.path.display(),
        );

        if self.path.exists() {
            // Check and tighten permissions if needed.
            let meta = std::fs::metadata(&self.path).map_err(|e| CryptoError::Decryption {
                detail: format!("cannot stat key file: {e}"),
            })?;
            let mode = meta.permissions().mode() & 0o777;
            if mode != 0o600 {
                std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600))
                    .map_err(|e| CryptoError::Decryption {
                        detail: format!("cannot chmod key file: {e}"),
                    })?;
                tracing::warn!(
                    target: "secrets.master-key.permissions-tightened",
                    path = %self.path.display(),
                    old_mode = format!("{mode:04o}"),
                    "master-key file had permissive mode; tightened to 0600",
                );
            }

            let content =
                std::fs::read_to_string(&self.path).map_err(|e| CryptoError::Decryption {
                    detail: format!("cannot read key file: {e}"),
                })?;
            let entries = parse_entries(&content)?;
            let best = entries.into_iter().max_by_key(|(v, _)| *v).ok_or_else(|| {
                CryptoError::Decryption {
                    detail: "key file exists but contains no entries".to_string(),
                }
            })?;
            return decode_key(&best.1);
        }

        // File does not exist — generate a new key.
        let key = generate_key()?;
        let b64 = BASE64.encode(key);
        let content = format!("version=1\nkey={b64}\n");

        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&self.path)
            .map_err(|e| CryptoError::KeyringUnavailable {
                detail: format!("cannot create key file {:?}: {e}", self.path),
            })?;
        file.write_all(content.as_bytes())
            .map_err(|e| CryptoError::KeyringUnavailable {
                detail: format!("cannot write key file: {e}"),
            })?;
        file.sync_all()
            .map_err(|e| CryptoError::KeyringUnavailable {
                detail: format!("sync_all failed: {e}"),
            })?;

        Ok(key)
    }

    /// Append a new versioned key entry and return `(new_key, new_version)`.
    async fn rotate(&self) -> Result<([u8; 32], u32), CryptoError> {
        // Read the current highest version.
        let current_version = if self.path.exists() {
            let content =
                std::fs::read_to_string(&self.path).map_err(|e| CryptoError::Decryption {
                    detail: format!("cannot read key file: {e}"),
                })?;
            let entries = parse_entries(&content)?;
            entries.into_iter().map(|(v, _)| v).max().unwrap_or(0)
        } else {
            0
        };
        let new_version = current_version + 1;

        let new_key = generate_key()?;
        let b64 = BASE64.encode(new_key);
        let entry = format!("version={new_version}\nkey={b64}\n");

        // Append the new entry (create if not present).
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&self.path)
            .map_err(|e| CryptoError::KeyringUnavailable {
                detail: format!("cannot open key file for append: {e}"),
            })?;
        file.write_all(entry.as_bytes())
            .map_err(|e| CryptoError::KeyringUnavailable {
                detail: format!("cannot append to key file: {e}"),
            })?;
        file.sync_all()
            .map_err(|e| CryptoError::KeyringUnavailable {
                detail: format!("sync_all failed: {e}"),
            })?;

        Ok((new_key, new_version))
    }

    fn kind(&self) -> &'static str {
        "file"
    }
}
