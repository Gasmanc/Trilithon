//! Linux Secret Service: first call generates, second call retrieves the same key.
//!
//! Gated on `target_os = "linux"`.  CI runners that do not have a session D-Bus
//! running (e.g. bare containers) are detected by attempting a keyring probe
//! before the main assertions; if the keyring is unavailable the test is skipped
//! via an early return (the test binary reports it as ignored/skipped, not failed).

#![cfg(target_os = "linux")]

use trilithon_adapters::secrets_local::{KeychainBackend, MasterKeyBackend as _};
use trilithon_core::secrets::CryptoError;

#[tokio::test]
async fn keychain_load_or_generate_linux() {
    let account = format!("master-key-v1-test-{}", std::process::id());
    let backend = KeychainBackend {
        service: "trilithon-test",
        account: account.clone(),
    };

    // Probe: if the Secret Service / D-Bus is not reachable, skip instead of fail.
    match backend.load_or_generate().await {
        Err(CryptoError::KeyringUnavailable { detail }) => {
            eprintln!("Skipping: Secret Service unavailable: {detail}");
            return;
        }
        Err(e) => panic!("unexpected error on first call: {e}"),
        Ok(key1) => {
            assert_eq!(key1.len(), 32, "key must be 32 bytes");

            let key2 = backend
                .load_or_generate()
                .await
                .expect("second load_or_generate should succeed");
            assert_eq!(key1, key2, "second call must return the same key");
        }
    }

    // Cleanup.
    if let Ok(entry) = keyring::Entry::new("trilithon-test", &account) {
        let _ = entry.delete_credential();
    }
}
