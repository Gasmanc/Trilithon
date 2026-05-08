# Adversarial Review — Phase 10 (Secrets Vault) — Round 10

**Design summary:** Phase 10 adds XChaCha20-Poly1305 secrets vault to Trilithon. The mutation pipeline routes secret fields through `extract_secrets` → `vault.encrypt` → `upsert_secret` → `substitute_secret_refs` → snapshot. `resolve_secret_refs` decrypts and POSTs resolved plaintext to Caddy with `persist: false` injected via deep-merge. Caddy admin API uses BasicAuth (R9-F02). Drift detection operates on masked non-secret fields only (R9-F01). `upsert_secret` uses an inner savepoint for UPDATE+INSERT atomicity (R9-F03). `secret_key_versions` table introduced (R6-F05).

**Prior rounds:** 87 findings across rounds 1–9, 20 confirmed non-findings. All treated as addressed and not re-raised. This round probes six specific axes identified in the round 10 brief.

---

## Findings

### R10-F01 — HIGH: Caddy admin API BasicAuth credential has no specified storage location — either plaintext at rest or a first-run circular dependency

**Category:** Authentication & Authorization

**Trigger:** R9-F02 requires that Caddy's admin API be protected with BasicAuth. Trilithon must possess the credential to make its own admin API calls (`POST /load`, the bootstrap config write that sets `persist: false`). No design document specifies where this credential is stored. Two structurally conflicting options exist:

- **Option A — Stored in the vault:** Trilithon can only retrieve the credential after the vault is initialised and decrypted. But the very first Caddy API call Trilithon must make is the bootstrap config POST (which sets `persist: false`). Caddy cannot yet have BasicAuth configured (it's a fresh install), so Trilithon must call the admin API *without* auth to configure it. Once BasicAuth is configured, Trilithon needs the credential for all subsequent calls — but if the credential is stored in the vault, the vault must already be initialised. If vault init itself requires a Caddy admin API call, there is a genuine circular dependency: vault init → Caddy API call (requires credential) → credential from vault (requires vault).

- **Option B — Stored outside the vault** (plaintext config file, environment variable): the encryption-at-rest guarantee for the Caddy admin credential is violated. The credential is plaintext at rest, and any process that reads Trilithon's config file can access the Caddy admin API, call `GET /config/`, and extract all plaintext secrets — negating the primary benefit of R9-F02.

The design summary states "Trilithon uses the credential in its own admin API calls" but is entirely silent on where the credential is stored or how the first-run bootstrap sequence resolves the ordering dependency.

**Consequence:** Either the vault's encryption-at-rest guarantee does not cover the Caddy admin API credential (Option B — plaintext at rest), or there is no valid first-run startup sequence (Option A — circular dependency). Option B means the credential unlocking `GET /config/` is itself unprotected, negating R9-F02's primary benefit. Option A means fresh installs cannot complete the bootstrap sequence without an unauthenticated first call to Caddy's admin API.

**Design assumption violated:** The design assumes the Caddy admin API credential can be cleanly managed under the vault. It cannot: the credential must exist before any vault content (first run) and enabling vault-based storage creates a dependency cycle.

**Suggested mitigation:** Specify a three-stage first-run bootstrap: (1) Trilithon generates a random Caddy admin credential at first run and writes it to a `0600` file (analogous to the master-key file fallback). The credential is stored outside the vault — a documented exception. (2) Trilithon sends the bootstrap Caddy config (including `persist: false` and the `admin.identity` BasicAuth block with the hashed credential) using this credential. All subsequent Caddy API calls use the same `0600`-stored credential. (3) Document in ADR-0014: the Caddy admin API credential is an accepted plaintext-at-rest item, analogous to the master key itself. The `0600` file is protected only by filesystem permissions and is not covered by vault encryption.

---

### R10-F02 — MEDIUM: `secret_key_versions` INSERT and re-encryption UPDATEs are not specified to share a transaction — crash between last UPDATE and INSERT leaves FK-invalid rows permanently unreadable

**Category:** Rollbacks

**Trigger:** R6-F05's fix direction states: "The rotation loop inserts a new `secret_key_versions` row atomically after all re-encryptions succeed." The phrase "atomically after" is ambiguous. The design does not specify that the `secret_key_versions` INSERT and the batch of re-encryption UPDATEs share a single SQLite transaction. With `PRAGMA foreign_keys = ON`, the FOREIGN KEY constraint on `secrets_metadata.key_version` is enforced.

If the rotation loop executes re-encryption UPDATEs first, then inserts the `secret_key_versions` row:
1. Re-encryption UPDATEs commit: all affected `secrets_metadata` rows now reference `key_version = N+1`.
2. Daemon crashes before the `secret_key_versions` INSERT for version N+1 commits.
3. On restart: `secrets_metadata` rows reference `key_version = N+1`; `secret_key_versions` has no row for N+1. With FK enforcement, all decrypt calls for those rows fail (`CryptoError::KeyMissing { version: N+1 }`). All affected secrets become permanently unreadable with no documented recovery path.

**Consequence:** A crash during rotation leaves re-encrypted rows pointing to a non-existent key version. The vault is broken for all affected secrets; manual SQL intervention is required for recovery.

**Design assumption violated:** R6-F05 implies the `secret_key_versions` INSERT is the atomic completion event of rotation. The design does not state that both the UPDATEs and the INSERT must be in a single transaction.

**Suggested mitigation:** Specify explicitly: "The rotation loop MUST execute all re-encryption UPDATEs and the `secret_key_versions` INSERT inside a single SQLite `BEGIN IMMEDIATE` transaction. The `secret_key_versions` INSERT must be the last statement in the transaction. If any UPDATE fails, the entire transaction rolls back, leaving all rows at the old key version." Add an acceptance test: inject a crash after the Nth UPDATE and assert all rows remain at `key_version = N` after the rollback.

---

### R10-F03 — MEDIUM: `resolve_secret_refs` contract for non-null, non-`$secret_ref` values at secret-marked paths is unspecified — schema migrations silently pass plaintext to Caddy

**Category:** Logic Flaws

**Trigger:** R7-F03's mitigation specifies that `null` at a secret-marked path is passed through as `null` by `resolve_secret_refs`. The unaddressed case: a non-null, non-`$secret_ref` value at a secret-marked path. This occurs concretely during a schema migration where a new release tags a previously-plaintext field as `secret`. All pre-migration snapshots contain the plaintext value at that path. `resolve_secret_refs` encounters a `String` or `Number` at the path rather than the expected `{"$secret_ref": "<ulid>"}` object. No contract is specified for this case.

Three possible implementations each have a different failure mode:
- **Reject with error:** Apply fails; routes with the affected field become unapplyable until re-issued. Recoverable but disruptive.
- **Pass through silently:** The plaintext value is sent to Caddy as-is — no audit trail, no redaction, no vault entry. Drift masking (R9-F01) would not detect this because the snapshot has plaintext at the path, matching Caddy's plaintext, so it compares as equal and raises no alarm. The secret leaks silently with no indication.
- **Silently encrypt and re-write:** Logic that belongs in the mutation pipeline, not the applier; violates single-responsibility.

**Consequence:** In the pass-through case (the most likely naive implementation), plaintext secrets leak to Caddy's running config with no audit trail, no redaction, no alarm, and no drift signal after a schema migration. The failure is invisible at every observation point.

**Design assumption violated:** The design assumes every value at a secret-marked path in a snapshot is either a valid `$secret_ref` or `null`. Schema migrations invalidate this for all pre-migration snapshots.

**Suggested mitigation:** Specify `resolve_secret_refs`'s contract: any non-null, non-`$secret_ref` value at a secret-marked path is a hard error — `Err(SecretsError::UnexpectedPlaintext { field_path })`. The apply is rejected. Document as a schema migration invariant: when a field is newly tagged `secret`, existing snapshots must be re-encrypted (migration task) or operators must re-issue the affected mutations before the upgrade. Add a test: `resolve_secret_refs_rejects_plaintext_at_secret_path`.

---

### R10-F04 — LOW: R8-F01 accepted residual documentation is incomplete — the Caddy admin BasicAuth credential is co-located in the unzeroable send buffer alongside resolved plaintext secrets

**Category:** Data Exposure

**Trigger:** The R8-F01 residual note states: "The reqwest send buffer containing the resolved Caddy config cannot be zeroed from application code." The admin block injected into every `POST /load` (R8-F03 fix) includes the Caddy admin API BasicAuth hashed credential in `"admin": {"identity": {...}}`. This credential is serialised into the same `Vec<u8>` POST body as all resolved plaintext secrets — the same unzeroable reqwest buffer.

The current R8-F01 documentation does not mention that the admin credential is co-located with plaintext secrets in the buffer. A future security auditor assessing "what is in the unzeroable buffer" will identify resolved secret values but miss the admin credential.

**Consequence:** Documentation gap only; no new structural failure. The buffer is already documented as unzeroable. The scope of the accepted residual is understated.

**Suggested mitigation:** Update the R8-F01 acceptance note to read: "The reqwest send buffer for each `POST /load` contains: (1) all resolved plaintext secrets, and (2) the Caddy admin API BasicAuth hashed credential embedded in the `admin.identity` block. Neither can be zeroed from application code. This is an accepted residual." No structural change required; the decision doc must be updated.

---

## Non-findings (explicit)

**Probe 2 — Drift masking and secret tampering:** R9-F01 explicitly accepts that drift detection is blind to secret field value changes. An alternative partial-detection strategy (hash comparison) would require either access to the plaintext or a new design element (hash stored alongside the `$secret_ref`). This is a design enhancement proposal, not an adversarial finding. No finding.

**Probe 5 — R8-F01 residual and admin credential (structural axis):** The structural no-zeroize finding is the accepted R8-F01 residual. Only the documentation gap is new (R10-F04 above). No new structural failure. No finding.

**Probe 6a — SchemaRegistry duplicate detection:** R8-F05 requires panic on duplicate registration. Closed. No finding.

**Probe 6b — OwnerKind::Other:** No V1 code path generates `OwnerKind::Other`. Round 6 probe 10(a) confirmed. No finding.

**Probe 6c — RedactedDiff newtype bypass:** The audit log writer accepts only `RedactedValue`/`RedactedDiff`, enforced by the type system. R9-F05 removed `redact` from `SecretsVault`. No bypass path. No finding.

**Probe 6d — AlgorithmTag exhaustiveness:** Confirmed non-finding in round 8 (probe 13). Rust enum, compiler-enforced. No finding.

**Probe 6e — Audit hash-chain interaction with secrets:** `SecretsRevealed` audit rows contain `target_id` and owner metadata, not plaintext. Redacted diff rows use the `RedactedDiff` newtype. No plaintext in the chain. Chain integrity is not broken by vault operations. No finding.

**Probe 6f — Key derivation path:** The vault uses the 32-byte master key directly for per-secret XChaCha20-Poly1305 encryption. No intermediary KDF in the Phase 10 design. This is the pre-existing design choice covered by R1-F01 (envelope encryption gap). No new finding in round 10.

---

## Summary

**Critical:** 0 · **High:** 1 · **Medium:** 2 · **Low:** 1 · **Non-findings:** 8

**Top concern:** R10-F01 — the Caddy admin API BasicAuth credential introduced by R9-F02 has no specified storage location. The vault-storage option creates a first-run circular dependency; the out-of-vault option is plaintext at rest and must be explicitly accepted as a documented exception (analogous to the master key). This must be resolved before Phase 10 is considered complete.

**Design-space signal:** After 10 rounds and 91 findings (28 confirmed non-findings), the design space is exhausted. The final sweep (probe 6) produced zero new findings across all remaining categories. The four findings this round are all consequences of the R9-F02 BasicAuth requirement (R10-F01, R10-F04) or gap-filling for existing mitigations (R10-F02, R10-F03).

**Space exhausted. Recommend `--final`.**
