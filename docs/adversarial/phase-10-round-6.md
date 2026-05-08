# Adversarial Review — Phase 10 (Secrets Vault) — Round 6

**Design summary:** Phase 10 adds an XChaCha20-Poly1305 secrets vault, routing secret-marked mutation fields through encrypt/`upsert_secret`/`substitute_secret_refs` before snapshot, with a step-up-authenticated reveal endpoint and OS keychain or file-fallback master key storage.

**Prior rounds:** 64 findings across rounds 1–5 (including 3 non-findings). All treated as known. This round probes composition failures, fix correctness, and remaining surfaces.

---

## Findings

### R6-F01 — HIGH: Horizontal privilege escalation on reveal — any authenticated operator can decrypt any secret regardless of route ownership

**Category:** Authentication & Authorization

**Trigger:** Slice 10.6 step 3 loads the secret row by `id` using only `get_secret(conn, &id)`. The `owner_kind` and `owner_id` fields are used only to construct `EncryptContext` for decryption (step 4) — never to verify the requester owns the resource. Any session that passes step-up can call `POST /api/v1/secrets/{secret_id}/reveal` with any ULID.

**Consequence:** In a multi-operator deployment, operator A can reveal any secret owned by operator B's routes by knowing or observing the ULID from B's `$secret_ref` tokens in shared audit-log views. The audit row is written with A's `user_id` but carries no ownership-mismatch signal. The access looks legitimate to any audit consumer.

**Design assumption violated:** The design assumes only the owning operator needs the plaintext of their own secrets. The access control is authentication (valid session + step-up) rather than authorization (session owns the resource).

**Fix direction:** After step 3, load the owning resource identified by `(row.owner_kind, row.owner_id)` and assert the session's user has read permission on it via the existing role middleware. If not, return 403 without writing a reveal audit row; write `auth.access-denied` instead. Add `reveal_wrong_owner_403.rs` test.

---

### R6-F02 — HIGH: `secrecy::Secret<String>` does not implement `Clone` — R5-F02 fix breaks `ExtractedSecret`'s derive and any pipeline site that clones it

**Category:** Logic Flaws

**Trigger:** The TODO shows `#[derive(Clone, Debug)]` on `ExtractedSecret`. R5-F02 directs changing `plaintext: String` to `plaintext: secrecy::Secret<String>`. The `secrecy` crate intentionally omits `Clone` for `Secret<T>` to prevent accidental duplication. If `ExtractedSecret` retains `#[derive(Clone)]` after R5-F02 is applied, the build fails: `Secret<String>` does not satisfy `Clone`.

**Consequence:** Either the build breaks at `ExtractedSecret`'s derive, or the implementer removes `Clone` and every call site that clones a `Vec<ExtractedSecret>` (test code, async pipeline hand-offs) also fails to compile. R5-F02's fix specification does not mention this dependency.

**Fix direction:** R5-F02's specification must be extended to: (1) remove `Clone` from `ExtractedSecret`'s derive, (2) audit all pipeline sites that depend on `ExtractedSecret: Clone`, and (3) replace clones with explicit construction. Alternatively, wrap only inner bytes in `secrecy::Secret<Vec<u8>>` and document the non-Clone contract explicitly.

---

### R6-F03 — HIGH: ADR-0012 and R5-F01 compose incorrectly — a mutation with a valid sequential `expected_version` receives a secret-field 409 it cannot rebase

**Category:** Race Conditions

**Trigger:** Mutation A (`expected_version = 5`) updates a route's `upstream_password` and succeeds; `config_version` advances to 6. Mutation B (`expected_version = 6`) — correctly reflecting A's landing — also updates `upstream_password` to a different value. B passes the ADR-0012 version check. B's `upsert_secret` then hits the UNIQUE constraint and R5-F01's mitigation returns `StorageError::SecretFieldConflict` → 409.

**Consequence:** B receives a 409 even though its `expected_version` was unambiguously valid. ADR-0012's 409 carries `expected_version` / `current_version` fields that guide rebase. A secret-field conflict has no meaningful version delta — the `expected_version` was correct. A client that handles ADR-0012's conflict by re-fetching the current config and resubmitting will loop indefinitely if B's intent is to set a different plaintext than A stored.

**Design assumption violated:** R5-F01 specified the error as "409 aligned with the ADR-0012 concurrency model." The two 409 paths require different client handling and must be distinguishable.

**Fix direction:** Give the secret-field conflict a distinct error code: `{ "code": "secret_field_conflict", "field_path": "...", "owner_id": "..." }` rather than the ADR-0012 `ConflictError` type. Document in the API spec that `409 Conflict` from mutation endpoints carries two distinct codes (`version_conflict` and `secret_field_conflict`) requiring different resolution strategies.

---

### R6-F04 — MEDIUM (show-stopper): The applier sends `desired_state_json` containing `$secret_ref` markers to Caddy — no `resolve_secret_refs` step exists, so secrets are never applied to the running proxy

**Category:** Logic Flaws

**Trigger:** The mutation pipeline rewrites the payload via `substitute_secret_refs` so the snapshot's `desired_state_json` contains `{"$secret_ref": "<id>"}`. The applier sends this snapshot to Caddy via `POST /load`. Caddy receives a `$secret_ref` JSON object where it expects a plaintext string. No slice in Phase 10 specifies a `resolve_secret_refs` step between loading the snapshot and calling `caddy_client.load_config`.

**Consequence:**
- **Case A (Caddy rejects):** Every mutation touching a secret-bearing route fails at the apply step with a Caddy 400 validation error. All secret-bearing routes are permanently unapplyable after Phase 10.
- **Case B (Caddy accepts literal value):** Caddy stores `"$secret_ref:..."` as the password literal. The route is silently broken — wrong credential, no error. The drift loop compares `desired_state_json` (`$secret_ref`) against Caddy's running config (`$secret_ref`) and reports no drift, hiding the breakage.

Either case makes Phase 10 a complete functional regression on secret-bearing routes.

**Design assumption violated:** The design implicitly assumes the snapshot's `desired_state_json` is what gets sent to Caddy. It must not be — Caddy needs the resolved form. The design has no specification for where or how this resolution happens.

**Fix direction:** Add `resolve_secret_refs(desired_state_json, vault, storage) -> Result<CaddyConfig, SecretsError>` as a required step in the applier, between snapshot load and `caddy_client.load_config`. The resolved form replaces every `{"$secret_ref": "<id>"}` with the decrypted plaintext. The resolved form is never written to SQLite; it lives only on the applier's stack for the duration of the Caddy HTTP call. Add an acceptance test asserting Caddy's running config contains the plaintext value after a secret-bearing mutation.

---

### R6-F05 — MEDIUM: `backend_kind` is per-row but key material is per-version — no enforced invariant prevents mixed-backend rows for the same `key_version` after a partial rotation

**Category:** State Manipulation

**Trigger:** During a rotation that switches backends (e.g., keychain → file after R1-F03 gate triggers), the re-encryption loop updates `key_version` and `backend_kind` per-row. A SIGKILL between row 50 and row 51 of 100 leaves 50 rows with `(backend_kind='file', key_version=2)` and 50 rows with `(backend_kind='keychain', key_version=1)`. On restart, the vault loads the backend indicated by each row — but row-level `backend_kind` does not correspond to where the key for `key_version=1` actually lives after the rotation was partially applied. Decryption for affected rows fails with `CryptoError::KeyMissing` or authentication-tag mismatch.

**Consequence:** A subset of secrets becomes permanently unreadable after any rotation that is interrupted. The only indicator is a stream of decryption errors; there is no schema-level signal that a partial rotation is in progress.

**Fix direction:** Introduce a `secret_key_versions` table with `(version INTEGER PRIMARY KEY, backend_kind TEXT NOT NULL, created_at INTEGER NOT NULL)`. `secrets_metadata` references it via `FOREIGN KEY (key_version) REFERENCES secret_key_versions(version)`. Drop `backend_kind` from `secrets_metadata`. The rotation loop inserts a new `secret_key_versions` row atomically after all re-encryptions succeed, enforcing the invariant at the schema level.

---

### R6-F06 — LOW: R2-F05 mitigation (explicit secret deletion) must choose between hard-delete and soft-delete — neither option is specified, and soft-delete leaves deleted secrets revealable

**Category:** Orphaned Data

**Trigger:** R2-F05 directs explicit deletion + `secrets.deleted` audit kind. The design does not specify whether deletion is hard (permanent `DELETE`) or soft (`deleted_at NOT NULL`). If hard-delete: the ciphertext is gone; forensic "what was this value" questions are unanswerable post-deletion. If soft-delete: the row remains in `secrets_metadata`, and the reveal handler's `get_secret` (step 3) does not filter on `deleted_at IS NULL` — logically deleted secrets remain revealable indefinitely.

**Fix direction:** Specify one strategy. If soft-delete: add `deleted_at INTEGER` to `secrets_metadata`, change `get_secret` to `WHERE id = ? AND deleted_at IS NULL`, and add `reveal_deleted_secret_404.rs`. If hard-delete: document that the plaintext value of a deleted secret is not recoverable and require explicit operator acknowledgment before route deletion.

---

## Non-findings

**Probe 9 — `rotate_master_key` async and object safety:** The project already mandates `#[async_trait]` on all async traits (trait-signatures.md). The `async_trait` crate boxes futures as `Box<dyn Future>`, which is object-safe. Making `rotate_master_key` async follows this existing pattern; `dyn SecretsVault` continues to compile. No finding.

**Probe 10(b) — SQLite WAL mode and savepoints:** Savepoints function identically in WAL and rollback-journal mode. No interaction. No finding.

**Probe 10(c) — Multi-version HashMap cold at startup:** Substantially covered by R1-F02 and R4-F04. No new concrete scenario. No finding.

**Probe 10(a) — `OwnerKind::Other` unhandled:** No V1 code path generates `OwnerKind::Other`. Without a concrete caller, this cannot be constructed as a concrete failure. Low-confidence hunch — not raised.

---

## Summary

**Critical:** 0 · **High:** 3 · **Medium:** 2 (one is a functional show-stopper) · **Low:** 1 · **Non-findings:** 4

**Top concern:** R6-F04 — the applier sends `$secret_ref` markers to Caddy with no resolution step, meaning secrets are never applied to the running proxy; every secret-bearing route is either rejected by Caddy or silently broken after Phase 10.

**Design-space signal:** Most categories have either concrete findings or confirmed non-findings across six rounds. The new HIGH findings this round are a horizontal privilege escalation (R6-F01), a compile-time composition failure from R5-F02 (R6-F02), and a 409-code ambiguity from R5-F01 (R6-F03). The MEDIUM show-stopper (R6-F04 — no `resolve_secret_refs` in applier) is a fundamental functional gap. Once these four are addressed in the design, the remaining open space is narrow. Recommend `--final` after the next design revision incorporates R6-F01 through R6-F04.
