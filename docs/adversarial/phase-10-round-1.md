# Adversarial Review — Phase 10 (Secrets Vault) — Round 1

**Design summary:** Phase 10 adds a local-first secrets vault to Trilithon, encrypting secrets with XChaCha20-Poly1305, backed by OS keychain or file fallback, with step-up auth on reveal and a redactor that prevents plaintext from reaching the audit log.

**Prior rounds:** None — this is round 1.

---

## Findings

### R1-F01 — CRITICAL: No envelope encryption means key rotation re-encrypts every row atomically or loses data
**Category:** Rollbacks
**Trigger:** The operator runs `rotate_master_key()`. The implementation does not use envelope encryption (which the ADR mandated so that rotation only re-wraps one key). Instead, every `secrets_metadata` row must be decrypted with the old master key and re-encrypted with the new key. The TODO sketches no atomic transaction for this bulk operation and acknowledges the gap only in prose.
**Consequence:** If the process crashes, is killed, or the database is locked mid-rotation, some rows are re-encrypted under the new key and some remain encrypted under the old key — which may already be deleted from the keychain. Those rows become permanently unreadable. The vault loses secrets without any warning.
**Design assumption violated:** The ADR explicitly states rotation is cheap because the envelope structure means data keys are not re-encrypted. Removing envelope encryption invalidates the rotation design entirely.
**Suggested mitigation:** Either (a) restore envelope encryption as ADR-0014 mandates so rotation only re-wraps one key-encryption-key, or (b) add a dedicated atomic rotation slice that wraps all row re-encryption in a single SQLite transaction with a `rotation_state` table tracking which rows have been migrated, and ensures the old master key is not deleted from the keychain until the transaction commits.

---

### R1-F02 — CRITICAL: Three-layer violation — `rotate_master_key` in core implies database I/O
**Category:** State Manipulation
**Trigger:** `MasterKeyRotation { re_encrypted_rows }` is returned from `rotate_master_key()`, a method declared on `SecretsVault` in `core`. Without envelope encryption, computing `re_encrypted_rows` requires reading all `secrets_metadata` rows and writing them back — both database I/O operations. The `core` layer is forbidden from holding a database connection.
**Consequence:** Either (a) the implementation silently breaks the three-layer rule by passing a `rusqlite::Connection` into core, creating an unliftable dependency, or (b) the caller in `adapters`/`cli` has to do a separate re-encryption loop that is not coordinated with the key write, introducing the same partial-rotation race described in F01.
**Design assumption violated:** `core` has no I/O. A vault trait that must re-encrypt stored rows during rotation cannot be in core without a connection handle.
**Suggested mitigation:** The rotation API must be split: `core` provides a pure `re_encrypt_blob(old_key, new_key, ciphertext) -> Ciphertext` function; the `adapters` layer owns the loop that reads rows, calls the core function, and writes back within a transaction. `rotate_master_key` is removed from the `SecretsVault` trait entirely.

---

### R1-F03 — CRITICAL: Linux keychain temporary failure causes permanent key downgrade, making existing ciphertexts unreadable
**Category:** Single Points of Failure
**Trigger:** On Linux, the vault constructor calls `KeychainBackend::load_or_generate`. If the Secret Service D-Bus is temporarily unavailable (daemon starting, D-Bus restart, snap confinement), the constructor detects failure and falls back to `FileBackend`, generating and storing a new master key in the file. The old master key remains in the Secret Service.
**Consequence:** All secrets encrypted under the keychain master key are now unreadable, because the file backend holds a different key. On the next process start (when D-Bus is available), the constructor will use whichever backend is tried first — if it switches back to keychain, the file-encrypted secrets are unreadable. The design does not track which backend encrypted which rows.
**Design assumption violated:** "D-Bus not available" is treated as equivalent to "should use file backend," but "temporarily unavailable" and "permanently absent" are indistinguishable.
**Suggested mitigation:** (a) Add a `backend_kind` column to `secrets_metadata` (the TODO adds this but doesn't wire it to key resolution) and validate at startup that the active backend matches all existing rows' `backend_kind`. (b) Distinguish transient errors (timeout, no reply) from structural errors (no Secret Service installed) — only the latter should trigger file fallback. Any fallback must be gated on zero existing rows encrypted under the keychain backend.

---

### R1-F04 — CRITICAL: `FileBackend` rotation writes are non-atomic — mid-write crash corrupts the only key store
**Category:** Rollbacks
**Trigger:** Slice 10.4 describes rotation by appending a new `version=N\nkey=<b64>\n` stanza to the existing file. An OS-level write is not guaranteed atomic beyond the page size. If the process crashes or is killed after writing the new version header but before writing the full key bytes, the file contains a partial stanza.
**Consequence:** The parser encounters a truncated stanza. Depending on error handling, either the entire file fails to parse (all secrets become unreadable) or the partial stanza is silently ignored (the new version is lost and the old key remains active, confusing rotation accounting). No recovery path is defined.
**Design assumption violated:** The design assumes append-to-file is safe for key material. It is not — file appends on Linux/macOS are not atomic beyond PIPE_BUF.
**Suggested mitigation:** Write new key versions atomically: write to a temp file with a `.tmp` suffix on the same filesystem, `fsync` it, then `rename()` it over the existing file. `rename()` is POSIX-atomic. The version history (if needed) must be maintained differently — not by accumulating stanzas in one file.

---

### R1-F05 — CRITICAL: Step-up auth on reveal has no rate-limiting or lockout — brute-force against loopback
**Category:** Authentication & Authorization
**Trigger:** `POST /api/v1/secrets/{secret_id}/reveal` accepts `current_password` in the request body and performs synchronous bcrypt/argon2 verification. The design states the endpoint is loopback-only per ADR-0011 as an "operational assumption," not enforced in the handler. Any local process (malicious binary, compromised plugin, script with loopback access) can POST to this endpoint in a loop.
**Consequence:** A local attacker can enumerate passwords without any throttle. bcrypt slows each attempt, but with a weak password and enough attempts, the step-up auth is broken. There is no lockout after N failures, no exponential backoff, and no alert emitted to the audit log on repeated failures.
**Design assumption violated:** The design assumes the loopback interface is a trust boundary. It is not — all local processes can reach loopback.
**Suggested mitigation:** Add an in-process atomic counter keyed by `secret_id` (or global) that locks out further attempts after 5 consecutive failures, with a mandatory 30-second cooldown. Write a `secrets.step_up_failed` audit row on each failure. Optionally enforce the loopback restriction in the handler (not just in docs) by inspecting the remote address.

---

### R1-F06 — CRITICAL: Redaction hash is stable on `secret_id` (a ULID), not on plaintext — breaks ADR guarantee
**Category:** Data Exposure
**Trigger:** ADR-0014 mandates that the stable placeholder is `SHA-256(plaintext)` prefixed `secret:`. This allows auditors to confirm, given the plaintext, that a specific audit row refers to that value. The TODO instead uses `SHA-256(secret_id)[0:12]`, which is a ULID hash.
**Consequence:** (a) The audit log no longer satisfies the ADR-0014 verifiability guarantee. (b) All audit rows for a given secret carry the same hash regardless of whether the value changed. A rotation that changes the plaintext would produce identical hashes in audit. (c) If the design is later corrected to hash the plaintext, old and new rows use different schemes, silently breaking audit tooling.
**Design assumption violated:** The stable hash is supposed to enable offline linkage between audit rows and plaintext values. Hashing the ULID provides no such linkage.
**Suggested mitigation:** Hash the plaintext as ADR-0014 specifies. If the concern is that hashing the plaintext creates a long-lived in-memory copy, address that in the `VaultBackedHasher` design (see F09), not by switching to a secret_id hash.

---

### R1-F07 — HIGH: `upsert_secret` takes `rusqlite::Connection` directly — breaks three-layer rule
**Category:** State Manipulation
**Trigger:** `upsert_secret(conn: &mut rusqlite::Connection, ...)` — this couples the storage adapter directly to rusqlite rather than going through the `Storage` trait boundary. Any code that calls this function cannot use an in-memory or mock storage backend.
**Consequence:** (a) Core (or shared module) now depends on rusqlite, violating the architecture rule. (b) The `Storage` trait boundary is bypassed — future storage backends cannot be substituted.
**Design assumption violated:** The `Storage` trait exists precisely to mediate this boundary.
**Suggested mitigation:** Replace the `rusqlite::Connection` parameter with a method on the `Storage` trait: `fn upsert_secret(&mut self, record: SecretRecord) -> Result<(), StorageError>`. The rusqlite implementation lives in `adapters`.

---

### R1-F08 — HIGH: Missing `master_key_initialised` audit row — ADR-0014 compliance gap
**Category:** Orphaned Data
**Trigger:** ADR-0014 explicitly mandates that master key generation is recorded as a `master_key_initialised` event. No slice handles this. Slice 10.3 generates the key but emits no audit event.
**Consequence:** If the master key was tampered with or replaced, there is no audit record of when the legitimate key was created. Compliance review of the audit log will show secrets being created with no prior key initialization record.
**Design assumption violated:** The audit log is the source of truth for all security-relevant events. Key initialization is a security-relevant event.
**Suggested mitigation:** Add an explicit task to slice 10.3: after generating the master key, emit a `master_key_initialised` event to the audit log before the vault is used for any encrypt/decrypt operation.

---

### R1-F09 — HIGH: `VaultBackedHasher` per-process cache holds plaintext-derived material with no eviction
**Category:** Data Exposure
**Trigger:** Slice 10.7 introduces a per-process cache mapping `plaintext-hash → ciphertext-hash`. For this map to be populated, the vault must compute `SHA-256(plaintext)` and store it as a key. There is no defined eviction policy.
**Consequence:** If plaintext hash is retained as a map key without zeroing, a heap dump or memory forensics tool can recover it and, for low-entropy secrets, reverse it. If the cache grows unbounded, it becomes a permanent in-process log of every plaintext secret's hash — surviving across secret rotations.
**Design assumption violated:** Plaintext MUST NOT persist in memory beyond the immediate operation.
**Suggested mitigation:** Either (a) eliminate the cache and recompute the hash on each redaction (cheap), or (b) make the cache a bounded LRU keyed by `secret_id + version`, not by plaintext hash, with zero-on-eviction.

---

### R1-F10 — HIGH: `async` keychain backend bridged into sync trait with no design for sync `rotate_master_key`
**Category:** Logic Flaws
**Trigger:** `KeychainBackend::load_or_generate` is `async`. The `SecretsVault` trait methods are synchronous. `rotate_master_key` is also synchronous but must write the new key back to the keychain, which requires an async call.
**Consequence:** Either (a) `rotate_master_key` blocks the Tokio thread with `block_on`, risking deadlock, or (b) the new key is written to the file backend but not the keychain, creating a split-brain key store, or (c) key rotation silently succeeds but the keychain still holds the old key.
**Design assumption violated:** The design assumes a sync trait can wrap an async backend without specifying how the async-to-sync boundary is crossed for write operations.
**Suggested mitigation:** Make `rotate_master_key` async on the `SecretsVault` trait, or split into a sync `Encryptor`/`Decryptor` and an async `KeyManager`. The async/sync boundary must be explicit.

---

### R1-F11 — HIGH: `getrandom` failure mapped to `CryptoError::Decryption` — wrong error variant
**Category:** Logic Flaws
**Trigger:** Nonce generation maps `getrandom` failure to `CryptoError::Decryption`. A CSPRNG failure is an entropy failure, not a decryption failure.
**Consequence:** An operator diagnosing entropy exhaustion sees "decryption failed" and wastes time investigating ciphertext or key when the real issue is missing entropy. A caller that retries on `Decryption` would retry forever.
**Suggested mitigation:** Add `CryptoError::EntropyFailure(String)` and map `getrandom` errors to it.

---

### R1-F12 — HIGH: `SecretsVault::redact` return type mismatch between TODO and trait-signatures.md
**Category:** Logic Flaws
**Trigger:** The TODO defines `redact` returning `RedactionResult`. The `trait-signatures.md §3` document defines it as returning `RedactedValue`. Two authoritative documents disagree.
**Consequence:** Different implementations will have incompatible `redact` signatures. If the types involve non-object-safe bounds, `Box<dyn SecretsVault>` fails to compile at usage sites, not at the trait definition — making the error hard to track.
**Suggested mitigation:** Resolve the canonical return type before slice 10.1 begins. Add a compile-time test `fn _assert_object_safe(_: &dyn SecretsVault) {}` at the trait definition site.

---

### R1-F13 — HIGH: Schema column `field_name` vs `field_path` divergence can break migration ordering
**Category:** State Manipulation
**Trigger:** The Phase 10 pre-flight task references `field_name` while the migration in slice 10.5 uses `field_path`. Application code compiled against one column name and database schema using the other will panic or error on first access.
**Suggested mitigation:** Pick one name (`field_path`) and audit every reference in the TODO, architecture docs, and any existing query strings before the migration slice is written.

---

### R1-F14 — MEDIUM: Reveal endpoint audit row has no type-level enforcement against plaintext inclusion
**Category:** Data Exposure
**Trigger:** The `RedactedDiff` newtype protects the diff path but not the reveal endpoint path. The reveal handler has the decrypted plaintext in scope when it writes the audit row; there is no type-system enforcement preventing accidental inclusion.
**Consequence:** A developer who accidentally logs the response body or includes the plaintext in the `notes` column would not be caught at compile time.
**Suggested mitigation:** Define a `RevealAuditRecord` newtype constructable only via a function that takes `secret_id`, `revealed_by`, and timestamp — no plaintext field. The type system then prevents leakage on the reveal path.

---

### R1-F15 — MEDIUM: `leaked_sqlite_does_not_leak_secrets` test cannot catch partial-rotation exposure
**Category:** Race Conditions
**Trigger:** The test verifies a SQLite dump does not expose plaintext but does not cover the state during a partial rotation (some rows re-encrypted under new key, old key deleted).
**Suggested mitigation:** Add a rotation integration test that kills the process mid-rotation, verifies all rows can still be decrypted, and verifies no row is left in an unreadable state.

---

### R1-F16 — MEDIUM: No nonce uniqueness enforcement — nonce reuse under the same key breaks XChaCha20-Poly1305 confidentiality
**Category:** Logic Flaws
**Trigger:** Nonces are generated via `getrandom` with no uniqueness check. With a 24-byte random nonce, birthday collision probability is negligible for a single workspace — but is realistic if the master key is reused across workspaces (copied file backend) or if the CSPRNG is weak.
**Suggested mitigation:** Add a UNIQUE constraint on `(nonce)` within `secrets_metadata`. On insert, if the unique constraint fires, regenerate and retry (max 3 attempts before hard error).

---

### R1-F17 — MEDIUM: Associated data constructed from stored row rather than caller context defeats row-swap protection
**Category:** Authentication & Authorization
**Trigger:** If the decrypt wrapper constructs associated data from the row it just fetched (rather than from the caller's expected context), a tampered row (swapped `field_path` or `owner_id` by SQL injection or direct DB edit) will decrypt successfully rather than failing AEAD authentication.
**Suggested mitigation:** The `decrypt` function signature must require the caller to pass `field_path` and `owner_id` explicitly from the request context, not from the DB row. This must be specified in the `EncryptContext` design in slice 10.1.

---

### R1-F18 — LOW: `rotated_at` column dropped in favor of `updated_at` — rotation history lost at schema level
**Category:** Orphaned Data
**Trigger:** The architecture spec defines `rotated_at` as a column. The TODO drops it and uses `updated_at`. Any update (not just rotation) sets `updated_at`, making it impossible to distinguish a metadata update from a key rotation at the schema level.
**Suggested mitigation:** Retain both columns, or explicitly record the decision to drop `rotated_at` in the ADR and switch audit tooling to the audit log for rotation timing.

---

## Summary

**Critical:** 6 · **High:** 6 · **Medium:** 4 · **Low:** 1

**Top concern:** The removal of envelope encryption from the implementation plan (contradicting ADR-0014) makes key rotation an all-or-nothing bulk re-encryption with no atomicity guarantee, no three-layer-clean API, and a crash window that permanently destroys secrets — this is the most dangerous gap in the design.
