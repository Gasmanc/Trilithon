# Phase 10 — Secrets vault — Implementation Slices

> Phase reference: [../phases/phase-10-secrets-vault.md](../phases/phase-10-secrets-vault.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference [../phases/phase-10-secrets-vault.md](../phases/phase-10-secrets-vault.md).
- Architecture §6.6 (audit kind `secrets.revealed`, `secrets.master-key-rotated`), §6.9 (`secrets_metadata`), §11 (security posture), §12.1 (tracing event `secrets.master-key.rotated`).
- Trait signatures: `core::secrets::SecretsVault`, `EncryptContext`, `Ciphertext`, `CryptoError`, `core::storage::Storage`.
- ADRs: ADR-0009 (audit log — reveal writes audit), ADR-0014 (secrets encrypted at rest with keychain master key).

## Slice plan summary

| # | Title | Primary files | Effort (h) | Depends on |
|---|-------|---------------|-----------:|-----------|
| 10.1 | `Ciphertext`, `EncryptContext`, `SecretsVault` types in core | `core/crates/core/src/secrets/mod.rs` | 4 | Phase 6 |
| 10.2 | XChaCha20-Poly1305 encryptor with associated-data binding | `core/crates/adapters/src/secrets_local/cipher.rs` | 5 | 10.1 |
| 10.3 | `KeychainBackend` (macOS Keychain, Linux Secret Service) | `core/crates/adapters/src/secrets_local/keychain.rs` | 6 | 10.1 |
| 10.4 | `FileBackend` fallback with mode 0600 master-key file | `core/crates/adapters/src/secrets_local/file.rs` | 5 | 10.1 |
| 10.5 | `0004_secrets.sql` migration plus `secrets_metadata` writer | `core/crates/adapters/migrations/0004_secrets.sql`, `core/crates/adapters/src/storage_sqlite/secrets.rs` | 4 | 10.2 |
| 10.6 | `POST /api/v1/secrets/{secret_id}/reveal` with step-up auth | `core/crates/adapters/src/http_axum/secrets_routes.rs` | 6 | 10.5, Phase 9 |
| 10.7 | Wire mutation pipeline through the vault and feed redactor with ciphertext hash | `core/crates/core/src/mutation/secrets_extract.rs`, `core/crates/adapters/src/mutation_queue/secrets.rs` | 6 | 10.5, Phase 6 |

---

## Slice 10.1 — `Ciphertext`, `EncryptContext`, `SecretsVault` types in core

### Goal

Land the pure-core surface for the vault per `trait-signatures.md` §3. No I/O, no key material handling. Adapters supply the actual key bytes; this slice only declares the trait, the typed `Ciphertext`, the `EncryptContext` for associated-data binding, and the error variants.

### Entry conditions

- Phase 6 done; the `RedactedValue` and `SchemaRegistry` types are in scope.

### Files to create or modify

- `core/crates/core/src/secrets/mod.rs` — module root with the trait and types.
- `core/crates/core/src/lib.rs` — `pub mod secrets;`.

### Signatures and shapes

```rust
use serde::{Deserialize, Serialize};
use crate::diff::JsonPointer;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EncryptContext {
    pub owner_kind:  OwnerKind,
    pub owner_id:    String,
    pub field_path:  JsonPointer,
    pub key_version: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OwnerKind { Route, Upstream, Token, User, Other }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Ciphertext {
    pub algorithm:   AlgorithmTag,        // "xchacha20-poly1305"
    pub nonce:       Vec<u8>,             // 24 bytes
    pub ciphertext:  Vec<u8>,             // includes 16-byte tag
    pub key_version: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AlgorithmTag { Xchacha20Poly1305 }

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("master key version {version} not present")]
    KeyMissing { version: u32 },
    #[error("decryption failed: {detail}")]
    Decryption { detail: String },
    #[error("os keychain unavailable: {detail}")]
    KeyringUnavailable { detail: String },
    #[error("argon2 derivation failed: {detail}")]
    Argon2Failure { detail: String },
}

pub trait SecretsVault: Send + Sync + 'static {
    fn encrypt(
        &self,
        plaintext: &[u8],
        context:   &EncryptContext,
    ) -> Result<Ciphertext, CryptoError>;

    fn decrypt(
        &self,
        ciphertext: &Ciphertext,
        context:    &EncryptContext,
    ) -> Result<Vec<u8>, CryptoError>;

    fn rotate_master_key(&self) -> Result<MasterKeyRotation, CryptoError>;

    fn redact(
        &self,
        value:  &serde_json::Value,
        schema: &crate::schema::SchemaRegistry,
    ) -> crate::audit::redactor::RedactionResult;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MasterKeyRotation {
    pub previous_version: u32,
    pub new_version:      u32,
    pub re_encrypted_rows: u32,
}
```

### Tests

- `core::secrets::tests::ciphertext_serde_round_trip` — every variant.
- `core::secrets::tests::encrypt_context_canonical_associated_data` — assert two contexts that differ only in `field_path` produce different canonical-bytes representations.

### Acceptance command

`cargo test -p trilithon-core secrets::tests`

### Exit conditions

- The trait MUST be object-safe (`dyn SecretsVault` compiles).
- Every type MUST round-trip through serde.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- ADR-0014.
- PRD T1.15.
- trait-signatures.md §3.

---

## Slice 10.2 — XChaCha20-Poly1305 encryptor with associated-data binding

### Goal

Implement the symmetric encryption core: a 256-bit key, a 24-byte per-record nonce drawn from `getrandom`, and authenticated associated data binding the ciphertext to its `EncryptContext`. A leak that swaps a ciphertext between rows MUST fail authentication on decrypt.

### Entry conditions

- Slice 10.1 done.

### Files to create or modify

- `core/crates/adapters/src/secrets_local/mod.rs` — module root.
- `core/crates/adapters/src/secrets_local/cipher.rs` — encrypt/decrypt.
- `core/crates/adapters/Cargo.toml` — add `chacha20poly1305`, `getrandom`.

### Signatures and shapes

```rust
use chacha20poly1305::{XChaCha20Poly1305, Key, XNonce};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use trilithon_core::secrets::{Ciphertext, EncryptContext, AlgorithmTag, CryptoError};

pub struct CipherCore {
    key:         Key,        // 32 bytes
    key_version: u32,
}

impl CipherCore {
    pub fn from_key_bytes(bytes: [u8; 32], key_version: u32) -> Self;

    pub fn encrypt(
        &self,
        plaintext: &[u8],
        context:   &EncryptContext,
    ) -> Result<Ciphertext, CryptoError>;

    pub fn decrypt(
        &self,
        ciphertext: &Ciphertext,
        context:    &EncryptContext,
    ) -> Result<Vec<u8>, CryptoError>;
}

/// Canonical serialisation of EncryptContext used as AAD. Sorted-key JSON.
pub fn associated_data(context: &EncryptContext) -> Vec<u8>;
```

### Algorithm

1. `from_key_bytes` constructs `XChaCha20Poly1305::new(Key::from_slice(&bytes))`.
2. `encrypt`:
   1. Sample 24 random bytes via `getrandom::getrandom(&mut nonce)`.
   2. Compute `aad = associated_data(context)`.
   3. `let ct = cipher.encrypt(XNonce::from_slice(&nonce), Payload { msg: plaintext, aad: &aad }).map_err(|e| CryptoError::Decryption { detail: e.to_string() })?;`.
   4. Return `Ciphertext { algorithm: AlgorithmTag::Xchacha20Poly1305, nonce: nonce.to_vec(), ciphertext: ct, key_version: self.key_version }`.
3. `decrypt`:
   1. If `ciphertext.key_version != self.key_version`, return `KeyMissing`. (Slice 10.3 will hold every version in a `HashMap`.)
   2. Compute `aad = associated_data(context)`. Decrypt; on failure return `Decryption`.
4. `associated_data` serialises the context as canonical JSON (sorted keys, no whitespace).

### Tests

- `core/crates/adapters/tests/cipher_encrypt_decrypt_round_trip.rs` — random plaintexts; decrypt-after-encrypt is the identity.
- `core/crates/adapters/tests/cipher_swap_owner_id_fails_decrypt.rs` — encrypt under context A; attempt decrypt under context A' differing only in `owner_id`; assert `CryptoError::Decryption`.
- `core/crates/adapters/tests/cipher_swap_field_path_fails_decrypt.rs` — same pattern with `field_path`.
- `core/crates/adapters/tests/cipher_uses_xchacha20poly1305.rs` — assert `Ciphertext.algorithm == Xchacha20Poly1305` and `nonce.len() == 24`.

### Acceptance command

`cargo test -p trilithon-adapters cipher_`

### Exit conditions

- Decrypt-after-encrypt MUST be the identity for any context.
- Any change to `EncryptContext` MUST fail decryption.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- ADR-0014.
- PRD T1.15.

---

## Slice 10.3 — `KeychainBackend` (macOS Keychain, Linux Secret Service)

### Goal

Source the master key from the OS keychain on macOS and the Secret Service API on Linux. On first run the backend generates a 256-bit key and stores it under service `trilithon`, account `master-key-v1`. The vault initialises the cipher with the retrieved key.

### Entry conditions

- Slice 10.2 done.

### Files to create or modify

- `core/crates/adapters/src/secrets_local/keychain.rs` — backend.
- `core/crates/adapters/Cargo.toml` — add `keyring`.

### Signatures and shapes

```rust
use trilithon_core::secrets::{CryptoError, MasterKeyRotation};

pub struct KeychainBackend {
    pub service: &'static str,    // "trilithon"
    pub account: String,          // "master-key-v1"
}

#[async_trait::async_trait]
pub trait MasterKeyBackend: Send + Sync + 'static {
    async fn load_or_generate(&self) -> Result<[u8; 32], CryptoError>;
    async fn rotate(&self) -> Result<([u8; 32], u32 /* new_version */), CryptoError>;
    fn kind(&self) -> &'static str;          // "keychain" or "file"
}

#[async_trait::async_trait]
impl MasterKeyBackend for KeychainBackend {
    async fn load_or_generate(&self) -> Result<[u8; 32], CryptoError>;
    async fn rotate(&self) -> Result<([u8; 32], u32), CryptoError>;
    fn kind(&self) -> &'static str { "keychain" }
}
```

### Algorithm

1. `load_or_generate`:
   1. `let entry = keyring::Entry::new(self.service, &self.account)?;`.
   2. `match entry.get_password() { Ok(s) => decode_base64(s), Err(NoEntry) => generate_and_store() }`.
   3. `generate_and_store`: 32 random bytes via `getrandom`; base64-encode; `entry.set_password(&encoded)?`. Return the bytes.
   4. On `keyring::Error::PlatformFailure`, return `CryptoError::KeyringUnavailable { detail }`. The caller (vault constructor) falls back to `FileBackend`.
2. `rotate`:
   1. Generate a new 32-byte key.
   2. Increment the version: read the current account string `master-key-v{n}`, store under `master-key-v{n+1}`, retain the old key for re-encryption.
   3. Return `(new_key, n+1)`.

### Tests

- `core/crates/adapters/tests/keychain_load_or_generate_macos.rs` (gated on `cfg(target_os = "macos")`) — fresh keychain entry; assert generation on first call, retrieval on second.
- `core/crates/adapters/tests/keychain_load_or_generate_linux.rs` (gated on `cfg(target_os = "linux")`) — same semantics through Secret Service. CI runners that do not have a session-bus running MUST skip this test (gate on `dbus` reachability; emit `cargo test … -- --skipped` instead of failing).
- `core/crates/adapters/tests/keychain_rotate_increments_version.rs` — rotate; assert version increment and that the previous version is still retrievable.
- `core/crates/adapters/tests/keychain_keyring_failure_returns_typed_error.rs` — inject a keyring stub returning `PlatformFailure`; assert `CryptoError::KeyringUnavailable`.

### Acceptance command

`cargo test -p trilithon-adapters keychain_`

### Exit conditions

- The backend MUST generate a 256-bit key on first run.
- A platform failure MUST surface `CryptoError::KeyringUnavailable` so the vault can fall back.

### Audit kinds emitted

None directly. The vault writes `secrets.master-key-rotated` on rotate (slice 10.6 wires the audit append).

### Tracing events emitted

`secrets.master-key.rotated` (architecture §12.1) on rotate.

### Cross-references

- ADR-0014.
- PRD T1.15.

---

## Slice 10.4 — `FileBackend` fallback with mode 0600 master-key file

### Goal

When the keychain is unavailable, store the master key in `<data_dir>/master-key` with mode 0600. Record the chosen backend in `secrets_metadata.backend_kind` (a column added by slice 10.5). Surface a startup warning recommending an out-of-band backup of the file.

### Entry conditions

- Slice 10.3 done.

### Files to create or modify

- `core/crates/adapters/src/secrets_local/file.rs` — backend.

### Signatures and shapes

```rust
pub struct FileBackend {
    pub path: std::path::PathBuf,        // <data_dir>/master-key
}

#[async_trait::async_trait]
impl MasterKeyBackend for FileBackend {
    async fn load_or_generate(&self) -> Result<[u8; 32], CryptoError>;
    async fn rotate(&self) -> Result<([u8; 32], u32), CryptoError>;
    fn kind(&self) -> &'static str { "file" }
}
```

The on-disk format is a single line of `version=N\nkey=<base64>\n`.

### Algorithm

1. `load_or_generate`:
   1. If the file exists, read it; parse `version` and `key`. Return the bytes. Verify the file mode is `0o600`; if it is more permissive, `chmod 0o600` and emit a `tracing::warn!(target = "secrets.master-key.permissions-tightened")`.
   2. Otherwise, generate 32 random bytes, write `version=1\nkey=<b64>\n` with `OpenOptions::new().create_new(true).write(true).mode(0o600)`. Return the bytes.
2. `rotate`:
   1. Read the current version. Generate a new key. Append a second line `version=<n+1>\nkey=<b64>\n`. The file is keyed by version; on read, the highest version is returned and earlier ones are kept for re-encryption.
3. The vault constructor records `kind = "file"` in `secrets_metadata.backend_kind` and emits `tracing::warn!(target = "secrets.file-backend.startup", "the master key is on disk; back up <data_dir>/master-key out-of-band")`.

### Tests

- `core/crates/adapters/tests/file_backend_creates_file_mode_0600.rs` (Unix-only) — fresh data dir; first call creates the file; assert mode `0o600`.
- `core/crates/adapters/tests/file_backend_round_trip.rs` — write then read; assert byte equality.
- `core/crates/adapters/tests/file_backend_tightens_permissions.rs` — pre-create the file as `0o644`; assert it is reset to `0o600`.
- `core/crates/adapters/tests/file_backend_startup_warning_emitted.rs` — assert the startup warning fires when the vault selects this backend.

### Acceptance command

`cargo test -p trilithon-adapters file_backend_`

### Exit conditions

- The master-key file MUST be mode 0600.
- The startup warning MUST fire when the file backend is in use.

### Audit kinds emitted

None.

### Tracing events emitted

`secrets.file-backend.startup`, `secrets.master-key.permissions-tightened` (both flagged below as candidates for §12.1).

### Cross-references

- ADR-0014.
- PRD T1.15.

---

## Slice 10.5 — `0004_secrets.sql` migration plus `secrets_metadata` writer

### Goal

Land the `secrets_metadata` schema per architecture §6.9, plus an extension column `backend_kind TEXT NOT NULL DEFAULT 'keychain'`. Provide the storage adapter that inserts ciphertext rows.

### Entry conditions

- Slice 10.2 done.

### Files to create or modify

- `core/crates/adapters/migrations/0004_secrets.sql` — DDL.
- `core/crates/adapters/src/storage_sqlite/secrets.rs` — writer/reader.

### Signatures and shapes

```sql
CREATE TABLE IF NOT EXISTS secrets_metadata (
    id                TEXT PRIMARY KEY,             -- ULID
    owner_kind        TEXT NOT NULL,
    owner_id          TEXT NOT NULL,
    field_path        TEXT NOT NULL,
    nonce             BLOB NOT NULL,
    ciphertext        BLOB NOT NULL,
    algorithm         TEXT NOT NULL,
    key_version       INTEGER NOT NULL,
    backend_kind      TEXT NOT NULL DEFAULT 'keychain',
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,
    last_revealed_at  INTEGER,
    last_revealed_by  TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS secrets_metadata_owner_field
    ON secrets_metadata(owner_kind, owner_id, field_path);
CREATE INDEX IF NOT EXISTS secrets_metadata_key_version
    ON secrets_metadata(key_version);
```

```rust
use trilithon_core::secrets::{Ciphertext, EncryptContext, OwnerKind};

#[derive(Clone, Debug)]
pub struct SecretRow {
    pub id:               String,
    pub owner_kind:       OwnerKind,
    pub owner_id:         String,
    pub field_path:       String,
    pub ciphertext:       Ciphertext,
    pub backend_kind:     String,
    pub created_at:       i64,
    pub updated_at:       i64,
    pub last_revealed_at: Option<i64>,
    pub last_revealed_by: Option<String>,
}

pub async fn upsert_secret(
    conn:    &mut rusqlite::Connection,
    row:     &SecretRow,
) -> Result<(), StorageError>;

pub async fn get_secret(
    conn:    &mut rusqlite::Connection,
    id:      &str,
) -> Result<Option<SecretRow>, StorageError>;

pub async fn record_reveal(
    conn:       &mut rusqlite::Connection,
    id:         &str,
    revealer:   &str,
    now:        i64,
) -> Result<(), StorageError>;
```

### Algorithm

1. The migration runner applies `0004_secrets.sql`.
2. `upsert_secret` writes via `INSERT INTO secrets_metadata (...) ON CONFLICT (owner_kind, owner_id, field_path) DO UPDATE SET nonce=excluded.nonce, ciphertext=excluded.ciphertext, algorithm=excluded.algorithm, key_version=excluded.key_version, updated_at=excluded.updated_at`. Plaintext NEVER touches this function.
3. `get_secret` returns the full row.
4. `record_reveal` updates `last_revealed_at` and `last_revealed_by` only.

### Tests

- `core/crates/adapters/tests/secrets_migration_creates_table.rs` — schema introspection asserts the columns and indexes.
- `core/crates/adapters/tests/secrets_upsert_round_trip.rs` — insert, fetch, assert byte equality of ciphertext.
- `core/crates/adapters/tests/secrets_unique_owner_field.rs` — second upsert for the same `(owner_kind, owner_id, field_path)` overwrites, not duplicates.
- `core/crates/adapters/tests/secrets_record_reveal.rs` — assert `last_revealed_at` and `last_revealed_by` populate.

### Acceptance command

`cargo test -p trilithon-adapters secrets_migration_ secrets_upsert_ secrets_unique_owner_field secrets_record_reveal`

### Exit conditions

- The schema MUST include every architecture §6.9 column plus `backend_kind`, `algorithm`.
- The unique index MUST forbid two ciphertexts for the same `(owner, field)`.

### Audit kinds emitted

None directly.

### Tracing events emitted

`storage.migrations.applied`.

### Cross-references

- ADR-0014.
- PRD T1.15.
- Architecture §6.9.

---

## Slice 10.6 — `POST /api/v1/secrets/{secret_id}/reveal` with step-up auth

### Goal

Implement the reveal endpoint: an authenticated session re-enters the user's password (step-up), the vault decrypts, the plaintext is returned in the HTTP response, and one `secrets.revealed` audit row is written carrying the secret id, the actor, and the correlation id but NOT the plaintext.

### Entry conditions

- Slices 10.1, 10.2, 10.3, 10.4, 10.5 done.
- Phase 9 done (auth middleware, `AuthenticatedSession`).

### Files to create or modify

- `core/crates/adapters/src/http_axum/secrets_routes.rs` — handler.

### Signatures and shapes

```rust
#[derive(serde::Deserialize)]
pub struct RevealRequest { pub current_password: String }

#[derive(serde::Serialize)]
pub struct RevealResponse { pub plaintext: String }

pub async fn reveal(
    State(state): State<AppState>,
    session:      AuthenticatedSession,
    Path(id):     Path<String>,
    Json(req):    Json<RevealRequest>,
) -> Result<Json<RevealResponse>, ApiError>;
```

### Algorithm

1. The session MUST be `AuthContext::Session`; tokens MUST NOT reveal secrets. Otherwise return 403.
2. Verify `req.current_password` against the user's hash. On mismatch return 401 with `{ code: "step-up-required" }` and DO NOT write any audit row beyond the standard `auth.login-failed`-equivalent (write `auth.login-failed` against the same user with `notes.context = "step-up"`).
3. Load the secret row by `id`. If absent, 404.
4. Construct the `EncryptContext` from the row.
5. `let plaintext_bytes = vault.decrypt(&row.ciphertext, &context)?;`. Convert to `String` via `String::from_utf8(plaintext_bytes).map_err(|_| ApiError::Internal { detail: "non-utf8 secret".into() })?`.
6. `record_reveal(&id, &session.user_id, now)`.
7. Write one `secrets.revealed` audit row with `correlation_id`, `actor = ActorRef::User { id: user_id }`, `event = AuditEvent::SecretsRevealed`, `target_kind = Some("secret")`, `target_id = Some(id)`, `notes = Some(serde_json::json!({"owner_kind": ..., "owner_id": ..., "field_path": ...}).to_string())`. The plaintext MUST NOT appear.
8. Return 200 with `RevealResponse { plaintext }`. The response is over loopback by default; remote-binding deployments are responsible for their own transport security.

### Tests

- `core/crates/adapters/tests/reveal_returns_plaintext_with_password.rs` — encrypt a known value; reveal with correct password; assert the plaintext returns.
- `core/crates/adapters/tests/reveal_without_password_401.rs` — empty `current_password`; 401.
- `core/crates/adapters/tests/reveal_wrong_password_401.rs` — wrong password; 401.
- `core/crates/adapters/tests/reveal_writes_audit_row_without_plaintext.rs` — assert the audit row exists, plaintext substring is absent from `redacted_diff_json`, `notes`, and any other column.
- `core/crates/adapters/tests/reveal_token_session_forbidden.rs` — bearer token session; 403.
- `core/crates/adapters/tests/reveal_unknown_id_404.rs` — 404.

### Acceptance command

`cargo test -p trilithon-adapters reveal_`

### Exit conditions

- Reveal MUST require a session (not a token) and MUST require step-up password verification.
- Reveal MUST write exactly one `secrets.revealed` audit row.
- The plaintext MUST NOT appear in any audit column.

### Audit kinds emitted

`secrets.revealed` (architecture §6.6). Failed step-up writes `auth.login-failed` with a `notes.context = "step-up"` discriminator.

### Tracing events emitted

`http.request.received`, `http.request.completed`.

### Cross-references

- ADR-0014.
- PRD T1.15.
- Architecture §6.6, §11.

---

## Slice 10.7 — Wire mutation pipeline through the vault and feed redactor with ciphertext hash

### Goal

Every mutation that carries a secret-marked field MUST route the field through the vault before the snapshot is computed. The snapshot stores `{ "$secret_ref": "<secret_id>" }` in place of the plaintext; the redactor (Phase 6) hashes the ciphertext to produce a stable prefix in `redacted_diff_json`.

### Entry conditions

- Slices 10.5, 10.6 done.
- Phase 6 redactor in place.

### Files to create or modify

- `core/crates/core/src/mutation/secrets_extract.rs` — pure-core walker that identifies secret leaves in a `Mutation` payload.
- `core/crates/adapters/src/mutation_queue/secrets.rs` — adapter that calls `vault.encrypt` and rewrites the payload.
- `core/crates/core/src/audit/redactor.rs` — extension: a `CiphertextHasher` impl backed by the vault's stored ciphertext.

### Signatures and shapes

```rust
// core/crates/core/src/mutation/secrets_extract.rs
use crate::diff::JsonPointer;
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct ExtractedSecret {
    pub field_path: JsonPointer,
    pub plaintext:  String,
}

/// Walks a mutation payload, returns every secret-marked leaf with its path.
/// Pure-core; no IO, no encryption.
pub fn extract_secrets(
    payload: &Value,
    schema:  &crate::schema::SchemaRegistry,
) -> Vec<ExtractedSecret>;

/// In-place rewrite. Replaces every secret leaf with `{ "$secret_ref": <id> }`.
pub fn substitute_secret_refs(
    payload: &mut Value,
    sites:   &[(JsonPointer, String /* secret_id */)],
);
```

```rust
// core/crates/adapters/src/mutation_queue/secrets.rs
pub async fn route_mutation_secrets_through_vault(
    payload:        &mut serde_json::Value,
    owner_kind:     trilithon_core::secrets::OwnerKind,
    owner_id:       &str,
    schema:         &trilithon_core::schema::SchemaRegistry,
    vault:          &dyn trilithon_core::secrets::SecretsVault,
    storage:        &dyn trilithon_core::storage::Storage,
) -> Result<u32 /* secrets stored */, MutationSecretsError>;
```

A new `CiphertextHasher` impl:

```rust
pub struct VaultBackedHasher<'a> {
    pub storage: &'a dyn trilithon_core::storage::Storage,
}

impl<'a> trilithon_core::audit::redactor::CiphertextHasher for VaultBackedHasher<'a> {
    fn hash_for_value(&self, plaintext: &str) -> String;
}
```

### Algorithm

1. Mutation handler in Phase 9 (slice 9.7) calls `route_mutation_secrets_through_vault` BEFORE constructing the snapshot.
2. The function:
   1. Calls `extract_secrets(payload, schema)` to collect every secret leaf with its `field_path`.
   2. For each leaf:
      - Generate `secret_id = Ulid::new().to_string()`.
      - Build `EncryptContext { owner_kind, owner_id, field_path, key_version }`.
      - `vault.encrypt(plaintext.as_bytes(), &context)?`.
      - `upsert_secret(&row).await?` (slice 10.5).
   3. Calls `substitute_secret_refs(payload, &sites)` so the payload now carries `{"$secret_ref": "<id>"}` at every secret site.
3. The snapshot computed by Phase 5 over the rewritten payload contains zero plaintext bytes.
4. The redactor's `VaultBackedHasher::hash_for_value` looks up the secret by hashing the canonicalised plaintext and indexing into a per-process cache that maps plaintext-hash → ciphertext-hash. Since the redactor sees the rewritten payload, it can extract the `$secret_ref` directly: a future iteration MAY drop the hasher and use the ref. For now, the rewritten payload renders `{"$secret_ref": "<id>"}` and the redactor outputs `***<first-12-of-sha256(secret_id)>`.

### Tests

- `core/crates/adapters/tests/mutation_secrets_routed_through_vault.rs` — submit `CreateRoute` with a basic-auth password; assert (a) the snapshot's `desired_state_json` contains `$secret_ref` but not the plaintext; (b) `secrets_metadata` carries one row; (c) the row's ciphertext decrypts back to the plaintext.
- `core/crates/adapters/tests/mutation_secrets_redacted_in_audit.rs` — assert the audit row's `redacted_diff_json` contains `***` and zero bytes of the plaintext.
- `core/crates/adapters/tests/leaked_sqlite_does_not_leak_secrets.rs` — copy the SQLite file to a temp location; without the master key, attempt every recovery method (parsing `secrets_metadata`, dumping `snapshots`, dumping `audit_log`); assert no plaintext is recovered.
- `core/crates/adapters/tests/identical_secrets_identical_hash_prefix.rs` — encrypt the same plaintext twice (different secret_ids); assert the redacted hash prefix is identical for the same `secret_id` but the ciphertexts differ (different nonces). Wait — the canonical interpretation per the phase reference is "identical secrets MUST produce identical hash prefixes" — re-read: the redactor's stable hash is over the ciphertext. Different nonces produce different ciphertexts, so the stable hash differs. The phase reference's "identical secrets" must therefore mean "identical secret references (same row)". The test asserts: same `secret_id` → same hash prefix across multiple redactor invocations; different `secret_id` for the same plaintext → different hash prefixes (this is acceptable because the audit log tracks per-row identity).

### Acceptance command

`cargo test -p trilithon-adapters mutation_secrets_ leaked_sqlite_does_not_leak_secrets identical_secrets_`

### Exit conditions

- Every Tier 1 secret-marked field MUST flow through the vault.
- The snapshot's `desired_state_json` MUST contain zero plaintext bytes for any secret.
- A leaked SQLite file MUST NOT permit secret recovery without the master key.

### Audit kinds emitted

The mutation pipeline emits the standard `mutation.applied` / `config.applied` rows. No new kind is introduced.

### Tracing events emitted

None new.

### Cross-references

- ADR-0014.
- PRD T1.15.
- Hazard H10.
- Architecture §6.9.

---

## Phase exit checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] All secret-marked fields are stored encrypted at rest under XChaCha20-Poly1305 (slices 10.2, 10.7).
- [ ] The master key lives outside the SQLite database; the keychain backend is the default on macOS and Linux (slices 10.3, 10.4).
- [ ] Reveal produces an audit row and requires step-up authentication (slice 10.6).
- [ ] A copy of the SQLite file alone is not sufficient to recover any secret (slice 10.7 leak test).
- [ ] The redactor is the only path between the diff engine and the audit log writer; no code path bypasses it (Phase 6 enforcement plus slice 10.7's vault wiring).
- [ ] `core/README.md` adds a "Secrets" section citing ADR-0014 and stating the master-key-loss equals data-loss invariant.

## Open questions

- The phase reference's "identical secrets MUST produce identical hash prefixes" admits two readings: per-plaintext or per-row. This breakdown chooses per-row (slice 10.7 test). The project owner SHOULD ratify before Phase 11 surfaces redacted hashes in the UI.
- `secrets.master-key.permissions-tightened` and `secrets.file-backend.startup` are tracing events not yet listed in architecture §12.1. The slices flag adding them.
- Master-key rotation flow (slice 10.3 `rotate`) is sketched but the cross-cutting "re-encrypt every ciphertext" walk is not surfaced as an HTTP endpoint in V1; it lives as a daemon command. Whether to expose `POST /api/v1/secrets/master-key/rotate` is a Phase 27 hardening question.
