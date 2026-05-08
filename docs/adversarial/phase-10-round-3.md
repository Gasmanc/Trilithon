# Adversarial Review — Phase 10 (Secrets Vault) — Round 3

**Design summary:** Phase 10 adds an XChaCha20-Poly1305 secrets vault to Trilithon, sourcing the master key from the OS keychain or a 0600 file fallback, routing every secret-marked mutation field through the vault before snapshot, and exposing a step-up-authenticated reveal endpoint.

**Prior rounds:** 17 findings from Round 1 and 14 from Round 2, all unaddressed. This round attacks new areas and composition failures from proposed mitigations.

---

## Findings

### R3-F01 — CRITICAL: `substitute_secret_refs` partial-write orphans revealable ciphertext rows with no snapshot reference

**Category:** Rollbacks / Orphaned Data

**Trigger:** `route_mutation_secrets_through_vault` processes N secret leaves. For leaves 1 and 2, `vault.encrypt` succeeds and `upsert_secret` commits the rows to `secrets_metadata`. For leaf 3, `vault.encrypt` returns `CryptoError` (entropy failure, `KeyMissing`). The function returns `Err`. The mutation is abandoned. No snapshot is written.

**Consequence:** The two committed `secrets_metadata` rows are fully revealable via `POST /api/v1/secrets/{secret_id}/reveal` — any session-authenticated user who learns the generated ULID can extract the plaintext. They have no corresponding snapshot row, no route reference, and no deletion pathway (R2-F05). Over repeated transient failures on N-secret routes, permanently revealable orphaned rows accumulate in proportion to failure frequency × secrets-per-route.

**Design assumption violated:** The design assumes `route_mutation_secrets_through_vault` is atomic across all N leaves: either all rows are committed and the payload is fully substituted, or nothing is committed. The actual structure — sequential `upsert_secret` calls outside any transaction — has no such atomicity.

**Suggested mitigation:** Wrap the entire `route_mutation_secrets_through_vault` call in a SQLite savepoint. If any `vault.encrypt` or `upsert_secret` call fails, roll back the savepoint before returning `Err`. Alternatively, collect all (plaintext, context, id) tuples first, verify all encryptions succeed, then write all rows in a single transaction.

---

### R3-F02 — CRITICAL: Bootstrap credentials (Hazard H13) bypass the vault on first run

**Category:** Authentication & Authorization

**Trigger:** ADR-0014 explicitly covers H13 — bootstrap credentials. Phase 10 introduces the vault through which all secret-marked fields must flow. But no slice routes bootstrap credential generation through the vault. The architecture records `auth.bootstrap-credentials-created` as an audit kind and states credentials shall be written to a 0600 file — but the vault is the designated encrypted store for secrets.

**Consequence:** Bootstrap credentials live outside the vault, protected only by filesystem mode 0600. A backup that captures the data directory (per T2.12) captures the credentials in plaintext. This contradicts the ADR-0014 threat model: "a leaked file does not leak secrets." The data directory is semi-public with respect to backups; the SQLite file is the named threat, but the 0600 file is in the same directory under the same backup scope.

**Design assumption violated:** The design implicitly treats the 0600 file as an acceptable substitute for vault encryption for bootstrap credentials. The ADR does not carve this out.

**Suggested mitigation:** After vault initialisation, generate bootstrap credentials and encrypt them under the vault. Store the ciphertext in `secrets_metadata` with `owner_kind = User`, `owner_id = "bootstrap"`, `field_path = "/password"`. Write plaintext to the 0600 file only as a transient delivery mechanism. When the user changes their password, delete the 0600 file and record the deletion in the audit log.

---

### R3-F03 — CRITICAL: Cross-machine restore (T2.12) produces silently broken desired state when master key is absent

**Category:** State Manipulation

**Trigger:** A user restores a SQLite backup to a new machine. Every snapshot's `desired_state_json` contains `{ "$secret_ref": "<ulid>" }` markers. The `secrets_metadata` rows contain ciphertexts encrypted under the old machine's master key. The new machine has no matching master key.

**Consequence:** The desired state is structurally valid JSON — `system.restore-applied` is emitted, the desired-state pointer advances. When Trilithon applies the desired state to Caddy, it resolves `$secret_ref` markers. Every decryption fails with `CryptoError::Decryption` or `CryptoError::KeyMissing`. The behavior at this point is unspecified: does the applier refuse? Does it pass the literal `$secret_ref` string to Caddy, making every basic-auth route non-functional without error? The `system.restore-cross-machine` audit kind is documented but no enforcement ensures the master-key handoff happened before the restore completes.

**Design assumption violated:** The design assumes restore and vault are decoupled. On a different machine they have an unresolved dependency that the design does not enforce.

**Suggested mitigation:** The restore endpoint must probe vault readiness before advancing the desired-state pointer: enumerate `secrets_metadata` key versions, attempt `vault.decrypt` on one ciphertext per version, and refuse the restore with a structured error listing missing key versions if any fail. Surface `system.restore-cross-machine` only after this check passes.

---

### R3-F04 — HIGH (composition with R2-F03 mitigation): INSERT-new-row mitigation breaks snapshot deduplication — every mutation on a secret-bearing route creates a new snapshot row

**Category:** Logic Flaws

**Trigger:** R2-F03 proposes always INSERTing a new row (new ULID) on secret update instead of overwriting. If a new ULID is generated unconditionally — even when the plaintext is unchanged (e.g., a no-op edit to a non-secret field on the same route) — the `$secret_ref` ULID in `desired_state_json` changes. The SHA-256 of canonical JSON therefore changes. ADR-0009's deduplication check ("if a snapshot with this hash exists, skip the insert") never matches.

**Consequence:** Every mutation on a secret-bearing route creates a new snapshot row, a new `secrets_metadata` row, and a new audit row — even for identical config. A CI/CD system polling `PUT /api/v1/routes/{id}` with unchanged values at 60-second intervals generates 10 rows per minute per route, permanently. The snapshot table grows without bound, defeating the "identical snapshots deduplicate" invariant.

**Design assumption violated:** The INSERT-new-row mitigation assumes the ULID is opaque to snapshot deduplication. It is not — the ULID appears literally in `desired_state_json` and participates in the SHA-256 hash.

**Suggested mitigation:** Assign a stable `secret_id` per `(owner_kind, owner_id, field_path)` tuple. On mutation, check whether the new plaintext equals the currently stored plaintext (via HMAC or decrypt-and-compare). If unchanged, reuse the existing `secret_id` and skip the `upsert_secret` call. Only generate a new row when the plaintext actually changes. This preserves both the rollback fidelity (old rows are not overwritten) and the deduplication invariant.

---

### R3-F05 — HIGH: LLM tool gateway can receive plaintext secrets through the `get_route` read tool if the route getter resolves `$secret_ref`

**Category:** Authentication & Authorization

**Trigger:** ADR-0014 prohibits revealing secrets through the language-model tool gateway. ADR-0008 states the gateway exposes "a strict subset of the read API." If `GET /api/v1/routes/{id}` resolves `$secret_ref` markers to plaintext for display (to present the operator with a complete route view), then an LLM tool call to `get_route` returns the same response — containing the plaintext. This is not a `reveal_secret` call; it writes no `secrets.revealed` audit row; it is not step-up authenticated.

**Consequence:** A language model with read-only scope (T2.3) receives plaintext secrets. Any prompt injection payload embedded in route data (hazard H16) can extract the plaintext through the gateway without triggering the reveal audit trail.

**Design assumption violated:** The design assumes secrets stay encrypted in all read paths except the explicit reveal endpoint. The read handlers are not audited for implicit decryption in any slice.

**Suggested mitigation:** All read handlers (not just the reveal endpoint) MUST return `$secret_ref` markers verbatim, never resolved. Add a `SecretField` enum with a `Redacted(String)` variant (serialises to `{ "$secret_ref": "<id>" }`) and a `Plaintext(String)` variant (used only in reveal response types). The compiler then prevents read handlers from returning a `Plaintext` variant without an explicit vault call.

---

### R3-F06 — HIGH: Audit hash-chain startup check has an unspecified ordering dependency with vault initialisation

**Category:** Race Conditions

**Trigger:** ADR-0009 requires the hash-chain check to run at daemon startup. If R1-F08 is fixed and `master_key_initialised` is written on vault init, the required order is: vault init → write audit row → chain check. If vault init and chain check run in parallel (both are async, both access SQLite), the chain check may execute before or after the `master_key_initialised` row commits.

**Consequence:** If the chain check reads before the row commits (SQLite snapshot isolation), it validates N rows as a complete chain. Then the row commits — creating an N+1 row that the check never saw. On the next startup, the chain check sees an additional row whose `prev_hash` is the all-zeros sentinel for "first row" but which is not the first row — the chain appears broken. The daemon may refuse to start, requiring manual recovery.

**Design assumption violated:** The design treats vault init and chain check as independent startup tasks with no specified sequencing barrier.

**Suggested mitigation:** Explicitly serialise the startup sequence: migrations first, then vault init (including any audit row writes), then hash-chain check, then begin serving requests. Document this order with a comment referencing the dependency.

---

### R3-F07 — HIGH: `CipherCore::from_key_bytes([u8; 32])` copies key bytes onto the call stack — key material in two unzeroed locations

**Category:** Data Exposure

**Trigger:** `from_key_bytes(bytes: [u8; 32], key_version: u32) -> Self` takes a `[u8; 32]` by value. Since `[u8; 32]: Copy`, the array is copied onto the `from_key_bytes` stack frame. When the function returns, the stack frame is popped but not zeroed. The raw key bytes remain on the stack until overwritten by a later call.

**Consequence:** Combined with R2-F07 (heap `Key` not zeroized on drop), the master key bytes exist in at least two locations: the heap-allocated `Key` in `CipherCore` and the stack region from `from_key_bytes`. In a Tokio work-stealing runtime, the idle thread stack is accessible via `/proc/<pid>/task/<tid>/mem` on Linux. A heap dump or core file captures both.

**Suggested mitigation:** Change the signature to `from_key_bytes(bytes: &[u8; 32], ...)` (borrow, not copy). Alternatively, accept `zeroize::Zeroizing<[u8; 32]>` so the caller's copy is zeroed on drop. The borrow approach is simplest and has no runtime cost.

---

### R3-F08 — MEDIUM: Argon2 step-up verify short-circuits on user-not-found before running Argon2 — timing oracle for account existence

**Category:** Authentication & Authorization

**Trigger:** The reveal handler loads the user's stored hash and calls Argon2 verify. If the user record is not found (deleted user with a valid session, race with concurrent account deletion), the handler returns 401 before running Argon2. An attacker on loopback measuring handler latency observes sub-millisecond response for non-existent users versus ~64ms for existing users with wrong passwords.

**Consequence:** The timing oracle leaks account existence. Under a session token obtained before account deletion, the attacker can confirm deletion timing and probe for valid usernames. If the step-up endpoint is ever reachable without a valid session (auth middleware misconfiguration), the oracle reveals valid usernames unconditionally.

**Suggested mitigation:** Always run Argon2 on a sentinel hash when the user record is not found. Store a pre-computed sentinel hash at startup (using a stable dummy input) and pass it to `argon2::verify_password` on the miss path. The response is 401 regardless, and latency is indistinguishable from a valid-user wrong-password case.

---

### R3-F09 — MEDIUM: `SecretRow.ciphertext` BLOB deserialization error surfaces as `StorageError` — decrypt path loses error semantics and does not trigger integrity-check event

**Category:** Logic Flaws

**Trigger:** `get_secret` deserializes the `ciphertext` BLOB column to a `Ciphertext` struct. If the BLOB is corrupted (SQLite page fault, bitflip, truncated write), deserialization fails as `StorageError::Deserialization`. The vault's decrypt call site receives a `StorageError` rather than a `CryptoError`, losing the distinction between "ciphertext row does not exist," "ciphertext bytes are corrupt," and "authentication tag mismatch."

**Consequence:** An operator sees `storage_error: deserialization_failed` with no indication whether the cause is data corruption or a key mismatch. The `storage.integrity-check.failed` tracing event (architecture §12.1) is never fired because the error is deserialization, not a SQLite integrity check. Corruption goes unreported.

**Suggested mitigation:** Map BLOB deserialization failures to `StorageError::CorruptedCiphertext { id, detail }`. The vault layer maps this to `CryptoError::Decryption { detail: "ciphertext blob malformed" }` and also fires `tracing::error!(target = "storage.integrity-check.failed", ...)`. This gives the operator an actionable signal distinct from a key mismatch.

---

### R3-F10 — MEDIUM: `VaultBackedHasher` holds a `Storage` reference that implies async I/O inside a synchronous `CiphertextHasher` trait — cache consistency unspecified

**Category:** Logic Flaws

**Trigger:** `VaultBackedHasher<'a> { storage: &'a dyn trilithon_core::storage::Storage }` implements the synchronous `CiphertextHasher` trait from `core::audit::redactor`. If `hash_for_value` needs a storage lookup and the `CiphertextHasher` trait is synchronous, the implementation must either block on async I/O (risking deadlock on the Tokio runtime) or use a pre-populated cache. The cache's invalidation policy when a secret is updated or deleted is unspecified.

**Consequence:** If the cache is not populated before `redact_diff` is called, `hash_for_value` returns an incorrect hash for secrets seen for the first time. If the mitigation direction is to avoid storage lookups entirely (use `secret_id` from the `$secret_ref` marker already in the rewritten payload), the `storage` field in `VaultBackedHasher` is unused but retained — creating a confusing struct that implies I/O but performs none.

**Suggested mitigation:** Remove the `storage` field from `VaultBackedHasher`. The redactor operates on the already-rewritten payload where `$secret_ref` markers are present; `hash_for_value` on this path is `sha256(secret_id)[..12]` with no storage access required. Document that `VaultBackedHasher` is marker-based, not lookup-based, and requires no storage reference.

---

### R3-F11 — LOW: `FileBackend::load_or_generate` partial-stanza recovery behavior after SIGKILL during rotation is unspecified

**Category:** State Manipulation

**Trigger:** `FileBackend::rotate` appends `version=<N+1>\nkey=<b64>\n`. A SIGKILL between writing the `version=` line and writing the `key=` line leaves a file with a syntactically partial terminal stanza. On next startup, `load_or_generate` parses the file. The design does not specify whether a partial final stanza is discarded (falling back to the previous complete stanza) or treated as a fatal parse error.

**Consequence:** If treated as fatal: the daemon refuses to start; the user must manually edit the key file with no documented procedure. If silently discarded: version accounting is inconsistent (the version number was incremented but the rotation did not complete). Note: R1-F04 flagged the non-atomic append itself; this finding probes specifically the read-side recovery when a partial stanza exists.

**Suggested mitigation:** `load_or_generate` must validate each stanza: both `version=` and `key=` lines must be present and the key must decode to exactly 32 bytes. A partial final stanza is discarded with a `tracing::warn!(target = "secrets.file-backend.partial-stanza-discarded")` event. Document this recovery behavior in a comment in the parser.

---

## Summary

**Critical:** 3 · **High:** 4 · **Medium:** 3 · **Low:** 1

**Top concern:** R3-F01 (partial-write orphaned ciphertext rows) is immediately exploitable under realistic transient failures — entropy exhaustion or `KeyMissing` errors during a multi-secret mutation leave permanently revealable rows with no snapshot reference, no deletion path, and no audit trail of their orphan status.
