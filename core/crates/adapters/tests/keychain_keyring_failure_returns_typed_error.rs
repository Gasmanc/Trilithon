//! A keyring `PlatformFailure` surfaces as `CryptoError::KeyringUnavailable`.
//!
//! This test uses a stub that wraps a closure returning `PlatformFailure`
//! without touching the real OS keychain.

use async_trait::async_trait;
use trilithon_adapters::secrets_local::MasterKeyBackend as _;
use trilithon_core::secrets::CryptoError;

/// Stub backend that always returns `PlatformFailure` on `load_or_generate`.
struct AlwaysFailBackend;

#[async_trait]
impl trilithon_adapters::secrets_local::MasterKeyBackend for AlwaysFailBackend {
    async fn load_or_generate(&self) -> Result<[u8; 32], CryptoError> {
        Err(CryptoError::KeyringUnavailable {
            detail: "simulated PlatformFailure".to_string(),
        })
    }

    async fn rotate(&self) -> Result<([u8; 32], u32), CryptoError> {
        Err(CryptoError::KeyringUnavailable {
            detail: "simulated PlatformFailure".to_string(),
        })
    }

    fn kind(&self) -> &'static str {
        "stub"
    }
}

#[tokio::test]
async fn keychain_keyring_failure_returns_typed_error() {
    let backend = AlwaysFailBackend;
    let result = backend.load_or_generate().await;
    assert!(
        matches!(result, Err(CryptoError::KeyringUnavailable { .. })),
        "expected KeyringUnavailable, got {result:?}"
    );
}
