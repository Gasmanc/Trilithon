//! Argon2id password hashing at RFC 9106 first-recommendation parameters.

use std::sync::OnceLock;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};

/// A dummy Argon2id hash computed once at startup for constant-time verification.
static DUMMY_HASH: OnceLock<String> = OnceLock::new();

/// Perform a dummy Argon2id verification to mitigate username-enumeration timing attacks.
///
/// Call this when a username lookup returns nothing so the handler takes the same time
/// as a real wrong-password path (F018).
pub fn dummy_verify(password: &str) {
    let hash = DUMMY_HASH.get_or_init(|| {
        let salt = SaltString::generate(&mut OsRng);
        // SAFETY: constants are always valid; this path is unreachable in tests too.
        hash_password("__dummy_password_placeholder__", &salt).unwrap_or_else(|_| String::new())
    });
    if hash.is_empty() {
        return;
    }
    // Ignore the result — timing is the only goal here.
    let _ = verify_password(password, hash);
}

/// Memory cost in KiB (RFC 9106 first recommendation: 19 MiB).
pub const ARGON2_M_COST_KIB: u32 = 19456;

/// Time cost in iterations (RFC 9106 first recommendation: 2).
pub const ARGON2_T_COST: u32 = 2;

/// Parallelism factor (RFC 9106 first recommendation: 1).
pub const ARGON2_P_COST: u32 = 1;

/// Errors produced by password hashing and verification operations.
#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    /// The Argon2 library returned an error during hashing or verification.
    #[error("argon2 failure: {0}")]
    Argon2(String),
    /// The encoded hash string could not be parsed as a valid PHC string.
    #[error("hash decoding failure: {0}")]
    Decode(String),
}

/// Construct an Argon2id instance with RFC 9106 first-recommendation parameters.
///
/// # Errors
///
/// Returns [`PasswordError::Argon2`] if the Argon2 parameter validation fails.
/// In practice this cannot occur because the constants are always within range.
pub fn argon2id() -> Result<Argon2<'static>, PasswordError> {
    let params = Params::new(ARGON2_M_COST_KIB, ARGON2_T_COST, ARGON2_P_COST, None)
        .map_err(|e| PasswordError::Argon2(e.to_string()))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

/// Hash a plaintext password using Argon2id. Returns the PHC-encoded string.
///
/// # Errors
///
/// Returns [`PasswordError::Argon2`] if the Argon2 library fails to hash the password.
pub fn hash_password(plaintext: &str, salt: &SaltString) -> Result<String, PasswordError> {
    argon2id()?
        .hash_password(plaintext.as_bytes(), salt)
        .map(|h| h.to_string())
        .map_err(|e| PasswordError::Argon2(e.to_string()))
}

/// Verify a plaintext password against a PHC-encoded hash.
///
/// Returns `Ok(true)` if the password matches, `Ok(false)` if it does not.
///
/// # Errors
///
/// Returns [`PasswordError::Decode`] if `encoded_hash` is not a valid PHC string.
/// Returns [`PasswordError::Argon2`] for any other Argon2 library error.
pub fn verify_password(plaintext: &str, encoded_hash: &str) -> Result<bool, PasswordError> {
    let parsed =
        PasswordHash::new(encoded_hash).map_err(|e| PasswordError::Decode(e.to_string()))?;
    match argon2id()?.verify_password(plaintext.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(e) => Err(PasswordError::Argon2(e.to_string())),
    }
}
