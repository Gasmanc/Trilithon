//! Decrypt-after-encrypt is the identity for arbitrary plaintexts and contexts.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::panic
)]
// reason: integration test — panics are the correct failure mode

use trilithon_adapters::secrets_local::CipherCore;
use trilithon_core::model::JsonPointer;
use trilithon_core::secrets::{EncryptContext, OwnerKind};

fn ctx() -> EncryptContext {
    EncryptContext {
        owner_kind: OwnerKind::Route,
        owner_id: "route-42".to_string(),
        field_path: JsonPointer("/tls/key".to_string()),
        key_version: 1,
    }
}

#[test]
fn round_trip_empty_plaintext() {
    let core = CipherCore::from_key_bytes([0xAB; 32], 1);
    let ctx = ctx();
    let ct = core.encrypt(b"", &ctx).expect("encrypt");
    let pt = core.decrypt(&ct, &ctx).expect("decrypt");
    assert_eq!(pt, b"");
}

#[test]
fn round_trip_short_plaintext() {
    let core = CipherCore::from_key_bytes([0x01; 32], 1);
    let ctx = ctx();
    let plaintext = b"hunter2";
    let ct = core.encrypt(plaintext, &ctx).expect("encrypt");
    let pt = core.decrypt(&ct, &ctx).expect("decrypt");
    assert_eq!(pt, plaintext);
}

#[test]
fn round_trip_long_plaintext() {
    let core = CipherCore::from_key_bytes([0xFF; 32], 1);
    let ctx = ctx();
    let plaintext = vec![0x42u8; 4096];
    let ct = core.encrypt(&plaintext, &ctx).expect("encrypt");
    let pt = core.decrypt(&ct, &ctx).expect("decrypt");
    assert_eq!(pt, plaintext);
}

#[test]
fn nonces_are_unique_across_encryptions() {
    let core = CipherCore::from_key_bytes([0x77; 32], 1);
    let ctx = ctx();
    let ct1 = core.encrypt(b"same", &ctx).expect("encrypt 1");
    let ct2 = core.encrypt(b"same", &ctx).expect("encrypt 2");
    assert_ne!(ct1.nonce, ct2.nonce, "nonces must be unique per encryption");
}
