//! Pure-core vault surface: trait, typed ciphertext, and error variants.
//!
//! No I/O or key material handling lives here. Adapters supply key bytes;
//! this module only declares the shared contract.

use serde::{Deserialize, Serialize};

use crate::model::JsonPointer;

// ── OwnerKind ─────────────────────────────────────────────────────────────────

/// Identifies the kind of entity that owns an encrypted field.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OwnerKind {
    /// A reverse-proxy route.
    Route,
    /// An upstream backend.
    Upstream,
    /// An API token.
    Token,
    /// A user account.
    User,
    /// Any other entity kind.
    Other,
}

// ── EncryptContext ─────────────────────────────────────────────────────────────

/// Associated data bound to every encryption operation.
///
/// Two contexts that differ in any field produce distinct ciphertexts, so
/// cross-context ciphertext reuse is rejected at decryption time.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EncryptContext {
    /// Kind of entity that owns the encrypted field.
    pub owner_kind: OwnerKind,
    /// Stable identifier for the owning entity (e.g. a route slug or user id).
    pub owner_id: String,
    /// JSON Pointer to the field within the entity's document.
    pub field_path: JsonPointer,
    /// Master-key version in use at encryption time.
    pub key_version: u32,
}

impl EncryptContext {
    /// Returns a canonical byte representation suitable for use as AEAD
    /// associated data.
    ///
    /// Two contexts that differ in any field produce different bytes, so
    /// ciphertext cannot be transplanted between contexts without detection.
    ///
    /// # Panics
    ///
    /// Never panics — `EncryptContext` always serialises successfully.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        // serde_json serialises struct fields in declaration order, giving a
        // deterministic byte string without an external canonical-JSON step.
        // `EncryptContext` contains only `String`, `u32`, and a newtype around
        // `String` — serialisation is infallible.
        match serde_json::to_vec(self) {
            Ok(bytes) => bytes,
            Err(e) => unreachable!("EncryptContext serialisation is infallible: {e}"),
        }
    }
}

// ── AlgorithmTag ──────────────────────────────────────────────────────────────

/// Identifies the AEAD algorithm used for a [`Ciphertext`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AlgorithmTag {
    /// XChaCha20-Poly1305 with a 192-bit nonce and 128-bit tag.
    Xchacha20Poly1305,
}

// ── Ciphertext ────────────────────────────────────────────────────────────────

/// An encrypted blob produced by [`SecretsVault::encrypt`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Ciphertext {
    /// AEAD algorithm used to produce this blob.
    pub algorithm: AlgorithmTag,
    /// 24-byte nonce required by XChaCha20-Poly1305.
    pub nonce: Vec<u8>,
    /// Encrypted payload including the 16-byte Poly1305 authentication tag.
    pub blob: Vec<u8>,
    /// Master-key version used during encryption; needed for key lookup on
    /// decryption.
    pub key_version: u32,
}

// ── MasterKeyRotation ─────────────────────────────────────────────────────────

/// Result returned by [`SecretsVault::rotate_master_key`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MasterKeyRotation {
    /// Key version that was active before the rotation.
    pub previous_version: u32,
    /// Key version that is active after the rotation.
    pub new_version: u32,
    /// Number of stored rows that were re-encrypted with the new key.
    pub re_encrypted_rows: u32,
}

// ── CryptoError ───────────────────────────────────────────────────────────────

/// Errors that can occur during vault operations.
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    /// The requested master-key version is not loaded in the vault.
    #[error("master key version {version} not present")]
    KeyMissing {
        /// Version that was requested.
        version: u32,
    },

    /// AEAD decryption failed (wrong key, tampered blob, or mismatched context).
    #[error("decryption failed: {detail}")]
    Decryption {
        /// Human-readable explanation of the failure.
        detail: String,
    },

    /// The OS keychain / secret-store is unavailable.
    #[error("os keychain unavailable: {detail}")]
    KeyringUnavailable {
        /// Human-readable explanation of the failure.
        detail: String,
    },

    /// Argon2 KDF returned an error during key derivation.
    #[error("argon2 derivation failed: {detail}")]
    Argon2Failure {
        /// Human-readable explanation of the failure.
        detail: String,
    },
}

// ── SecretsVault ──────────────────────────────────────────────────────────────

/// Pure-core vault trait.  Adapters implement this; no key material is stored
/// in core types.
///
/// The trait is object-safe: all methods take `&self` and use only
/// `'static`-compatible bounds, so `dyn SecretsVault` is valid.
pub trait SecretsVault: Send + Sync + 'static {
    /// Encrypts `plaintext` binding `context` as AEAD associated data.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError`] if the requested key version is absent or if
    /// the underlying AEAD operation fails.
    fn encrypt(
        &self,
        plaintext: &[u8],
        context: &EncryptContext,
    ) -> Result<Ciphertext, CryptoError>;

    /// Decrypts `ciphertext`, verifying `context` as AEAD associated data.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError`] if the key version is absent, if the tag is
    /// invalid, or if the context does not match the one used at encryption.
    fn decrypt(
        &self,
        ciphertext: &Ciphertext,
        context: &EncryptContext,
    ) -> Result<Vec<u8>, CryptoError>;

    /// Rotates the master key, re-encrypting all stored secrets.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError`] if the new key cannot be generated or stored,
    /// or if any re-encryption fails.
    fn rotate_master_key(&self) -> Result<MasterKeyRotation, CryptoError>;

    /// Redacts secret fields from a JSON value according to `schema`.
    fn redact(
        &self,
        value: &serde_json::Value,
        schema: &crate::schema::SchemaRegistry,
    ) -> crate::audit::redactor::RedactionResult;
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

    fn sample_context(field: &str) -> EncryptContext {
        EncryptContext {
            owner_kind: OwnerKind::Route,
            owner_id: "route-1".to_string(),
            field_path: JsonPointer(field.to_string()),
            key_version: 1,
        }
    }

    #[test]
    fn ciphertext_serde_round_trip() {
        let ct = Ciphertext {
            algorithm: AlgorithmTag::Xchacha20Poly1305,
            nonce: vec![0u8; 24],
            blob: vec![1u8; 32],
            key_version: 1,
        };
        let json = serde_json::to_string(&ct).unwrap();
        let rt: Ciphertext = serde_json::from_str(&json).unwrap();
        assert_eq!(ct, rt);
    }

    #[test]
    fn encrypt_context_canonical_associated_data() {
        let ctx_a = sample_context("/route/upstream/password");
        let ctx_b = sample_context("/route/upstream/api_key");

        let bytes_a = ctx_a.canonical_bytes();
        let bytes_b = ctx_b.canonical_bytes();

        assert_ne!(
            bytes_a, bytes_b,
            "contexts differing only in field_path must produce different canonical bytes"
        );

        let rt: EncryptContext = serde_json::from_slice(&bytes_a).unwrap();
        assert_eq!(ctx_a, rt);
    }
}
