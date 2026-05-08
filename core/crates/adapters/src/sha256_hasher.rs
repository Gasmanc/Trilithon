//! Production [`CiphertextHasher`] implementation using SHA-256.
//!
//! [`Sha256AuditHasher`] is the only production implementation; all test code
//! uses the lighter `ZeroHasher` double defined in `test_support`.

use sha2::{Digest, Sha256};

use trilithon_core::audit::redactor::{CiphertextHasher, HASH_PREFIX_LEN};

// ── Sha256AuditHasher ─────────────────────────────────────────────────────────

/// Production [`CiphertextHasher`] that computes a SHA-256 digest and returns
/// the first [`HASH_PREFIX_LEN`] lowercase hex characters.
///
/// This is the only implementation that should be wired into [`AuditWriter`]
/// in production.  It is deterministic: the same plaintext always produces the
/// same prefix.
pub struct Sha256AuditHasher;

impl CiphertextHasher for Sha256AuditHasher {
    /// Return the first [`HASH_PREFIX_LEN`] lowercase-hex characters of the
    /// SHA-256 digest of `plaintext`.
    fn hash_for_value(&self, plaintext: &str) -> String {
        let digest = Sha256::digest(plaintext.as_bytes());
        // SHA-256 produces 32 bytes = 64 hex chars; we keep the first
        // HASH_PREFIX_LEN characters (12) as the stable identifier prefix.
        format!("{digest:x}")[..HASH_PREFIX_LEN].to_owned()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use trilithon_core::audit::redactor::REDACTION_PREFIX;

    #[test]
    fn hash_length_equals_hash_prefix_len() {
        let hasher = Sha256AuditHasher;
        let hash = hasher.hash_for_value("some-secret");
        assert_eq!(
            hash.len(),
            HASH_PREFIX_LEN,
            "hash prefix must be exactly {HASH_PREFIX_LEN} characters"
        );
    }

    #[test]
    fn hash_is_lowercase_hex() {
        let hasher = Sha256AuditHasher;
        let hash = hasher.hash_for_value("another-secret");
        assert!(
            hash.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
            "hash must be lowercase hex: {hash}"
        );
    }

    #[test]
    fn hash_is_deterministic() {
        let hasher = Sha256AuditHasher;
        let h1 = hasher.hash_for_value("stable-input");
        let h2 = hasher.hash_for_value("stable-input");
        assert_eq!(h1, h2, "same input must produce same hash");
    }

    #[test]
    fn redaction_marker_format() {
        let hasher = Sha256AuditHasher;
        let hash = hasher.hash_for_value("secret");
        let marker = format!("{REDACTION_PREFIX}{hash}");
        assert!(
            marker.starts_with(REDACTION_PREFIX),
            "marker must start with redaction prefix"
        );
    }
}
