//! Swapping `owner_id` in the context must fail AEAD authentication.

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
fn swap_owner_id_fails_decrypt() {
    let core = CipherCore::from_key_bytes([0x11; 32], 1);

    let ctx_a = EncryptContext {
        owner_kind: OwnerKind::Route,
        owner_id: "route-1".to_string(),
        field_path: JsonPointer("/tls/key".to_string()),
        key_version: 1,
    };
    let ctx_b = EncryptContext {
        owner_id: "route-2".to_string(),
        ..ctx_a.clone()
    };

    let ct = core.encrypt(b"secret", &ctx_a).expect("encrypt");
    let err = core
        .decrypt(&ct, &ctx_b)
        .expect_err("must fail with wrong owner_id");
    assert!(
        matches!(err, CryptoError::Decryption { .. }),
        "expected Decryption error, got {err:?}"
    );
}
