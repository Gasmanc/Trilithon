//! Ciphertexts carry the correct algorithm tag and a 24-byte nonce.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::panic
)]
// reason: integration test — panics are the correct failure mode

use trilithon_adapters::secrets_local::CipherCore;
use trilithon_core::model::JsonPointer;
use trilithon_core::secrets::{AlgorithmTag, EncryptContext, OwnerKind};

#[test]
fn algorithm_tag_and_nonce_length() {
    let core = CipherCore::from_key_bytes([0x33; 32], 1);
    let ctx = EncryptContext {
        owner_kind: OwnerKind::User,
        owner_id: "user-99".to_string(),
        field_path: JsonPointer("/api_key".to_string()),
        key_version: 1,
    };

    let ct = core.encrypt(b"my-api-key", &ctx).expect("encrypt");
    assert_eq!(ct.algorithm, AlgorithmTag::Xchacha20Poly1305);
    assert_eq!(
        ct.nonce.len(),
        24,
        "XChaCha20-Poly1305 requires a 24-byte nonce"
    );
}
