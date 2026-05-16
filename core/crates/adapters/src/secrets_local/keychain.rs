//! OS-keychain master-key backend.
//!
//! On macOS this uses the Keychain; on Linux it uses the Secret Service API
//! via D-Bus (provided by the `keyring` crate's platform layer).
//!
//! # First-run behaviour
//!
//! If no entry is found for the configured service/account pair, `load_or_generate`
//! generates 32 random bytes from the OS CSPRNG, base64-encodes them, and stores
//! the result in the keychain before returning the raw bytes.
//!
//! # Platform-failure handling
//!
//! Any `keyring::Error::PlatformFailure` surfaces as
//! [`CryptoError::KeyringUnavailable`] so the caller (vault constructor) can
//! fall back to a `FileBackend`.

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use trilithon_core::secrets::CryptoError;

// ── MasterKeyBackend ──────────────────────────────────────────────────────────

/// Trait that abstracts over keychain and file-based master-key storage.
#[async_trait]
pub trait MasterKeyBackend: Send + Sync + 'static {
    /// Load the master key from the backend, generating and storing a new one
    /// if none exists.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::KeyringUnavailable`] if the OS keychain is not
    /// accessible, or [`CryptoError::Decryption`] if the stored value cannot
    /// be decoded.
    async fn load_or_generate(&self) -> Result<[u8; 32], CryptoError>;

    /// Generate a new master key and store it under a versioned account name.
    ///
    /// Returns the new key bytes and the new version number.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::KeyringUnavailable`] if the OS keychain is not
    /// accessible.
    async fn rotate(&self) -> Result<([u8; 32], u32), CryptoError>;

    /// Returns a short string identifying the backend kind.
    ///
    /// `"keychain"` for [`KeychainBackend`]; `"file"` for a file-based backend.
    fn kind(&self) -> &'static str;
}

// ── KeychainBackend ───────────────────────────────────────────────────────────

/// OS-keychain master-key backend.
///
/// Stores the 256-bit master key as a base64-encoded password under
/// `service` / `account` in the platform credential store.
pub struct KeychainBackend {
    /// Service name passed to the keyring entry; typically `"trilithon"`.
    pub service: &'static str,
    /// Account name; typically `"master-key-v1"` for version 1.
    pub account: String,
}

impl KeychainBackend {
    /// Parse the version number embedded in the account string `"master-key-v{n}"`.
    ///
    /// Returns `None` if the account name does not match the expected pattern.
    fn version_from_account(account: &str) -> Option<u32> {
        account.strip_prefix("master-key-v")?.parse().ok()
    }

    /// Build an account name for the given version.
    fn account_for_version(version: u32) -> String {
        format!("master-key-v{version}")
    }

    /// Generate 32 random bytes and store them in the keychain under `entry`.
    fn generate_and_store(entry: &keyring_core::Entry) -> Result<[u8; 32], CryptoError> {
        let mut key = [0u8; 32];
        getrandom::getrandom(&mut key).map_err(|e| CryptoError::KeyringUnavailable {
            detail: format!("getrandom failed: {e}"),
        })?;
        let encoded = BASE64.encode(key);
        entry
            .set_password(&encoded)
            .map_err(|e| CryptoError::KeyringUnavailable {
                detail: e.to_string(),
            })?;
        Ok(key)
    }

    /// Decode a base64-encoded 32-byte key from `s`.
    fn decode_key(s: &str) -> Result<[u8; 32], CryptoError> {
        let bytes = BASE64
            .decode(s.trim())
            .map_err(|e| CryptoError::Decryption {
                detail: format!("base64 decode failed: {e}"),
            })?;
        bytes.try_into().map_err(|_| CryptoError::Decryption {
            detail: "stored key is not 32 bytes".to_string(),
        })
    }
}

#[async_trait]
impl MasterKeyBackend for KeychainBackend {
    async fn load_or_generate(&self) -> Result<[u8; 32], CryptoError> {
        let entry = keyring_core::Entry::new(self.service, &self.account).map_err(|e| {
            CryptoError::KeyringUnavailable {
                detail: e.to_string(),
            }
        })?;

        match entry.get_password() {
            Ok(s) => Self::decode_key(&s),
            Err(keyring_core::Error::NoEntry) => Self::generate_and_store(&entry),
            Err(keyring_core::Error::PlatformFailure(e)) => Err(CryptoError::KeyringUnavailable {
                detail: e.to_string(),
            }),
            Err(e) => Err(CryptoError::KeyringUnavailable {
                detail: e.to_string(),
            }),
        }
    }

    async fn rotate(&self) -> Result<([u8; 32], u32), CryptoError> {
        let current_version = Self::version_from_account(&self.account).unwrap_or(1);
        let new_version = current_version + 1;
        let new_account = Self::account_for_version(new_version);

        let entry = keyring_core::Entry::new(self.service, &new_account).map_err(|e| {
            CryptoError::KeyringUnavailable {
                detail: e.to_string(),
            }
        })?;
        let new_key = Self::generate_and_store(&entry)?;
        Ok((new_key, new_version))
    }

    fn kind(&self) -> &'static str {
        "keychain"
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::panic
)]
mod tests {
    use super::*;

    #[test]
    fn version_from_account_parses_correctly() {
        assert_eq!(
            KeychainBackend::version_from_account("master-key-v1"),
            Some(1)
        );
        assert_eq!(
            KeychainBackend::version_from_account("master-key-v42"),
            Some(42)
        );
        assert_eq!(KeychainBackend::version_from_account("bad"), None);
    }

    #[test]
    fn account_for_version_round_trips() {
        let v = 7u32;
        let account = KeychainBackend::account_for_version(v);
        assert_eq!(KeychainBackend::version_from_account(&account), Some(v));
    }

    #[test]
    fn decode_key_rejects_wrong_length() {
        // 16 bytes base64-encoded → 24 chars; decode should yield 16 bytes.
        let short = BASE64.encode([0u8; 16]);
        assert!(KeychainBackend::decode_key(&short).is_err());
    }

    #[test]
    fn decode_key_accepts_32_byte_key() {
        let key = [0xABu8; 32];
        let encoded = BASE64.encode(key);
        let decoded = KeychainBackend::decode_key(&encoded).unwrap();
        assert_eq!(decoded, key);
    }
}
