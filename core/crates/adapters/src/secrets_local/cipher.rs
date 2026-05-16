//! XChaCha20-Poly1305 encrypt / decrypt with AAD binding.
//!
//! # Security properties
//!
//! * Each record gets a fresh 24-byte nonce drawn from the OS CSPRNG via
//!   `getrandom`.
//! * The [`EncryptContext`] is serialised as canonical JSON (sorted-key, no
//!   whitespace) and passed as AEAD associated data, so transplanting a
//!   ciphertext to a different row's context fails authentication.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use trilithon_core::secrets::{AlgorithmTag, Ciphertext, CryptoError, EncryptContext};

// ── CipherCore ────────────────────────────────────────────────────────────────

/// Single-key XChaCha20-Poly1305 encryptor/decryptor.
///
/// Slice 10.3 wraps multiple `CipherCore` instances in a key-version map.
pub struct CipherCore {
    cipher: XChaCha20Poly1305,
    key_version: u32,
}

impl CipherCore {
    /// Construct from raw key bytes.
    ///
    /// `key_version` is stored verbatim in every [`Ciphertext`] produced by
    /// [`CipherCore::encrypt`] so callers can route to the correct key on
    /// decryption.
    pub fn from_key_bytes(bytes: [u8; 32], key_version: u32) -> Self {
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&bytes));
        Self {
            cipher,
            key_version,
        }
    }

    /// Encrypt `plaintext`, binding `context` as AEAD associated data.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::Decryption`] (reused as the encryption failure
    /// variant — there is no separate encryption-error variant in the core
    /// type) if the AEAD operation fails, which is extremely unlikely with a
    /// correctly initialised cipher.
    pub fn encrypt(
        &self,
        plaintext: &[u8],
        context: &EncryptContext,
    ) -> Result<Ciphertext, CryptoError> {
        let mut nonce_bytes = [0u8; 24];
        getrandom::getrandom(&mut nonce_bytes).map_err(|e| CryptoError::Decryption {
            detail: format!("getrandom failed: {e}"),
        })?;

        let aad = associated_data(context);
        let blob = self
            .cipher
            .encrypt(
                XNonce::from_slice(&nonce_bytes),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|e| CryptoError::Decryption {
                detail: e.to_string(),
            })?;

        Ok(Ciphertext {
            algorithm: AlgorithmTag::Xchacha20Poly1305,
            nonce: nonce_bytes.to_vec(),
            blob,
            key_version: self.key_version,
        })
    }

    /// Decrypt `ciphertext`, verifying `context` as AEAD associated data.
    ///
    /// # Errors
    ///
    /// * [`CryptoError::KeyMissing`] — ciphertext's `key_version` does not
    ///   match this `CipherCore`'s version.
    /// * [`CryptoError::Decryption`] — tag verification failed (wrong key,
    ///   tampered blob, or mismatched context).
    pub fn decrypt(
        &self,
        ciphertext: &Ciphertext,
        context: &EncryptContext,
    ) -> Result<Vec<u8>, CryptoError> {
        if ciphertext.key_version != self.key_version {
            return Err(CryptoError::KeyMissing {
                version: ciphertext.key_version,
            });
        }

        let aad = associated_data(context);
        let nonce = XNonce::from_slice(&ciphertext.nonce);
        self.cipher
            .decrypt(
                nonce,
                Payload {
                    msg: &ciphertext.blob,
                    aad: &aad,
                },
            )
            .map_err(|e| CryptoError::Decryption {
                detail: e.to_string(),
            })
    }
}

// ── associated_data ───────────────────────────────────────────────────────────

/// Canonical serialisation of [`EncryptContext`] used as AEAD associated data.
///
/// The struct fields are emitted in declaration order by `serde_json`, which
/// gives deterministic bytes across compilations.  Every distinct context value
/// produces a distinct byte string (see [`EncryptContext::canonical_bytes`]).
pub fn associated_data(context: &EncryptContext) -> Vec<u8> {
    context.canonical_bytes()
}
