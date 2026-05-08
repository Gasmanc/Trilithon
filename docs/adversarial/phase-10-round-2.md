# Adversarial Review — Phase 10 (Secrets Vault) — Round 2

**Design summary:** Phase 10 adds a secrets vault encrypting secret-marked fields with XChaCha20-Poly1305, a master key in the OS keychain (or file fallback), a step-up-authenticated reveal endpoint, and a mutation pipeline that substitutes `$secret_ref` pointers into snapshots before they are content-addressed.

**Prior rounds:** 17 findings from Round 1, all unaddressed. This round attacks new areas and composition failures from proposed Round 1 mitigations.

---

## Findings

### R2-F01 (composition with R1-F01 + R1-F02 mitigations) — CRITICAL: Envelope encryption first-run has no atomicity boundary and no owning slice

**Category:** State Manipulation

**Trigger:** R1-F01's mitigation restores envelope encryption (per ADR-0014): a workspace key is generated and stored in `secrets_metadata` encrypted with the master key. R1-F02's mitigation splits `rotate_master_key` into a pure core function plus an adapters loop. Neither mitigation specifies: (a) which table holds the workspace key (missing from the migration DDL in slice 10.5), (b) who generates and persists the workspace key on first run, or (c) what happens if the process crashes between writing the master key to the keychain and writing the encrypted workspace key to SQLite.

**Consequence:** On first-run crash between keychain write (succeeds) and workspace-key INSERT (not yet committed), the daemon restarts, finds a master key in the keychain, but no workspace key in the database. The vault has no specified behavior for "master key present, workspace key absent." If it generates a new workspace key, a second encryption root is silently created. If it errors, the daemon refuses to start with no documented recovery procedure.

**Design assumption violated:** The design assumes envelope encryption is a bolt-on to the existing schema. In reality, a `workspace_keys` table, a creation transaction, and a recovery procedure for partial-first-run are all required but absent from any slice.

**Suggested mitigation:** Add a `workspace_keys` table to the migration. Perform the keychain write BEFORE the SQLite transaction opens; wrap workspace-key derivation and INSERT in a single transaction. Add an explicit startup check: "master key present but no workspace key → emit recovery instructions and refuse to start."

---

### R2-F02 — CRITICAL: Migration `0004_secrets.sql` collides with existing migration at slot 4

**Category:** State Manipulation

**Trigger:** Slice 10.5 instructs creation of `core/crates/adapters/migrations/0004_secrets.sql`. The migrations directory already contains `0004_snapshots_immutable.sql` and `0005_canonical_json_version.sql` from prior phases. Any migration runner keyed on filename prefix will either refuse to apply the new `0004` (version already applied), silently skip it, or raise a UNIQUE constraint failure on `schema_migrations.version`.

**Consequence:** `secrets_metadata` is never created. All of Phase 10's encryption pipeline immediately fails with `no such table: secrets_metadata`. Alternatively, the migration run aborts mid-application, leaving the database in a partially migrated state.

**Design assumption violated:** The migration numbering was written without auditing the existing `migrations/` directory, which already occupies slots 0001–0005.

**Suggested mitigation:** Rename to `0006_secrets.sql` (the next available slot). Add a CI check asserting migration filenames form a gapless ascending sequence with no duplicates.

---

### R2-F03 — CRITICAL: Secret update overwrites ciphertext with no rollback path — breaks ADR-0009 snapshot fidelity

**Category:** Rollbacks

**Trigger:** When an operator updates a Route's upstream password, `upsert_secret` uses `ON CONFLICT (owner_kind, owner_id, field_path) DO UPDATE SET nonce=..., ciphertext=...`. The old ciphertext is silently overwritten. If the Caddy apply then fails, the mutation pipeline has no compensating path — the old secret value is gone. The old snapshot still exists (ADR-0009) and references the old `$secret_ref` ULID, but that ULID now decrypts to the new password, not the one that was canonical at the time of the snapshot.

**Consequence:** Rolling back to a prior snapshot now applies the new secret value (not the one that was current when the snapshot was taken). The operator has lost the old credential with no recovery path.

**Design assumption violated:** The design treats `secrets_metadata` as a one-row-per-field mutable store while `snapshots` is immutable. This breaks the snapshot-is-self-contained invariant.

**Suggested mitigation:** Change `upsert_secret` to INSERT a new row with a new `secret_id` on every secret update. The new `$secret_ref` in the snapshot points to the new row. Old snapshot `$secret_ref` values remain valid and resolve to the original ciphertext. A retention policy slice must accompany this change.

---

### R2-F04 — CRITICAL: Rollback to a snapshot whose `$secret_ref` targets a deleted ciphertext silently applies a dangling reference

**Category:** Rollbacks

**Trigger:** A Route is deleted. Its `secrets_metadata` row is deleted (per architecture §6.9 retention policy). A prior snapshot contains `{"$secret_ref": "<secret_id>"}`. The user rolls back to that snapshot. The rollback path encounters the `$secret_ref` marker and calls `get_secret`, which returns `None`. No behavior is specified for this case.

**Consequence:** Either (a) the rollback crashes with an unhandled error — the snapshot exists but cannot be applied — or (b) the pipeline substitutes an empty or omitted value, pushing a Caddy configuration with a missing credential, silently applying a security regression.

**Design assumption violated:** ADR-0009 assumes snapshots are self-contained and can always be applied. The `$secret_ref` indirection breaks this: a snapshot is now only as durable as the ciphertext rows it references.

**Suggested mitigation:** The rollback preflight (Phase 12) must validate `$secret_ref` resolution before applying any snapshot. Any unresolvable `$secret_ref` must surface as a blocking preflight failure. Add this as an explicit requirement to Phase 12's design.

---

### R2-F05 — HIGH: Secret deletion has no design, no transactional boundary, and no actor

**Category:** Orphaned Data

**Trigger:** Architecture §6.9 states "Retention: deleted when the owning configuration object is deleted." No slice in Phase 10 implements this. `secrets_metadata` has no foreign key to any owner table; there is no `ON DELETE CASCADE`. When a Route is deleted, the ciphertext row is never deleted.

**Consequence:** Orphaned ciphertext rows accumulate indefinitely. They remain revealable via `POST /api/v1/secrets/{id}/reveal` as long as the `secret_id` is known (from an old audit row, snapshot, or access log). An attacker can retrieve plaintext for credentials belonging to deleted — but potentially still-live — upstream services.

**Design assumption violated:** The retention policy enforces itself. Without a foreign key cascade or an explicit deletion call inside the owner deletion transaction, nothing enforces it.

**Suggested mitigation:** Add a `delete_secrets_for_owner(owner_kind, owner_id)` call inside the Route/Upstream deletion transaction. Add a `secrets.deleted` audit kind. Define this as a new slice (10.8) with acceptance criteria asserting the secret row is gone after owner deletion.

---

### R2-F06 — HIGH: `RevealRequest.current_password` is a plain `String` — not zeroed on drop, participates in Rust's default `Debug`

**Category:** Data Exposure

**Trigger:** `RevealRequest { pub current_password: String }` deserialized from the request body. If any middleware logs request bodies, or if the Argon2id crate panics and the panic handler includes the stack frame containing the `RevealRequest`, the password appears in plaintext in logs.

**Consequence:** The step-up password appears in daemon logs, which may be shipped to a log aggregation endpoint with weaker access controls than the keychain.

**Suggested mitigation:** Wrap `current_password` in `secrecy::Secret<String>`. `RevealRequest` must derive neither `Debug` nor `Serialize`. Audit all error types that bubble from the reveal handler for accidental password inclusion.

---

### R2-F07 — HIGH: `CipherCore.key` is a plain `Key` — not zeroed on drop, master key survives vault replacement in heap

**Category:** Data Exposure

**Trigger:** `CipherCore { key: Key, ... }` where `Key = GenericArray<u8, U32>`. `GenericArray` does not implement `Zeroize` unless the `zeroize` feature is explicitly enabled on `chacha20poly1305`. When the vault is dropped (config reload, rotation), the 32-byte master key remains in heap memory until overwritten by the next allocation.

**Consequence:** A heap dump, `/proc/<pid>/mem` read, or core dump taken at any point during daemon operation exposes the master key. This defeats the entire ADR-0014 threat model.

**Suggested mitigation:** Add the `zeroize` feature to `chacha20poly1305` in `adapters/Cargo.toml`. Wrap the key field in `zeroize::Zeroizing<[u8; 32]>`. Implement a custom `Drop` that calls `zeroize()`.

---

### R2-F08 — HIGH: Plaintext in reveal response has no transport confidentiality on loopback — any BPF listener reads it

**Category:** Data Exposure

**Trigger:** `RevealResponse { plaintext: String }` is returned as a JSON body over unencrypted loopback TCP. On Linux, any process with `CAP_NET_RAW` or a BPF program loaded by a root-equivalent process can read all loopback traffic. On macOS, a process with a BPF device handle can do the same.

**Consequence:** A co-resident monitoring agent reads the plaintext secret from the response body without authenticating to the reveal endpoint. The step-up auth and audit trail are bypassed at the network layer.

**Design assumption violated:** The TODO comment defers this: "remote-binding deployments are responsible for their own transport security." ADR-0011's threat model does not include co-resident privileged processes on the loopback interface.

**Suggested mitigation:** Document the co-resident privileged process as an explicit out-of-scope threat in ADR-0014, with a mitigation note. Alternatively, return the plaintext only over an already-authenticated WebSocket or SSE channel rather than embedding it in a plain HTTP response body.

---

### R2-F09 (composition with R1-F03 mitigation) — MEDIUM: D-Bus fallback gate introduces a TOCTOU on Linux between the row count check and vault activation

**Category:** Race Conditions

**Trigger:** R1-F03's proposed mitigation gates file-backend fallback on "zero existing keychain-encrypted rows." The count query runs at vault initialization. Between the count query and the vault becoming the live vault object, a concurrent daemon startup (systemd restart race) can insert new rows with `backend_kind = 'keychain'`. The second startup sees count=0 and falls back to file, generating a new master key.

**Consequence:** Two daemon processes are briefly alive simultaneously. Rows created during the race window are encrypted under the new file key. When the race resolves, the surviving daemon uses the keychain backend, but those rows have `backend_kind = 'file'`. Decryption fails with `KeyMissing`.

**Suggested mitigation:** Wrap the count-check and vault-initialization in a SQLite `BEGIN EXCLUSIVE` transaction, or use a PID file to enforce single-writer semantics.

---

### R2-F10 — MEDIUM: `extract_secrets` returns empty on schema miss — plaintext flows into snapshot if schema is not yet loaded

**Category:** Logic Flaws

**Trigger:** `extract_secrets(payload, schema)` consults the `SchemaRegistry` to identify secret-marked fields. If a mutation arrives before the schema for the payload's entity type is registered (lazy loading, plugin load, cold start), `extract_secrets` returns an empty `Vec` with no error. The plaintext is stored directly in `desired_state_json`.

**Consequence:** The plaintext appears in the snapshot and, because the redactor also consults the `SchemaRegistry`, in the audit log's `redacted_diff_json`. This violates constraint 12 and hazard H10.

**Suggested mitigation:** `extract_secrets` must return `Err` on an unknown schema path rather than an empty `Vec`, to prevent silent plaintext passthrough.

---

### R2-F11 — MEDIUM: `record_reveal` is an UPDATE on `secrets_metadata` — unaudited reveal possible if the UPDATE commits but the audit write crashes

**Category:** Logic Flaws

**Trigger:** Slice 10.6 step 6 calls `record_reveal` (UPDATE to `secrets_metadata.last_revealed_at/by`), then step 7 writes the `secrets.revealed` audit row. These are sequential with no wrapping transaction. A crash between steps 6 and 7 produces an updated `secrets_metadata` row with no corresponding audit row.

**Consequence:** The reveal occurred and `last_revealed_at` was updated, but no audit row exists. An operator inspecting the audit log sees no evidence of the reveal. The reveal was both real and invisible.

**Suggested mitigation:** Wrap `record_reveal` and the audit row write in a single SQLite transaction. Alternatively, remove `last_revealed_at`/`last_revealed_by` from `secrets_metadata` entirely and rely solely on the append-only audit log for access history.

---

### R2-F12 — MEDIUM: `FileBackend` accumulates all historical key versions in plaintext — every historical master key is recoverable from the file at any point

**Category:** Orphaned Data

**Trigger:** The rotation algorithm appends `version=N+1\nkey=<b64>\n` to the master-key file indefinitely. Old key versions are kept "for re-encryption," deferred to Phase 27. The file grows to contain every historical master key in plaintext.

**Consequence:** Any snapshot of the `master-key` file (e.g., from T2.12 backups that include the data directory) contains every historical master key. Every secret ever encrypted under any version is retrospectively decryptable.

**Suggested mitigation:** After re-encryption completes (Phase 27), old key versions must be removed from the file via atomic rename. Until re-encryption is implemented, document the accumulation risk explicitly and consider encrypting the file at rest using the keychain or a user passphrase.

---

### R2-F13 (composition with R1-F05 mitigation) — LOW: Step-up lockout counter must be checked and incremented atomically — concurrent requests bypass per-burst lockout

**Category:** Race Conditions

**Trigger:** The R1-F05 mitigation proposes an in-process lockout counter. In a Tokio async handler, if the check and increment are not atomic, concurrent requests all pass the check before any of them increments the counter.

**Consequence:** A burst of 10 concurrent wrong-password requests all pass the lockout check. The counter reaches 10 only after all 10 have been served. The lockout logic is bypassed for each burst.

**Suggested mitigation:** Use `AtomicU32::fetch_add` with acquire/release ordering for the counter. The check-and-increment must be atomic, or the per-user state must be serialized through a `Mutex<HashMap<user_id, AttemptState>>`.

---

### R2-F14 — LOW: Prometheus `audit_rows_total` metric leaks reveal timing via metric delta correlation

**Category:** Data Exposure

**Trigger:** Architecture §12 specifies `trilithon_audit_rows_total` on the metrics endpoint at `127.0.0.1:9898`. A co-resident monitoring agent observing this metric sees a delta spike immediately after a `POST /api/v1/secrets/{id}/reveal`, allowing correlation of reveal timing and target.

**Suggested mitigation:** Gate the metrics endpoint with the same authentication as the API, or exclude security-sensitive event counts from the public metrics surface.

---

## Summary

**Critical:** 4 · **High:** 4 · **Medium:** 4 · **Low:** 2

**Top concern:** The `upsert_secret` ON CONFLICT overwrite (R2-F03) combined with the absence of foreign-key–enforced secret deletion (R2-F05) means snapshot rollback can silently apply a dangling `$secret_ref` pointing to either the wrong secret version or a deleted row — producing either an undetected security regression or a crash at rollback time with no operator-visible explanation.
