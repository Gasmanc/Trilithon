//! Local symmetric-encryption vault: XChaCha20-Poly1305 with per-record nonces.
//!
//! Key material never leaves this module boundary. The [`CipherCore`] struct
//! holds a single 256-bit key; Slice 10.3 will wrap it in a key-version map.

pub mod cipher;
pub mod keychain;
pub use cipher::CipherCore;
pub use keychain::{KeychainBackend, MasterKeyBackend};
