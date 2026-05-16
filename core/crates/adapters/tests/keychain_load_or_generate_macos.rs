//! macOS Keychain: first call generates, second call retrieves the same key.
//!
//! This test is gated on `target_os = "macos"` and writes a real entry into
//! the macOS Keychain under the test service name `"trilithon-test"`.  The
//! entry is cleaned up after the test.

#![cfg(target_os = "macos")]

use trilithon_adapters::secrets_local::{KeychainBackend, MasterKeyBackend as _};

#[tokio::test]
async fn keychain_load_or_generate_macos() {
    // Use a unique account per test run so parallel runs don't collide.
    let account = format!("master-key-v1-test-{}", std::process::id());
    let backend = KeychainBackend {
        service: "trilithon-test",
        account: account.clone(),
    };

    // First call: should generate a 32-byte key and store it.
    let key1 = backend
        .load_or_generate()
        .await
        .expect("first load_or_generate should succeed");
    assert_eq!(key1.len(), 32, "key must be 32 bytes");

    // Second call: should retrieve the same key.
    let key2 = backend
        .load_or_generate()
        .await
        .expect("second load_or_generate should succeed");
    assert_eq!(key1, key2, "second call must return the same key");

    // Cleanup: delete the test entry so we don't litter the Keychain.
    if let Ok(entry) = keyring::Entry::new("trilithon-test", &account) {
        let _ = entry.delete_credential();
    }
}
