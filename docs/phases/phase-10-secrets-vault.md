# Phase 10 — Secrets vault

Source of truth: [`../phases/phased-plan.md#phase-10--secrets-vault`](../phases/phased-plan.md#phase-10--secrets-vault).

## Pre-flight checklist

- [ ] Phase 6 complete (redactor exists and is wired into audit).
- [ ] Phase 9 complete (HTTP authentication exists).

## Tasks

### Backend / core crate

- [ ] **Define the `SecretsVault` trait.**
  - Acceptance: `crates/core/src/secrets.rs` MUST expose `encrypt(plaintext, context) -> Ciphertext`, `decrypt(ciphertext, context) -> Plaintext`, `rotate_master_key(new) -> ()`. The trait MUST be pure; key material is supplied by the adapter.
  - Done when: the trait compiles and a unit test asserts no I/O dependency.
  - Feature: T1.15.
- [ ] **Define typed `Ciphertext` and associated-data binding.**
  - Acceptance: `Ciphertext` MUST carry the algorithm tag, the 24-byte nonce, the wrapped bytes, and the `key_version`. The associated data MUST bind ciphertext to its row identifier so swaps fail authentication.
  - Done when: a unit test that swaps row identifiers between rows observes authentication failure.
  - Feature: T1.15.

### Backend / adapters crate

- [ ] **Implement the `KeychainBackend`.**
  - Acceptance: A `KeychainBackend` adapter MUST use the `keyring` crate on macOS and the Secret Service API on Linux, generating a 256-bit master key on first run and storing it under service `trilithon`, account `master-key-v1`.
  - Done when: integration tests on macOS and Linux runners exercise generation and retrieval.
  - Feature: T1.15.
- [ ] **Implement the `FileBackend` fallback.**
  - Acceptance: The fallback MUST write the master key to `<data_dir>/master-key` with mode `0600` and ownership matching the daemon user. The chosen backend MUST be recorded in `secrets_metadata` and surfaced at startup.
  - Done when: an integration test simulating keychain failure exercises the fallback and observes the file mode.
  - Feature: T1.15.
- [ ] **Implement XChaCha20-Poly1305 encryption.**
  - Acceptance: Encryption MUST use XChaCha20-Poly1305 via the `chacha20poly1305` crate with a per-record 24-byte nonce drawn from `getrandom`.
  - Done when: a unit test asserts the algorithm and a property test asserts decrypt-after-encrypt is the identity.
  - Feature: T1.15.

### Database migrations

- [ ] **Author migration `0004_secrets.sql`.**
  - Acceptance: Migration `0004_secrets.sql` MUST add `secret_id`, `owner_kind`, `owner_id`, `field_name`, `ciphertext`, `nonce`, `algorithm`, `key_version`, `created_at`, `updated_at` to `secrets_metadata`.
  - Done when: a schema-introspection test asserts the columns and the integration suite passes.
  - Feature: T1.15.

### HTTP endpoints

- [ ] **Implement `POST /api/v1/secrets/{secret_id}/reveal`.**
  - Acceptance: The endpoint MUST require an authenticated session, MUST require re-entry of the user's password as a step-up control, MUST return the plaintext, and MUST write a `SecretsRevealed` audit row containing the secret identifier, the actor, and the correlation identifier — but NOT the plaintext.
  - Done when: an integration test asserts every clause and observes the audit row without plaintext.
  - Feature: T1.15.

### Wiring

- [ ] **Route every secret-marked field through the vault.**
  - Acceptance: Every mutation that carries a secret-marked field MUST route the field through the vault; the snapshot MUST store the ciphertext reference, never the plaintext.
  - Done when: an end-to-end integration test asserts the snapshot row carries no plaintext byte.
  - Feature: T1.15.
- [ ] **Hash ciphertext for the redactor's diff representation.**
  - Acceptance: The Phase 6 redactor MUST hash the ciphertext for diff representation; identical secrets MUST produce identical hash prefixes.
  - Done when: a unit test exercises the stable-hash invariant.
  - Feature: T1.15 (mitigates H10).
- [ ] **Surface the file-backend backup warning at startup.**
  - Acceptance: When the file backend is in use the daemon MUST emit a startup warning recommending a backup of `<data_dir>/master-key`.
  - Done when: an integration test asserts the warning under fallback conditions.
  - Feature: T1.15.

### Tests

- [ ] **Leaked-SQLite simulation.**
  - Acceptance: A "leaked SQLite file" simulation MUST verify that no secret can be recovered without the master key.
  - Done when: a test that exfiltrates the SQLite file and runs every recovery method observes no plaintext recovery.
  - Feature: T1.15.
- [ ] **Step-up authentication on reveal.**
  - Acceptance: An integration test MUST assert that reveal without re-entered password returns 401.
  - Done when: the test passes.
  - Feature: T1.15.

### Documentation

- [ ] **Document the secrets architecture and recovery semantics.**
  - Acceptance: `core/README.md` MUST add a "Secrets" section describing the vault, the master-key location, the backup recommendation, and the master-key-loss equals data-loss invariant.
  - Done when: the section is present and references ADR-0014.
  - Feature: T1.15.

## Cross-references

- ADR-0014 (secrets encrypted at rest with keychain master key).
- ADR-0009 (audit log — reveal writes audit).
- PRD T1.15 (secrets abstraction).
- Architecture: "Secrets vault," "Step-up reveal," "Failure modes — master-key access denied."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] All secret-marked fields are stored encrypted at rest under XChaCha20-Poly1305.
- [ ] The master key lives outside the SQLite database; on macOS and Linux the keychain backend is the default.
- [ ] Reveal produces an audit row and requires step-up authentication.
- [ ] A copy of the SQLite file alone is not sufficient to recover any secret; the test corpus exercises this.
- [ ] The redactor is the only path between the diff engine and the audit log writer; no code path bypasses it.
