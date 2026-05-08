# Adversarial Review — Phase 10 (Secrets Vault) — Round 5

**Design summary:** Phase 10 adds an XChaCha20-Poly1305 secrets vault, routing secret-marked mutation fields through encrypt/`upsert_secret`/`substitute_secret_refs` before snapshot, with a step-up-authenticated reveal endpoint and OS keychain or file-fallback master key storage.

**Prior rounds:** 53 findings across rounds 1–4 (including 1 non-finding). All treated as known. This round probes composition failures, fix correctness, and four new areas not previously attacked.

---

## Findings

### R5-F01 — HIGH: R4-F01 re-fetch fix introduces silent last-writer-wins on conflicting concurrent secret updates — operator's intended value is silently discarded

**Category:** State Manipulation

**Trigger:** Mutations A and B arrive concurrently for the same route, carrying different values for the same secret field (two operators updating the upstream password simultaneously). A's INSERT succeeds. B's INSERT fails with UNIQUE constraint on `(owner_kind, owner_id, field_path)`. Per the R4-F01 mitigation, B re-fetches the winning row to obtain A's `secret_id`, rewrites its `$secret_ref` in the payload to use A's `secret_id`, and writes its snapshot.

**Consequence:** B's snapshot is committed to the audit log and the desired-state pointer advances. Caddy is applied with B's non-secret configuration. Every future reveal of the secret decrypts to A's plaintext — not B's intended value. B's audit row records `mutation.applied` for B's actor. There is no indication in the audit log, the snapshot, or the applied state that B's intended secret value was silently replaced by A's. The operator who submitted B receives a success response but their credential change was not applied. This is invisible corruption.

**Design assumption violated:** The R4-F01 fix treats "which INSERT won the UNIQUE constraint race" as equivalent to "which plaintext should be canonical." These are different questions. ADR-0012 uses `expected_version` to surface concurrency conflicts. The secret update path has no equivalent — two mutations can both pass the version check and both attempt different secret values for the same field, with one silently dropped.

**Fix direction:** When B's INSERT fails with UNIQUE constraint, return `StorageError::SecretFieldConflict { field_path, winning_mutation_id }` from `upsert_secret` rather than silently adopting A's row. Surface this as a `409 Conflict` response aligned with the ADR-0012 concurrency model. B's submitter must re-read and resubmit. This preserves the invariant that every committed snapshot's secret content reflects what the committing actor intended.

---

### R5-F02 — HIGH: Savepoint rollback (R3-F01 fix) leaves plaintext in heap if `ExtractedSecret.plaintext` is plain `String` — must compose with R2-F06 zeroizing or plaintext persists until allocator reuse

**Category:** Data Exposure

**Trigger:** R3-F01 wraps `route_mutation_secrets_through_vault` in a SQLite savepoint: on failure, the savepoint rolls back `upsert_secret` calls. R2-F06 wraps `RevealRequest.current_password` in `secrecy::Secret<String>`. These fixes are specified for different types. If `ExtractedSecret.plaintext` remains a plain `String`, dropping `Vec<ExtractedSecret>` after savepoint rollback calls `String::drop`, which frees the heap allocation but does NOT zero the bytes.

**Consequence:** If a mutation carrying N secret leaves fails at leaf K, the savepoint discards K-1 SQLite rows but the plaintext values for all processed leaves remain in freed heap memory until overwritten. These are secrets the operator never "revealed" — no `secrets.revealed` audit row was written. A crash dump, `process_vm_readv` call, or heap scanner can recover these bytes silently. Rolling back SQL does not un-materialize the heap.

**Design assumption violated:** R3-F01 assumes rolling back the SQL writes is sufficient to protect the plaintext. The plaintext was materialized in Rust's heap before the SQL writes. SQL rollback does not zero heap.

**Fix direction:** Change `ExtractedSecret.plaintext` from `String` to `secrecy::Secret<String>`. On savepoint rollback, when `Vec<ExtractedSecret>` drops, each `Secret<String>` is zeroed before the heap is freed. Both R3-F01 and R2-F06 must be composed on the same struct — the specification must make this explicit.

---

### R5-F03 — HIGH: Reveal plaintext is materialized before audit transaction commits — disk-full or busy-timeout produces an unaudited reveal with plaintext already sent to client

**Category:** Authentication & Authorization

**Trigger:** The R2-F11 mitigation wraps `record_reveal` and the audit write in a single SQLite transaction. The reveal handler sequence ends: …(5) write `secrets.revealed` audit row, (6) update `last_revealed_at`, (7) commit transaction, (8) return `Json(RevealResponse { plaintext })`. On the happy path this is safe — commit precedes TCP transmission. The gap is: if the handler materializes `RevealResponse { plaintext }` before calling `commit()` (e.g., because Axum's `Json(...)` wrapper is constructed inline and held in a local variable while `commit()` is called), the plaintext bytes live in a heap-allocated struct. If `commit()` then fails (disk full, `SQLITE_BUSY` exceeding `busy_timeout`), the handler must return an error — but the plaintext bytes are already in a local variable that will be dropped. The question is whether they were already serialized into Axum's send buffer.

**Consequence:** If Axum begins serializing the response before `commit()` is called (a realistic pattern when the struct is constructed before the commit call), the plaintext may already be in the TCP send buffer when the commit fails. The client receives the plaintext. The audit row was never committed. An operator performing forensic review of the audit log finds no `secrets.revealed` entry. This violates ADR-0014's guarantee that "every plaintext access produces a row."

**Design assumption violated:** The mitigation assumes commit-before-response ordering. This ordering is easy to state and easy to get wrong in async Rust where the handler's return value is typically constructed before Axum serializes and transmits it.

**Fix direction:** The acceptance criteria for slice 10.6 must explicitly state: "the plaintext MUST NOT be placed in any response struct until after the audit transaction has committed." In code, this means: call `commit()` first; if it errors, return an error response (no `RevealResponse` constructed); if it succeeds, then construct `RevealResponse { plaintext }` and return it. Add an acceptance test that simulates a `commit()` failure (inject a SQLite error mock) and asserts that the handler returns a non-200 response with no plaintext body.

---

### R5-F04 — MEDIUM: `SecretsVault::redact` and `DiffEngine::redact_diff` are two independent redaction implementations — audit rows written via different paths carry incompatible redaction tokens for the same secret

**Category:** Data Exposure

**Trigger:** `SecretsVault::redact(value: &serde_json::Value, ...)` and `DiffEngine::redact_diff(diff: &Diff, ...)` are separate trait methods with separate implementations producing potentially different token formats. ADR-0014 specifies the token as `SHA-256(plaintext)` prefixed `secret:`. The design context specifies `***<first-12-of-sha256(secret_id)>`. These are already inconsistent (raised in R1-F06); but having two independent implementations means a caller using the wrong method for a given context produces tokens in yet a third format.

**Consequence:** Audit rows written through the diff-engine path and audit rows written through the vault-redact path (e.g., the `system.restore-applied` handler, which has a raw `Value` rather than a typed `Diff`) carry incompatible tokens for the same underlying secret. An operator searching for all audit rows referencing a particular secret by its redaction token will miss rows produced by the other path. The guarantee "identical secrets MUST produce identical hash prefixes" is violated across audit paths.

**Design assumption violated:** The design assumes one redactor with one output format. Two trait methods with two independent implementations defeat this.

**Fix direction:** Remove `SecretsVault::redact` from the audit-log write path. The single canonical redaction path is `DiffEngine::redact_diff`. Any code path with a raw `serde_json::Value` that needs a redacted representation must go through a shared `RedactorCore` free function in `core` that both trait implementations call. Document explicitly: `SecretsVault::redact` MUST NOT be called on any path that writes to `audit_log.redacted_diff_json`.

---

### R5-F05 — MEDIUM: `AuthenticatedSession` from Phase 9 likely carries only `user_id` — R4-F06 re-validation query has no session ID key without a cross-phase type change

**Category:** Authentication & Authorization

**Trigger:** R4-F06 requires the reveal handler to re-query `SELECT revoked_at FROM sessions WHERE id = ?` using the session ID. `AuthContext::Session` is produced by Phase 9's auth middleware. The middleware validates the session token from the cookie, looks up the `sessions` row, and produces `AuthContext`. If `AuthContext::Session` is `{ user_id: UserId, role: Role }` — the common pattern — the session ID is consumed by the middleware and is not propagated into the handler.

**Consequence:** The reveal handler receives `AuthContext::Session { user_id, role }` but has no `session_id`. It cannot execute the re-validation query. The R4-F06 mitigation is structurally unimplementable without modifying Phase 9's `AuthContext` type to carry `session_id`. This is a cross-phase type change that Phase 10's design must specify as a dependency.

**Design assumption violated:** R4-F06 assumes the session ID is available in the handler context. Phase 9's `AuthContext` was designed before Phase 10's requirements were known.

**Fix direction:** Add `session_id: SessionId` to `AuthContext::Session`. Document as a cross-phase dependency in slice 10.6: "Phase 10 slice 10.6 requires `AuthContext::Session` to carry `session_id`. If Phase 9 shipped without this field, a patch to Phase 9 must add it before Phase 10 can implement R4-F06."

---

### R5-F06 — MEDIUM: FileBackend atomic-rename (R1-F04 fix) silently degrades to non-atomic copy-then-delete when temp file is written outside `data_dir`

**Category:** Rollbacks

**Trigger:** R1-F04 specifies "write to a temp file on the same filesystem, `fsync`, then `rename()`." If the implementation writes to `std::env::temp_dir()` (commonly `/tmp`, a `tmpfs` mount separate from the main filesystem), `rename()` fails with `EXDEV`. Common Rust implementations recover from `EXDEV` by falling back to `std::fs::copy` + `std::fs::remove_file` — which is non-atomic. Additionally, if `data_dir` is an NFS v3 mount, even within-directory `rename()` may not be atomic at the server.

**Consequence:** On affected configurations (tmpfs `/tmp`, NFS `data_dir`), the FileBackend falls back to copy-then-delete. A SIGKILL between copy and delete leaves both the `.tmp` file and the original `master-key` file present. The parser's behavior with two coexisting key files is unspecified. If it opens the wrong one, secrets encrypted after the rotation become permanently unreadable.

**Fix direction:** Require the temp file to be written to `data_dir` itself (not `temp_dir()`). Document that NFS `data_dir` is unsupported for FileBackend. Add an `EXDEV` detection test with a cross-device temp path and assert the error is surfaced rather than silently falling back.

---

### R5-F07 — MEDIUM: `KeychainBackend::generate_and_store` does not verify stored bytes — silent OS encoding of the base64 string produces a different key on next retrieval

**Category:** Single Points of Failure

**Trigger:** `generate_and_store` generates 32 raw bytes, base64-encodes them, calls `entry.set_password(&encoded)`, and returns the raw bytes without a read-back verification. The macOS Security framework stores keychain items as UTF-8 strings under the `kSecValueData` attribute. If the `keyring` crate stores via an attribute that allows OS-level Unicode normalization, a base64 string containing `+`, `/`, or certain byte patterns could be transformed. On the next daemon start, `get_password` returns a different string than was stored. The generated key is gone; the stored key is different.

**Consequence:** `vault.decrypt` fails with `CryptoError::Decryption` for every ciphertext. The operator sees authentication tag mismatch with no indication the stored key was corrupted at write time. Recovery requires the original key bytes, which existed only in process memory during `generate_and_store`.

**Design assumption violated:** The design assumes the keychain is a binary-safe byte store. Platform keychain implementations are string stores with encoding semantics and are not guaranteed to be binary-safe for arbitrary base64 strings.

**Fix direction:** After `set_password`, immediately call `get_password` and compare returned bytes to the generated key. If they differ, return `CryptoError::KeyringUnavailable { detail: "keychain encoding mismatch" }` and fall through to `FileBackend`. Add a platform integration test asserting round-trip fidelity.

---

### R5-F08 — NON-FINDING: `Ciphertext` serde round-trip — `Vec<u8>` as JSON integer array is lossless in serde_json

**Category:** Logic Flaws

**Finding:** `serde_json` serializes `Vec<u8>` as a JSON array of unsigned integers 0–255, using the `Number` representation without float coercion. Values 0–255 fit exactly in `u64` (serde_json's internal representation) and deserialize back to `u8` without loss. The round-trip is mathematically lossless. The storage overhead (3–4× versus raw binary or base64) is real but not a correctness issue. No finding.

---

### R5-F09 — NON-FINDING: `Zeroizing<[u8; 32]>` is `Send + Sync` — no trait bound failure

**Category:** Logic Flaws

**Finding:** `[u8; 32]: Send + Sync`. `Zeroizing<T>` is `Send` iff `T: Send` and `Sync` iff `T: Sync`. Therefore `Zeroizing<[u8; 32]>: Send + Sync`. The `SecretsVault: Send + Sync + 'static` bound is satisfied after R2-F07's mitigation. No finding.

---

### R5-F10 — LOW: Pre-initialization vault access from early inbound HTTP request has unspecified behavior — potential panic, misleading error, or hang

**Category:** Single Points of Failure

**Trigger:** If the HTTP listener is bound before vault initialization completes (overlapped startup for faster bind time), an early inbound mutation request calls `vault.encrypt` on an `Option<CipherCore>` or `OnceCell<CipherCore>` that is not yet populated. The behavior is unspecified: `unwrap` on `None` panics; returning `CryptoError::KeyMissing { version: 0 }` misleads the caller; blocking on `OnceCell::get_or_init` risks thread-pool starvation under load.

**Fix direction:** Enforce a strict startup barrier: vault initialization and storage migration must complete before `HttpServer::bind` is called. Document this dependency order in `cli/main.rs`.

---

## Summary

**Critical:** 0 · **High:** 3 · **Medium:** 4 · **Low:** 1 · **Non-findings:** 2

**Top concern:** R5-F03 (reveal plaintext materialized before audit commit) can produce a plaintext disclosure with no audit trail under a realistic failure condition (disk full, SQLite busy timeout), violating ADR-0014's guarantee that "every plaintext access produces a row" — and the failure is easy to introduce accidentally in async Rust.
