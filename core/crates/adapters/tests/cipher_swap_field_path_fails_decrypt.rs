//! Swapping `field_path` in the context must fail AEAD authentication.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::panic
)]
// reason: integration test — panics are the correct failure mode

use trilithon_adapters::secrets_local::CipherCore;
use trilithon_core::model::JsonPointer;
use trilithon_core::secrets::{CryptoError, EncryptContext, OwnerKind};

#[test]
fn swap_field_path_fails_decrypt() {
    let core = CipherCore::from_key_bytes([0x22; 32], 1);

    let ctx_a = EncryptContext {
        owner_kind: OwnerKind::Route,
        owner_id: "route-1".to_string(),
        field_path: JsonPointer("/tls/key".to_string()),
        key_version: 1,
    };
    let ctx_b = EncryptContext {
        field_path: JsonPointer("/tls/cert".to_string()),
        ..ctx_a.clone()
    };

    let ct = core.encrypt(b"secret", &ctx_a).expect("encrypt");
    let err = core
        .decrypt(&ct, &ctx_b)
        .expect_err("must fail with wrong field_path");
    assert!(
        matches!(err, CryptoError::Decryption { .. }),
        "expected Decryption error, got {err:?}"
    );
}
