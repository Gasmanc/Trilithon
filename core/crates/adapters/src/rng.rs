//! Random-byte generation abstraction for adapters.

/// Generates cryptographically random bytes.
pub trait RandomBytes: Send + Sync + 'static {
    /// Fill `buf` with random bytes.
    fn fill_bytes(&self, buf: &mut [u8]);
}

/// Implementation backed by `rand::thread_rng`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreadRng;

impl RandomBytes for ThreadRng {
    fn fill_bytes(&self, buf: &mut [u8]) {
        use rand::RngCore as _;
        rand::thread_rng().fill_bytes(buf);
    }
}
