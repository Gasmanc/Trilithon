# Adversarial Review — Phase 10 (Secrets Vault) — Round 7

**Design summary:** Phase 10 adds a secrets vault (encrypt/decrypt/rotate/redact) to Trilithon's local-first Caddy reverse-proxy manager, with a reveal endpoint, FileBackend and KeychainBackend, a mutation pipeline that substitutes secret refs before snapshot, and a proposed `resolve_secret_refs` step in the applier (R6-F04 mitigation).

**Prior rounds:** 70 findings across rounds 1–6 (including 7 non-findings). All treated as known. This round probes composition failures of round 6 fixes and remaining surfaces.

---

## Findings

### R7-F01 — HIGH: Caddy autosave persists the fully-resolved plaintext config to disk — complete encryption-at-rest bypass introduced by R6-F04's mitigation

**Category:** Data Exposure

**Trigger:** R6-F04's `resolve_secret_refs` step substitutes decrypted plaintexts into the Caddy config before `caddy_client.load_config`. Caddy's admin API accepts this config and, by default, persists its running configuration to `autosave.json` (`$CADDY_DATA_DIR/autosave.json`). This file is written by Caddy without any encryption. Any user or process with read access to the Caddy data directory — including backup jobs, monitoring agents, log shippers, or an attacker with local file read — obtains all secret values in plaintext without touching the vault.

**Consequence:** The vault's encryption-at-rest guarantee is bypassed entirely. Backup tarballs of the Caddy data directory become plaintext secret dumps. This is a regression introduced by the fix for R6-F04 — the design was correct to withhold secrets from Caddy, but the applier resolution step creates a new plaintext persistence path that was not present before.

**Design assumption violated:** The design assumes only the vault ciphertext store and transient in-memory buffers contain plaintext secrets. Caddy's own persistence layer is outside the vault's control and is not addressed in any slice.

**Suggested mitigation:** The applier's bootstrap step must verify before sending any resolved config that Caddy is configured with `admin.config.persist: false`. If this condition is not met, the daemon must refuse to start with a structured error: `"Caddy autosave must be disabled when Trilithon manages secrets; set admin.config.persist: false in the bootstrap config."` This must be a hard startup precondition, not a deployment note. Add a capability-probe check for this setting alongside the existing Caddy capability probes.

---

### R7-F02 — HIGH: Resolved plaintext Caddy config serialised as `String`/`Vec<u8>` for the HTTP POST body — not zeroed after the call

**Category:** Data Exposure

**Trigger:** After `resolve_secret_refs` substitutes plaintexts, the applier serialises the resolved config via `serde_json::to_vec` or `to_string` for the POST body. This buffer contains every plaintext secret in the payload. It is passed to `reqwest` (or equivalent), which may copy it into an internal send buffer. The local `String`/`Vec<u8>` is dropped after the call; Rust's default drop does not zero heap memory. Rounds 2–3 addressed zeroizing of `CipherCore.key` and `ExtractedSecret.plaintext`; this is a third plaintext location not yet in scope.

**Consequence:** Plaintext secrets are readable in process memory dumps, core files, or via heap inspection for an indeterminate period after the HTTP call completes. The duration is bounded by allocator reuse, which is non-deterministic.

**Suggested mitigation:** Wrap the serialised request body in `Zeroizing<Vec<u8>>` before passing to the HTTP client. Document the residual: `reqwest`'s internal copy of the send buffer is not zeroable from outside the crate. The decision doc must explicitly accept or reject this residual as a known gap, with reasoning.

---

### R7-F03 — MEDIUM: `extract_secrets` on `null` or `""` values loses type information — pipeline has no specified contract for absent or intentionally-empty secrets

**Category:** Logic Flaws

**Trigger:** A route config has a secret-marked field set to `null` (intentionally absent) or `""` (explicitly cleared). `ExtractedSecret.plaintext: String` has no representation for JSON `null`. If the pipeline serialises `null` as the string `"null"` and encrypts it, the reveal produces the string `"null"` where the caller expected JSON null — type information is irreversibly lost. If the pipeline skips `null` values, the `null` remains in `desired_state_json` unencrypted, but no `$secret_ref` is inserted. The `resolve_secret_refs` step must then handle paths that have `null` instead of a `$secret_ref` object — a case the design does not specify.

**Consequence:** Ambiguous pipeline behaviour on null/empty secrets, with two failure modes: silent type corruption (null becomes the string `"null"`) or unspecified behaviour in `resolve_secret_refs` when it encounters a non-`$secret_ref` value at a secret-marked path.

**Suggested mitigation:** Specify explicitly: (a) `null` at a secret-marked path is skipped by `extract_secrets`; the path is left as `null` in the snapshot; `resolve_secret_refs` passes it through as `null` to Caddy. (b) `""` is treated as a valid secret and encrypted normally. Both cases must be documented and have explicit test coverage.

---

### R7-F04 — MEDIUM: Rotation scan re-encrypts all historical orphaned rows — O(N×M) cost instead of O(M)

**Category:** Resource Exhaustion

**Trigger:** R2-F03's INSERT-new-row approach accumulates N×M rows for M secret fields each updated N times. The rotation scan `SELECT * FROM secrets_metadata WHERE key_version = ?` is not filtered to current rows. Rotation decrypts and re-encrypts every historical row at the old key version, not just the M live rows.

**Consequence:** On a table with 1,000 fields × 100 updates (100K rows), rotation performs 100K decrypt/encrypt/write operations — a minutes-long blocking operation that contends with concurrent reads. `MasterKeyRotation.re_encrypted_rows` misrepresents progress (reports history count, not live-secrets count).

**Suggested mitigation:** Scope the rotation query to current rows only. Add `is_current BOOLEAN DEFAULT TRUE` to `secrets_metadata`, set to `FALSE` on supersession, and add an index on `(key_version, is_current)`. The rotation query becomes `SELECT * FROM secrets_metadata WHERE key_version = ? AND is_current = TRUE`. The design doc must state explicitly which rows rotation touches.

---

### R7-F05 — MEDIUM: R6-F01 ownership check returns ambiguous error when `owner_id` references a deleted route — 403 and 404 are indistinguishable

**Category:** Logic Flaws

**Trigger:** R6-F01's fix loads the route identified by `(row.owner_kind, row.owner_id)` before decrypting. Until R2-F05 (secret cleanup on route deletion) is implemented, orphaned secret rows exist whose `owner_id` references a deleted route. The storage load returns `None`. The handler must choose a status code: 404 (misleads — the ciphertext exists), 403 (indistinguishable from the R6-F01 unauthorized-access case), or 410 (leaks deletion history).

**Consequence:** Operators hitting orphaned secrets receive an error they cannot act on. The 403 response is indistinguishable from the legitimate "you don't own this resource" case, blocking legitimate debugging and support workflows.

**Suggested mitigation:** The design must specify the error response explicitly and note the R2-F05 dependency: use a structured error body that distinguishes `owner_not_found` from `secret_not_found`, e.g. `{ "code": "owner_deleted", "owner_kind": "...", "owner_id": "..." }`. This is a design-doc requirement, not just an implementation note.

---

### R7-F06 — LOW: R2-F10 fix (return `Err` on schema miss) creates a hard startup ordering requirement that is not documented

**Category:** Logic Flaws

**Trigger:** R2-F10's fix changes `extract_secrets` to return `Err(SchemaNotFound)` on an unknown schema path. If schema registration is lazy (triggered on first mutation of a new entity type), the first mutation after a daemon restart or after a new entity type is added in a future phase fails hard with `SchemaNotFound`. This is indistinguishable from a misconfigured schema.

**Consequence:** Operators see hard failures on the first mutation of a new entity type with no indication that retrying after schema registration completes will succeed.

**Suggested mitigation:** The design must mandate eager schema registration: all entity-type schemas are registered synchronously in the daemon's startup sequence, before the HTTP listener is bound. Lazy registration is explicitly prohibited. Document this as a startup invariant.

---

## Non-findings (explicit)

**Probe 6 — In-memory session cache bypass after daemon restart:** Phase 9 uses SQLite as the persistent session store with no in-memory cache layer. R5-F05's re-validation query hits SQLite directly. No cache to invalidate. No finding.

**Probe 7 — Caddy interprets `{"$secret_ref": ...}` as a special directive:** Caddy's JSON config does not define `$`-prefixed keys as extension points. Caddy will treat the object as an unexpected type at the config path and reject the config with a validation error — a safe failure, not a security issue. No new finding beyond R6-F04.

**Probe 8 — `re_encrypted_rows: u32` overflow:** At realistic scale (thousands of secrets), 2^32 rows is unreachable. Related u32 overflow concern already flagged as R4-F09. No new finding.

---

## Summary

**Critical:** 0 · **High:** 2 · **Medium:** 3 · **Low:** 1 · **Non-findings:** 3

**Top concern:** R7-F01 — Caddy's `autosave.json` persists the fully-resolved plaintext config to disk after R6-F04's mitigation is applied, completely bypassing the vault's encryption-at-rest guarantee unless Caddy persistence is explicitly disabled and enforced as a hard startup precondition.

**Design-space assessment:** After 7 rounds and 76+ findings, the space is substantially exhausted. The two HIGHs this round (R7-F01, R7-F02) are new surfaces introduced specifically by R6-F04's mitigation. The three MEDIUMs are implementation-detail corrections that can be captured in the decision doc. If R7-F01 and R7-F02 are addressed in the design, recommend **`--final`**.
