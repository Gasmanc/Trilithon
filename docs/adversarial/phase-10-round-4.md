# Adversarial Review — Phase 10 (Secrets Vault) — Round 4

**Design summary:** Phase 10 adds an XChaCha20-Poly1305 secrets vault to Trilithon, sourcing the master key from the OS keychain or a 0600 file fallback, routing every secret-marked mutation field through the vault before snapshot, and exposing a step-up-authenticated reveal endpoint.

**Prior rounds:** 17 findings from Round 1, 14 from Round 2, 11 from Round 3 — all treated as known. This round attacks new areas and composition failures between proposed mitigations.

---

## Findings

### R4-F01 — HIGH: Concurrent mutations on a secret-bearing route cause the second mutation's `$secret_ref` to reference a rejected ULID — snapshot written with unresolvable reference

**Category:** Race Conditions

**Trigger:** The R3-F04 mitigation assigns a stable `secret_id` per `(owner_kind, owner_id, field_path)` via the unique index and performs decrypt-and-compare. Under ADR-0012 optimistic concurrency, two mutations arrive concurrently for the same route. Both read the same `secrets_metadata` row at time T, both generate a new ULID and encrypt, both call `upsert_secret`. Mutation A's INSERT succeeds. Mutation B's INSERT fails with `UNIQUE constraint violated` on `(owner_kind, owner_id, field_path)`. Mutation B has already rewritten its payload with `{ "$secret_ref": "<new_ulid_B>" }`. Mutation B proceeds to write its snapshot with that ULID.

**Consequence:** The snapshot committed by mutation B contains a `$secret_ref` that does not exist in `secrets_metadata`. At Caddy apply time, every secret field in the B snapshot returns `NotFound` or `Decryption` error. If the applier passes the literal marker to Caddy, the route is silently configured with `"$secret_ref:..."` as its password — a broken proxy with no error emitted.

**Design assumption violated:** The R3-F04 mitigation assumes the decrypt-and-compare decision is atomic with the `upsert_secret` write. Under concurrent mutations it is not.

**Fix direction:** When `upsert_secret` fails with a UNIQUE constraint violation, the mutation must not use its optimistically-generated ULID. It must re-fetch the winning row by `(owner_kind, owner_id, field_path)`, use the winner's `secret_id`, and overwrite its `$secret_ref` in the payload with that ULID before writing the snapshot. The UNIQUE constraint failure path in `upsert_secret` must return the surviving row's `id`, not just an error.

---

### R4-F02 — HIGH: Rollback preflight-to-apply TOCTOU — `$secret_ref` rows deleted between preflight validation and actual apply

**Category:** Race Conditions

**Trigger:** The R2-F04 mitigation requires rollback preflight to enumerate all `$secret_ref` values in the target snapshot and verify each resolves in `secrets_metadata`. Preflight passes at time T. Between preflight exit and `Applier::rollback` writing the desired-state pointer, a concurrent Route deletion (plus its R2-F05 secret cleanup) deletes one of the referenced rows.

**Consequence:** The desired-state pointer advances to the rolled-back snapshot. The applier resolves `$secret_ref` markers. One or more decrypt calls return `NotFound`. Either the apply fails (the rollback appeared to succeed in the audit log but Caddy's config has not changed), or the applier passes the literal marker to Caddy, silently misconfiguring the route. In either case the rollback audit row (`config.rolled-back`) was written before the error was detected.

**Fix direction:** Wrap the rollback preflight and apply in a SQLite exclusive transaction with a savepoint. Within the transaction, re-verify the `$secret_ref` rows exist immediately before the desired-state pointer write. If any ref has been deleted, abort and return `RollbackError::SecretRefGone { ids }`.

---

### R4-F03 — HIGH: Nonce-collision retry (R1-F16 mitigation) is at the wrong abstraction level relative to INSERT-new-row (R2-F03 mitigation)

**Category:** Logic Flaws

**Trigger:** R1-F16 proposes a UNIQUE constraint on `(nonce)` in `secrets_metadata` with a retry loop. The retry is described as happening inside `vault.encrypt`. But the collision is detected at `upsert_secret` call time. The `Ciphertext` returned by `vault.encrypt` already encodes the colliding nonce — the nonce is the AEAD nonce used for encryption; swapping it without re-encrypting is invalid. To retry, the caller must call `vault.encrypt` again, not just retry `upsert_secret` with the same `Ciphertext`.

**Consequence:** If the retry loop is placed inside `upsert_secret` (the natural location), it retries with the same `Ciphertext`, collides again, and loops until the retry limit is exhausted. The mutation then fails and the route configuration is rejected due to a 1-in-2^192 probabilistic event that becomes a systematic failure if the retry is wired incorrectly.

**Fix direction:** Remove the UNIQUE constraint on `(nonce)` as a collision-detection mechanism. The 192-bit nonce space makes probabilistic uniqueness stronger than any realistic constraint can enforce. If a nonce constraint is retained for defense-in-depth, place the retry loop at the call site in `route_mutation_secrets_through_vault` that owns both `vault.encrypt` and `upsert_secret`, so re-encryption is performed on each attempt.

---

### R4-F04 — HIGH: `EncryptContext.key_version` in the reveal handler has no explicit source contract — a hardcoded or current-version value breaks all pre-rotation secrets permanently

**Category:** Logic Flaws

**Trigger:** Slice 10.6 step 4 says "Construct the `EncryptContext` from the row." The `EncryptContext` includes `key_version: u32`. The first three fields (`owner_kind`, `owner_id`, `field_path`) come naturally from the `SecretRow` top-level columns. The `key_version` must come from `row.ciphertext.key_version` — but this is not stated. If an implementer writes `key_version: self.vault.current_key_version()` (the current version, not the stored ciphertext's version), the AAD at decrypt time diverges from the AAD at encrypt time for any ciphertext encrypted before the latest rotation.

**Consequence:** After any master-key rotation, all pre-rotation secrets become permanently unrevealable with `CryptoError::Decryption` and no actionable error message. The operator cannot distinguish "wrong key" from "wrong AAD" from "ciphertext tampered." Rotation silently breaks all existing secrets — a data-loss scenario invisible until the first post-rotation reveal is attempted.

**Fix direction:** Remove `key_version` from `EncryptContext`. AAD should bind only the row's stable identity (`owner_kind`, `owner_id`, `field_path`). If `key_version` must appear in AAD, add an explicit contract note to the slice: "populate `key_version` from `row.ciphertext.key_version`, never from any other source." Add an acceptance test: encrypt under key v1, simulate rotation to v2, reveal — must succeed.

---

### R4-F05 — MEDIUM: `RevealResponse { plaintext: String }` enters the Axum response pipeline — `tower_http::trace::TraceLayer` body logging emits the plaintext to the structured log

**Category:** Data Exposure

**Trigger:** The reveal handler returns `Json(RevealResponse { plaintext })`. If `tower_http::trace::TraceLayer` is configured with `on_response` body logging (a common copy-paste from the tower-http examples), the response body — including the plaintext secret — is emitted as a `tracing::trace!` event. An operator enabling verbose tracing for debugging causes all subsequent reveals to be logged until the level is lowered.

**Consequence:** The plaintext secret appears in the daemon's structured log, written to the same data directory covered by the backup threat model. The guarantee that "plaintext MUST NOT appear in any audit column" is enforced at the application layer but bypassed by the observability layer.

**Fix direction:** Register the reveal route on a sub-router that does not attach `TraceLayer`. Introduce a `SensitiveBody` newtype that implements `IntoResponse` by writing serialised bytes directly, bypassing any body-intercepting middleware. Document in the reveal handler's module that body-level tracing MUST NOT be enabled for this route.

---

### R4-F06 — MEDIUM: Session revocation between middleware auth check and reveal handler execution allows a revoked session to complete a reveal

**Category:** Authentication & Authorization

**Trigger:** The auth middleware validates the session once per request and passes `AuthContext::Session` to the handler. Between middleware validation and the reveal handler's step-up check, a concurrent `DELETE /api/v1/sessions/current` (or admin-triggered revocation) deletes the session row. The handler still sees `AuthContext::Session` and proceeds; if the step-up password is correct, the reveal completes. The audit row records the actor's user ID but does not note that the session was revoked at the time of the reveal.

**Consequence:** A revoked session completes the highest-privilege operation in the system. An operator responding to a detected breach by revoking sessions cannot rely on the revocation to prevent in-flight reveals.

**Fix direction:** The reveal handler must re-validate session freshness as its first step: `SELECT revoked_at FROM sessions WHERE id = ?`. If `revoked_at IS NOT NULL`, return 403. This costs one SQLite read and closes the TOCTOU window.

---

### R4-F07 — MEDIUM: Architecture §6.9 DDL omits `key_version`, `algorithm`, and `backend_kind` columns — schema used for disaster recovery is inconsistent with the migration

**Category:** State Manipulation

**Trigger:** Architecture §6.9 defines the canonical DDL for `secrets_metadata` without `algorithm`, `key_version`, `backend_kind`, or the `secrets_metadata_key_version` index. The migration in slice 10.5 adds all of these. An operator who reconstructs the schema from the architecture document (a realistic disaster-recovery practice) creates a table missing `key_version`. Every rotation scan (`SELECT * FROM secrets_metadata WHERE key_version = ?`), every `upsert_secret`, and every `CipherCore::decrypt` key-version check fail at runtime with "no such column."

**Consequence:** A disaster-recovery rebuild from the architecture DDL produces an inoperable vault. The daemon starts and serves traffic, but any mutation touching a secret field or any rotation attempt immediately errors with no startup-time indication.

**Fix direction:** Update architecture §6.9 to include `algorithm`, `key_version`, `backend_kind`, and the `key_version` index. Add a CI check that extracts the live schema from the migration runner and diffs it against the architecture DDL — any divergence fails the build.

---

### R4-F08 — LOW: `Secret<String>` heap residue on async cancellation — NON-FINDING (informational)

**Category:** Data Exposure

**Trigger:** Probe asked whether `secrecy::Secret<String>` heap memory is zeroed when the future holding it is cancelled by Tokio.

**Finding:** The `zeroize::Zeroize` implementation for `String` zeroes the bytes in place before deallocating. Rust's async runtime drops the future's `Pin<Box<...>>` on client disconnect, which calls `Secret::drop`, which calls `Zeroize::zeroize` on the inner bytes before the allocator reclaims the block. The bytes ARE zeroed. This is not a finding — the `zeroize` crate correctly handles the async cancellation path. This note is informational: when R2-F06 is mitigated with `secrecy::Secret<String>`, the async cancellation path is safe.

---

### R4-F09 — LOW: `KeychainBackend::rotate` has no u32 overflow check — version aliasing at u32::MAX overwrites the v0 key

**Category:** Logic Flaws

**Trigger:** `rotate` parses the current version from the account name string `master-key-v{N}`, increments, and stores under `master-key-v{N+1}`. If `N = u32::MAX`, `N + 1` wraps to 0 in integer arithmetic, aliasing to `master-key-v0` — the original account. The rotate call overwrites the v0 key with the new key. All secrets encrypted under the original v0 key become permanently unrecoverable.

**Consequence:** Silent data loss of all v0-encrypted secrets after 2^32 rotations. Negligible in practice; rated LOW.

**Fix direction:** Use `u32::checked_add(1)` with `CryptoError::KeyringUnavailable { detail: "key version overflow" }` on failure.

---

## Summary

**Critical:** 0 · **High:** 4 · **Medium:** 3 · **Low:** 2 (one non-finding)

**Top concern:** R4-F04 (`EncryptContext.key_version` populated from the wrong source) is silent, implementation-time, and permanently destroys the ability to reveal any secret encrypted before the first rotation — a data-loss scenario that passes all unit tests unless a post-rotation reveal test is explicitly included.
