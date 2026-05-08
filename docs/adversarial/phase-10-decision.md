# Adversarial Decision — Phase 10 (Secrets Vault)

**Review process:** 10 rounds, 91 findings, 28 confirmed non-findings.
**Recommendation:** Implementation may proceed after the required design changes in §2 are incorporated. All findings are classified as either a required design change, an accepted residual, or a confirmed non-finding.

---

## §1 — Process summary

| Round | Critical | High | Medium | Low | Non-findings |
|-------|----------|------|--------|-----|--------------|
| 1 | 6 | 6 | 4 | 1 | 0 |
| 2 | 4 | 4 | 4 | 2 | 0 |
| 3 | 3 | 4 | 3 | 1 | 0 |
| 4 | 0 | 4 | 3 | 2 | 1 |
| 5 | 0 | 3 | 4 | 1 | 2 |
| 6 | 0 | 3 | 2 | 1 | 4 |
| 7 | 0 | 2 | 3 | 1 | 3 |
| 8 | 0 | 1 | 3 | 1 | 5 |
| 9 | 0 | 3 | 1 | 2 | 5 |
| 10 | 0 | 1 | 2 | 1 | 8 |
| **Total** | **13** | **31** | **29** | **13** | **28** |

---

## §2 — Required design changes

These findings must be addressed in the Phase 10 design documents before implementation begins. They are grouped by subsystem.

### 2.1 — Encryption architecture

**R1-F01 / R1-F02 — Envelope encryption vs. direct master-key encryption**
ADR-0014 mandates envelope encryption (per-secret data key → workspace key → master key). The TODO slices implement direct master-key encryption, which invalidates the ADR's rotation-cheapness argument and creates an O(N) bulk re-encryption window with no crash-recovery guarantee. Resolution: either restore envelope encryption per ADR-0014, or produce a revised ADR that explicitly retracts the envelope requirement and specifies an atomic rotation slice (using a `rotation_state` table or a single `BEGIN IMMEDIATE` transaction over all rows). The three-layer rule mandates that the rotation loop (which requires database I/O) lives in `adapters`, not `core`. `rotate_master_key` must be removed from `SecretsVault` or split into a pure `re_encrypt_blob` function in core plus an adapter loop.

**R4-F04 — `EncryptContext.key_version` must come from the stored ciphertext, never from `vault.current_key_version()`**
The reveal handler must populate `key_version` from `row.ciphertext.key_version`. Using the current key version causes authentication-tag mismatch for all pre-rotation ciphertexts. Add an acceptance test: encrypt under v1, rotate to v2, reveal — must succeed. Alternative: remove `key_version` from AAD entirely and bind AAD only to the row's stable identity (`owner_kind`, `owner_id`, `field_path`).

**R6-F05 — `backend_kind` must be per key-version, not per row**
Add `secret_key_versions(version PK, backend_kind, created_at)`. Remove `backend_kind` from `secrets_metadata`. A partial rotation (crash between row N and row N+1) cannot leave mixed-backend rows when `backend_kind` is a property of the version entry. The version entry is written atomically after all re-encryptions succeed.

**R10-F02 — `secret_key_versions` INSERT and re-encryption UPDATEs must share a transaction**
All re-encryption UPDATEs and the `secret_key_versions` INSERT must execute inside a single `BEGIN IMMEDIATE` transaction. A crash mid-rotation must leave all rows at the old key version (full rollback), never in a state where rows reference a version with no `secret_key_versions` entry.

### 2.2 — Schema and migrations

**R2-F02 — Migration slot collision**
Rename `0004_secrets.sql` to `0006_secrets.sql` (the next available slot). Add a CI check asserting migration filenames form a gapless ascending sequence with no duplicates.

**R4-F07 — Architecture §6.9 DDL is diverged from the migration**
Update architecture §6.9 to include `algorithm`, `key_version`, `is_current`, `backend_kind` (removed in favour of `secret_key_versions`), and the partial unique index. Add a CI check that diffs the live schema (produced by the migration runner) against the architecture DDL.

**R8-F04 — Partial unique index required for INSERT-new-row strategy**
The UNIQUE index `ON secrets_metadata(owner_kind, owner_id, field_path)` must be replaced with a partial unique index:
```sql
CREATE UNIQUE INDEX secrets_metadata_owner_field_current
    ON secrets_metadata(owner_kind, owner_id, field_path)
    WHERE is_current = TRUE;
```
This allows multiple history rows per field while enforcing uniqueness of the current row.

**R1-F13 — Column name `field_path` must be consistent**
All references in the TODO, architecture docs, and query strings must use `field_path` (not `field_name`). Audit before the migration slice is written.

### 2.3 — Mutation pipeline

**R2-F03 — `upsert_secret` must INSERT a new row on every secret update**
Change `ON CONFLICT DO UPDATE SET` to an unconditional `INSERT` with a new ULID. The old row is set `is_current = FALSE`. This preserves rollback fidelity: old snapshots' `$secret_ref` values continue to resolve to the credential that was canonical when the snapshot was taken.

**R3-F01 — `route_mutation_secrets_through_vault` must be wrapped in a SQLite savepoint**
Wrap the entire call in a savepoint. If any `vault.encrypt` or `upsert_secret` call fails, roll back the savepoint before returning `Err`. Partial writes (orphaned, revealable ciphertext rows with no snapshot reference) are prevented by rollback.

**R9-F03 — `upsert_secret` UPDATE+INSERT must share an inner savepoint**
The `UPDATE SET is_current = FALSE` and the `INSERT` for the new row must execute within a single inner savepoint inside `upsert_secret`. If the INSERT fails, the UPDATE is rolled back. A failed INSERT must never leave zero current rows for a `(owner_kind, owner_id, field_path)` tuple.

**R3-F04 / R7-F04 — Stable `secret_id` per field; rotation scoped to current rows**
Assign a stable `secret_id` per `(owner_kind, owner_id, field_path)`. On mutation, check whether the new plaintext equals the currently stored plaintext (via decrypt-and-compare) to avoid generating a new row for a no-op edit (snapshot deduplication). The rotation query must be `WHERE key_version = ? AND is_current = TRUE`.

**R5-F01 — Concurrent secret update conflict must surface as a distinct 409**
When `upsert_secret`'s INSERT fails a UNIQUE constraint race (two concurrent mutations for the same field), return `StorageError::SecretFieldConflict { field_path, winning_mutation_id }` rather than silently adopting the winning row's ULID. Surface as `409 { "code": "secret_field_conflict", "field_path": "..." }` distinct from ADR-0012's `409 { "code": "version_conflict" }`. Document both codes and their required client handling in the API spec.

**R7-F03 — Explicit contract for `null` and `""` at secret-marked paths**
`null` at a secret-marked path: skipped by `extract_secrets`; left as `null` in the snapshot; `resolve_secret_refs` passes through as `null`. `""` (empty string): treated as a valid secret and encrypted normally. Both cases must have explicit test coverage.

**R10-F03 — `resolve_secret_refs` must reject unexpected plaintext at secret-marked paths**
Any non-null, non-`$secret_ref` value at a secret-marked path is a hard error: `Err(SecretsError::UnexpectedPlaintext { field_path })`. This prevents schema migrations from silently passing plaintext to Caddy with no audit trail. Document as a migration invariant: when a field is newly tagged `secret`, all affected snapshots must be re-encrypted or the affected mutations re-issued before deployment.

### 2.4 — Applier and Caddy integration

**R6-F04 — `resolve_secret_refs` step is required in the applier**
Add `resolve_secret_refs(desired_state_json, vault, storage) -> Result<CaddyConfig, SecretsError>` as a required step in the applier between snapshot load and `caddy_client.load_config`. The resolved form replaces every `{"$secret_ref": "<id>"}` with decrypted plaintext. The resolved form is never written to SQLite. Add an acceptance test asserting Caddy's running config contains the plaintext value after a secret-bearing mutation.

**R7-F01 / R8-F02 / R8-F03 — Caddy `persist: false` must be embedded and re-asserted on every POST**
Embed `"admin": {"config": {"persist": false}}` in the bootstrap config payload (enforced at startup before any `resolve_secret_refs` call). Include the same block via deep-merge in every subsequent `POST /load` body. Deep-merge strategy: `resolved_config["admin"] = deepmerge(resolved_config.get("admin").unwrap_or({}), {"config": {"persist": false}})` — ensuring `persist: false` wins while preserving other admin settings (R9-F04). Add a test asserting a pre-existing `"admin"` key in the snapshot retains its non-persist settings after the merge.

**R9-F01 — Drift detection must operate on non-secret fields only**
Secret-marked paths must be masked in both the snapshot form and the live Caddy config before comparison. The drift engine must not call `GET /config/` as a plaintext source on the drift path. Document the accepted operational gap: drift detection does not detect changes to secret field values.

**R9-F02 — Caddy admin API authentication required**
Configure Caddy's admin API with BasicAuth (`admin.identity`). Trilithon must use the credential in all its own admin API calls. Socket permissions restrict access to the Trilithon daemon's OS user. See §2.7 for credential storage.

**R10-F04 (merged with R9-F04) — Admin block injection merge strategy**
Specify explicitly: the admin block constant is owned by `resolve_secret_refs` and never stored in snapshots. The deep-merge ensures `persist: false` always wins without discarding operator-configured admin settings.

### 2.5 — Reveal endpoint

**R6-F01 — Ownership check required before decrypt**
After loading the secret row by `id`, load the owning resource identified by `(row.owner_kind, row.owner_id)` and assert the session's user has read permission on it. If not, return 403 without writing a reveal audit row; write `auth.access-denied` instead. Add a `reveal_wrong_owner_403.rs` test.

**R5-F03 — Plaintext must not be materialised before audit transaction commits**
The plaintext MUST NOT be placed in any response struct until after the audit transaction has committed. Call `commit()` first; if it errors, return an error response with no `RevealResponse` constructed; if it succeeds, then construct and return `RevealResponse { plaintext }`. Add an acceptance test that injects a `commit()` failure and asserts a non-200 response with no plaintext body.

**R4-F06 / R5-F05 — Session re-validation and `session_id` in `AuthContext`**
The reveal handler must re-query `SELECT revoked_at FROM sessions WHERE id = ?` as its first step. This requires `AuthContext::Session` to carry `session_id`. Document as a cross-phase dependency: Phase 10 slice 10.6 requires a patch to Phase 9 to add `session_id: SessionId` to `AuthContext::Session`.

**R4-F05 — Reveal route must not be covered by `TraceLayer` body logging**
Register the reveal route on a sub-router that does not attach `tower_http::trace::TraceLayer`, or introduce a `SensitiveBody` newtype that bypasses body-intercepting middleware.

**R2-F11 / R9-F06 — Audit INSERT and metadata UPDATE in the same transaction**
The `INSERT INTO audit_log` row and the `UPDATE secrets_metadata SET last_revealed_at, last_revealed_by` must execute within the same SQLite transaction. Either both commit or neither does.

**R3-F08 — Argon2 step-up must run sentinel hash on user-not-found**
Always run Argon2 against a pre-computed sentinel hash when the user record is not found, to prevent timing oracle exposure of account existence. Return 401 regardless; latency must be indistinguishable from a valid-user wrong-password case.

**R2-F06 — `RevealRequest.current_password` must be wrapped in `secrecy::Secret<String>`**
`RevealRequest` must derive neither `Debug` nor `Serialize`. Audit all error types that bubble from the reveal handler for accidental password inclusion.

**R6-F03 — Distinct 409 error codes for version conflict vs. secret field conflict**
`409 Conflict` from mutation endpoints carries two distinct codes: `version_conflict` (ADR-0012 rebasing guidance) and `secret_field_conflict` (re-read and re-submit required). Document both codes and their required resolution strategies in the API spec.

### 2.6 — Key storage backends

**R1-F04 — FileBackend rotation must be atomic**
Write to a temp file on the same filesystem (not `std::env::temp_dir()`), `fsync`, then `rename()`. `rename()` is POSIX-atomic. The stanza format must validate on read: both `version=` and `key=` lines must be present and the key must decode to exactly 32 bytes. A partial final stanza is discarded with a `tracing::warn!` event.

**R1-F03 / R5-F06 — KeychainBackend fallback gate**
Distinguish transient D-Bus errors from structural absence. File-backend fallback is gated on zero existing `is_current = TRUE` rows with `backend_kind` matching the keychain backend. The count check and vault activation must be in a `BEGIN EXCLUSIVE` transaction (or a PID-file-enforced single-writer) to prevent TOCTOU on concurrent daemon starts. Detect `EXDEV` on rename (cross-device temp dir) and surface as error rather than falling back to non-atomic copy.

**R5-F07 — KeychainBackend must verify round-trip fidelity after `set_password`**
After `set_password`, immediately call `get_password` and compare returned bytes to the generated key. If they differ, return `CryptoError::KeyringUnavailable { detail: "keychain encoding mismatch" }` and fall through to `FileBackend`.

**R3-F07 — `CipherCore::from_key_bytes` must take a borrow, not a copy**
Change signature to `from_key_bytes(bytes: &[u8; 32], ...)` (borrow, not copy) to prevent key material being copied onto the call stack. Alternatively, accept `Zeroizing<[u8; 32]>`.

**R2-F07 — `CipherCore.key` must be wrapped in `Zeroizing`**
Enable the `zeroize` feature on `chacha20poly1305`. Wrap the key field in `Zeroizing<[u8; 32]>`. Implement `Drop` to call `zeroize()`.

**R5-F02 — `ExtractedSecret.plaintext` must be `secrecy::Secret<String>`**
Change `plaintext: String` to `plaintext: secrecy::Secret<String>`. Remove `Clone` from `ExtractedSecret`'s derive (R6-F02). Audit all pipeline sites that depend on `ExtractedSecret: Clone` and replace clones with explicit construction. On savepoint rollback, dropped `Vec<ExtractedSecret>` correctly zeroes each `Secret<String>` before freeing heap.

### 2.7 — Caddy admin API credential storage (R10-F01)

The Caddy admin API BasicAuth credential (required by R9-F02) must be stored outside the vault. Vault storage creates a first-run circular dependency (vault cannot be initialised before Caddy is accessible, Caddy auth configuration requires the Caddy API to be callable). Specified bootstrap:

1. Trilithon generates a random Caddy admin credential at first run and writes it to a `0600` file (analogous to the master-key file fallback).
2. Trilithon sends the bootstrap config (including `persist: false` and the `admin.identity` BasicAuth block with the hashed credential) using this credential. Subsequent API calls use the same `0600`-stored credential.
3. Document in ADR-0014: the Caddy admin credential is an accepted plaintext-at-rest item, analogous to the master key itself, protected only by filesystem permissions.

### 2.8 — Audit and redaction

**R1-F06 — Redaction token must hash the plaintext, not the `secret_id`**
ADR-0014 mandates `SHA-256(plaintext)` prefixed `secret:`. Remove the ULID-hash approach. If in-memory concern exists, address in `VaultBackedHasher` design, not by switching hash input.

**R5-F04 / R9-F05 — Single canonical redaction entry point**
Remove `redact` from the `SecretsVault` trait. Move to a standalone free function `fn redact_secret_value(plaintext: &str) -> RedactedToken` in `core/src/secrets/redact.rs`. Both `DiffEngine::redact_diff` and all audit-producing paths call it. The `SecretsVault` trait exposes only `encrypt`, `decrypt`, `rotate_master_key`.

**R2-F08 / R3-F09 — Audit BLOB redaction and error semantics**
Map BLOB deserialization failures to `StorageError::CorruptedCiphertext { id, detail }`. The vault layer maps this to `CryptoError::Decryption { detail: "ciphertext blob malformed" }` and fires `tracing::error!(target = "storage.integrity-check.failed", ...)`. The `DiffEngine` must redact diff BLOBs before they reach `audit_log.redacted_diff_json`.

**R1-F08 — `master_key_initialised` audit event**
After generating the master key, emit `master_key_initialised` to the audit log before the vault is used for any encrypt/decrypt operation. Serialise the startup sequence: migrations → vault init (including audit row) → hash-chain check → begin serving requests (R3-F06).

### 2.9 — Schema registry and secret cleanup

**R2-F10 — `extract_secrets` must return `Err` on unknown schema path**
Return `Err(SchemaNotFound)` on an unknown schema path rather than an empty `Vec`. All entity-type schemas must be registered synchronously in the daemon's startup sequence, before the HTTP listener is bound (R7-F06 — eager registration is a startup invariant).

**R8-F05 — `SchemaRegistry::register` must detect duplicate `field_path` entries**
Panic (or return `Err`) at registration time on duplicate `field_path` for the same entity type. Registration is a startup-phase operation; a panic surfaces the bug before any traffic is served.

**R2-F05 — Explicit secret deletion on owner deletion**
Add `delete_secrets_for_owner(owner_kind, owner_id)` inside the Route/Upstream deletion transaction. Add a `secrets.deleted` audit kind.

**R7-F05 — Structured error for `owner_not_found` vs `secret_not_found`**
The ownership check must distinguish `owner_deleted` from `secret_not_found` via a structured error body: `{ "code": "owner_deleted", "owner_kind": "...", "owner_id": "..." }`. This is a design-doc requirement.

### 2.10 — Cross-cutting

**R6-F06 — Hard-delete vs soft-delete for secrets**
Choose one strategy explicitly. If soft-delete: add `deleted_at INTEGER` to `secrets_metadata`, change `get_secret` to `WHERE id = ? AND deleted_at IS NULL`, add `reveal_deleted_secret_404.rs`. If hard-delete: document that the plaintext value of a deleted secret is not recoverable and require explicit operator acknowledgment before route deletion.

**R3-F02 — Bootstrap credentials must flow through the vault**
After vault initialisation, encrypt bootstrap credentials under the vault. Store ciphertext in `secrets_metadata` with `owner_kind = User`, `owner_id = "bootstrap"`, `field_path = "/password"`. Write plaintext to the 0600 file only as a transient delivery mechanism. Delete the file when the user changes their password; record the deletion in the audit log.

**R3-F03 — Cross-machine restore must probe vault readiness**
Before advancing the desired-state pointer, enumerate `secrets_metadata` key versions, attempt `vault.decrypt` on one ciphertext per version, and refuse the restore with a structured error listing missing key versions if any fail. Surface `system.restore-cross-machine` only after this check passes.

**R3-F05 / R2-F14 — LLM gateway must never resolve `$secret_ref`**
All read handlers must return `$secret_ref` markers verbatim, never resolved. Define a `SecretField` enum with `Redacted(String)` and `Plaintext(String)` variants — the compiler prevents read handlers from returning a `Plaintext` variant without an explicit vault call.

**R5-F10 — Vault initialisation must complete before the HTTP listener binds**
Enforce a strict startup barrier: vault initialisation and storage migration must complete before `HttpServer::bind` is called.

**R4-F02 — Rollback preflight must re-verify `$secret_ref` rows within a transaction**
Wrap the rollback preflight and apply in a SQLite exclusive transaction with a savepoint. Re-verify `$secret_ref` rows exist immediately before the desired-state pointer write. If any ref has been deleted, abort with `RollbackError::SecretRefGone { ids }`.

**R4-F09 — Rotation version overflow check**
Use `u32::checked_add(1)` with `CryptoError::KeyringUnavailable { detail: "key version overflow" }` on failure.

**R1-F11 — `CryptoError::EntropyFailure` for `getrandom` failures**
Add `CryptoError::EntropyFailure(String)` and map `getrandom` errors to it.

---

## §3 — Accepted residuals

These gaps are acknowledged as structurally unresolvable within the Phase 10 design constraints. Each must be documented in the ADR or the decision doc before the phase ships.

**R8-F01 / R10-F04 — reqwest send buffer cannot be zeroed**
`reqwest::Body::from(Vec<u8>)` calls `bytes::Bytes::from(vec)` (zero-copy ownership transfer). The `Zeroizing` wrapper holds an empty Vec and zeroes nothing. The actual plaintext bytes — including all resolved secret values and the Caddy admin BasicAuth hashed credential embedded in the `admin.identity` block — remain in the reqwest send buffer until freed by reqwest without zeroing. This is an accepted residual. Mitigated operationally by `persist: false` (prevents Caddy from persisting the received config to disk) and by restricting admin socket access to the Trilithon daemon's OS user. If zeroing is required in a future hardening phase, a custom `hyper::Body` implementation that zeroes its internal buffer on drop is the only viable approach.

**R2-F08 — Loopback traffic is readable by co-resident privileged processes**
`RevealResponse` is returned over unencrypted loopback TCP. A process with `CAP_NET_RAW` or a BPF program can read reveal responses. This is explicitly out-of-scope per ADR-0011. Document in ADR-0014 that co-resident privileged processes are an out-of-scope threat.

**R2-F12 — Historical master key versions accumulate in the FileBackend file**
Until rotation re-encryption is complete (future phase), the `master-key` file accumulates all historical master key versions in plaintext. Document the risk and note that the file is protected only by filesystem permissions (0600). Every backup of the data directory captures all historical keys.

**R9-F01 (drift gap) — Drift detection is blind to secret field value changes**
Secret-marked paths are masked during drift comparison. External modification of Caddy's running config at a secret-bearing field is not detectable by drift detection. This is an accepted operational gap documented explicitly in the Phase 10 design.

**R10-F01 (credential storage exception) — Caddy admin BasicAuth credential is plaintext at rest**
The Caddy admin credential is stored in a `0600` file outside the vault, analogous to the master key itself. This is an accepted plaintext-at-rest exception. ADR-0014 must list this alongside the master key as an explicitly accepted exception to vault coverage.

---

## §4 — Confirmed non-findings

The following probes produced no concrete failure scenario and are formally closed:

- **R4-F08** — `Secret<String>` heap residue on async cancellation: `zeroize::Zeroize` correctly handles async cancellation; bytes are zeroed when the future is dropped.
- **R5-F08** — `Ciphertext` serde round-trip: `serde_json` serialises `Vec<u8>` as integer arrays (0–255); lossless round-trip.
- **R5-F09** — `Zeroizing<[u8; 32]>: Send + Sync`: satisfied transitively; no trait-bound failure.
- **R6 probe 9** — `rotate_master_key` async and object safety: `#[async_trait]` boxes futures as `Box<dyn Future>`, which is object-safe.
- **R6 probe 10b** — SQLite WAL mode and savepoints: savepoints function identically in WAL and rollback-journal mode.
- **R7 probe 6** — In-memory session cache bypass after restart: Phase 9 uses SQLite as the persistent session store with no in-memory cache layer.
- **R7 probe 7** — Caddy interprets `{"$secret_ref": ...}` as a special directive: Caddy rejects the config with a validation error — a safe failure, not a security issue.
- **R7 probe 8** — `re_encrypted_rows: u32` overflow: at realistic scale, 2^32 rows is unreachable.
- **R8 probe** — ULID recycling collision: 80-bit random component; birthday-paradox probability ~10^{-18} at 10^6 rows.
- **R8 probe** — JSON BLOB storage overhead: ~120 bytes/row; ~1.2 MB for 10,000 secrets; within SQLite's practical limits.
- **R8 probe** — Per-apply decrypt performance: ~1 µs/op × 100 fields = ~100 µs total; sub-millisecond versus Caddy's HTTP round-trip.
- **R8 probe** — `JsonPointer` normalisation collision: trailing slashes rejected by `JsonPointer` validation at the type level.
- **R8 probe** — `AlgorithmTag` exhaustiveness: single-variant Rust enum; compiler-enforced exhaustive match.
- **R9 probe** — Abuse cases / rate limiting: covered by R1-F05, R2-F13.
- **R9 probe** — Resource exhaustion: no new unbounded allocation scenario.
- **R9 probe** — Single points of failure: covered by R1-F03, R5-F07, R5-F10.
- **R9 probe** — Rollback atomicity (beyond R9-F03): covered by R2-F03, R3-F01, R4-F02.
- **R9 probe** — Eventual consistency: SQLite is the single store; no multi-store gap.
- **R10 probe** — Drift masking and secret tampering: accepted operational gap per R9-F01.
- **R10 probe** — SchemaRegistry duplicate detection: closed by R8-F05.
- **R10 probe** — `OwnerKind::Other` unhandled: no V1 code path generates `OwnerKind::Other`.
- **R10 probe** — `RedactedDiff` newtype bypass: type-system enforced; no bypass path.
- **R10 probe** — `AlgorithmTag` exhaustiveness: confirmed non-finding (rounds 8 and 10).
- **R10 probe** — Audit hash-chain interaction with secrets: no plaintext in the chain; vault operations do not break chain integrity.
- **R10 probe** — Key derivation path: direct 32-byte master key use is the pre-existing design choice covered by R1-F01; no new finding.
- **R10 probe** — R8-F01 residual structural axis: only the documentation gap (R10-F04) is new; no new structural failure.
- **R10 probe** — Multi-process keychain race: single-daemon local-first tool; not a realistic scenario.

---

## §5 — Sign-off

**Space exhausted.** After 10 rounds, no new attack categories produced concrete findings in round 10 beyond the six surfaces explicitly identified in the brief. All structural weaknesses are documented above.

**Implementation may proceed** once the required design changes in §2 are reflected in the Phase 10 TODO slices, the architecture document (§6.9, §11, §12.1), and ADR-0014. The accepted residuals in §3 must be documented in ADR-0014 before the phase ships.

**Priority order for design revisions:**
1. §2.4 (applier: `resolve_secret_refs`, `persist: false`, drift masking) — functional show-stopper
2. §2.1 (envelope encryption decision) — data-loss on rotation
3. §2.7 (Caddy admin credential bootstrap) — no valid first-run sequence otherwise
4. §2.3 (mutation pipeline atomicity: savepoints, INSERT-new-row, conflict codes)
5. §2.5 (reveal endpoint: ownership check, commit ordering, session re-validation)
6. §2.2, §2.6, §2.8–§2.10 (schema, backends, audit, cross-cutting)
