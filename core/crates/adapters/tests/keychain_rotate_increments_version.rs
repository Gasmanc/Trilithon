//! `rotate` stores the new key under `master-key-v{n+1}` and returns the

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::panic
)]
// reason: integration test — panics are the correct failure mode
//! incremented version.  The old version's entry is still retrievable.

use trilithon_adapters::secrets_local::{KeychainBackend, MasterKeyBackend as _};
use trilithon_core::secrets::CryptoError;

#[tokio::test]
async fn keychain_rotate_increments_version() {
    let pid = std::process::id();
    let account_v1 = format!("master-key-v1-rot-{pid}");
    let backend = KeychainBackend {
        service: "trilithon-test",
        account: account_v1.clone(),
    };

    // Load or generate the v1 key; skip if keychain is unavailable.
    let key_v1 = match backend.load_or_generate().await {
        Err(CryptoError::KeyringUnavailable { detail }) => {
            eprintln!("Skipping: keychain unavailable: {detail}");
            return;
        }
        Err(e) => panic!("load_or_generate failed: {e}"),
        Ok(k) => k,
    };

    // Rotate: should store under v2 and return version 2.
    let (key_v2, new_version) = backend.rotate().await.expect("rotate should succeed");
    assert_eq!(new_version, 2, "rotate must return version 2");
    assert_ne!(key_v1, key_v2, "rotated key must differ from original");

    // The v1 entry must still be retrievable and unchanged.
    let backend_v1_check = KeychainBackend {
        service: "trilithon-test",
        account: account_v1.clone(),
    };
    let retrieved_v1 = backend_v1_check
        .load_or_generate()
        .await
        .expect("v1 key must still be retrievable after rotate");
    assert_eq!(
        key_v1, retrieved_v1,
        "v1 key must be unchanged after rotate"
    );

    // Cleanup.
    for account in [account_v1, format!("master-key-v2-rot-{pid}")] {
        if let Ok(entry) = keyring_core::Entry::new("trilithon-test", &account) {
            let _ = entry.delete_credential();
        }
    }
}
