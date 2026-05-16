
## Slice 10.1
**Status:** complete
**Date:** 2026-05-16
**Summary:** Created `core/crates/core/src/secrets.rs` with the full pure-core vault surface: `OwnerKind`, `EncryptContext` (with `canonical_bytes()`), `AlgorithmTag`, `Ciphertext`, `MasterKeyRotation`, `CryptoError`, and the object-safe `SecretsVault` trait. Registered the module in `lib.rs`. Both spec tests pass and the full `just check-rust` gate is green.
### Simplify Findings
- F1: Redundant per-function `#[allow(clippy::unwrap_used, clippy::disallowed_methods)]` attributes in two test functions — already covered by the mod-level `#[allow]` block.
- F2: Self-evident comment in `encrypt_context_canonical_associated_data` test narrating what the following assertion already expressed.
### Items Fixed Inline
- Removed redundant `#[allow]` from `fn ciphertext_serde_round_trip` and `fn encrypt_context_canonical_associated_data`.
- Removed self-evident comment before the round-trip assertion in `encrypt_context_canonical_associated_data`.
### Items Left Unfixed
none
