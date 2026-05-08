# Adversarial Review — Phase 10 (Secrets Vault) — Round 8

**Design summary:** Phase 10 adds a secrets vault (encrypt/decrypt/rotate/redact) to Trilithon's local-first Caddy reverse-proxy manager, with a reveal endpoint, FileBackend and KeychainBackend, a mutation pipeline that substitutes secret refs before snapshot, and `resolve_secret_refs` in the applier (R6-F04 mitigation). R7-F01 requires `persist: false` in the Caddy bootstrap config; R7-F02 proposes wrapping the serialised resolved config in `Zeroizing<Vec<u8>>`.

**Prior rounds:** 76 findings across rounds 1–7 (including 10 non-findings). All treated as known. This round probes composition failures of round 7 fixes and any remaining surfaces.

---

## Findings

### R8-F01 — HIGH: `Zeroizing<Vec<u8>>` wrapper on the serialised Caddy POST body is a structural no-op — `reqwest::Body::from(Vec<u8>)` performs a zero-copy ownership transfer to `bytes::Bytes`

**Category:** Data Exposure

**Trigger:** R7-F02's mitigation wraps the serialised resolved config in `Zeroizing<Vec<u8>>` before passing it to the HTTP client. `reqwest::Body::from(vec: Vec<u8>)` calls `bytes::Bytes::from(vec)`, which reinterprets the heap allocation in-place without copying — zero-copy ownership transfer. The `Zeroizing` wrapper subsequently holds an empty `Vec<u8>` (the backing buffer was transferred) and calls `zeroize()` on an allocation of length zero. The actual plaintext bytes now live in the `Bytes` allocation, owned by reqwest's send pipeline, and are freed by reqwest without zeroing.

**Consequence:** The R7-F02 mitigation is a false fix. Plaintext secrets in the Caddy POST body remain in heap memory — unzeroed — for the lifetime of reqwest's internal send buffer. This is the residual documented in R7-F02's original finding; the proposed mitigation does not address it.

**Suggested mitigation:** Accept this as a known residual in the decision doc. The decision doc must state: "The reqwest send buffer containing the resolved Caddy config cannot be zeroed from application code. This is an accepted residual; plaintext bytes exist in the reqwest send pipeline for the duration of the HTTP call. Mitigated operationally by `persist: false` (R7-F01) and Caddy admin API binding to `127.0.0.1` only." If zeroing is required, a custom `hyper::Body` implementation that zeroes its internal buffer on drop is the only viable approach — document this as a future hardening option.

---

### R8-F02 — MEDIUM: R7-F01's mitigation checks `persist: false` via `GET /api/config/` at startup — this is a post-hoc assertion, not enforcement; autosave occurs before the check completes

**Category:** Data Exposure

**Trigger:** R7-F01's proposed mitigation probes Caddy's current config at startup via `GET /api/config/` and refuses to start if `admin.config.persist` is not `false`. But on first run, Caddy may start in its default configuration (persist enabled) before Trilithon's bootstrap config is applied. The sequence is: Caddy starts → autosave path created with default persist-enabled config → Trilithon startup probe reads config → probe passes because the bootstrap config POST (which sets `persist: false`) has not yet been sent. If Trilithon's first `resolve_secret_refs` apply fires before the bootstrap POST, autosave captures the resolved plaintext config.

**Consequence:** The startup check can pass in a window where Caddy still has persistence enabled. The check is condition-confirming, not condition-enforcing.

**Suggested mitigation:** Embed `"admin": {"config": {"persist": false}}` directly in the bootstrap config payload that Trilithon sends to Caddy at startup — before any `resolve_secret_refs` call. This makes persistence-disabling part of the configuration act, not a verification step. The startup probe becomes secondary confirmation.

---

### R8-F03 — MEDIUM: The `persist: false` assertion is startup-only — a third-party `PATCH /api/config/admin` to Caddy re-enables persistence silently

**Category:** Data Exposure

**Trigger:** R8-F02's mitigation embeds `persist: false` in the bootstrap config, enforcing the setting at startup. But Caddy's admin API (`PATCH /api/config/admin`) can modify the admin config at runtime. Any process with access to the Caddy admin socket (which must be accessible to Trilithon itself) can re-enable persistence. Subsequent `resolve_secret_refs` applies then produce autosaved plaintext config on disk, with no signal to Trilithon.

**Consequence:** A misconfigured or adversarial process on the same host can permanently re-enable autosave between Trilithon's applies without Trilithon detecting the change.

**Suggested mitigation:** Include `"admin": {"config": {"persist": false}}` in every resolved-config POST body sent to Caddy (`POST /load`), not just the bootstrap. Since `/load` replaces the entire running config, this re-asserts persistence-disabled on every apply cycle, eliminating the runtime window.

---

### R8-F04 — MEDIUM: The `is_current` column (R7-F04) and INSERT-new-row strategy (R2-F03) require a partial unique index — neither mitigation specifies this, and the full UNIQUE index blocks the INSERT

**Category:** Logic Flaws

**Trigger:** R2-F03 changes `upsert_secret` to INSERT a new row on every update, relegating the old row to history. R7-F04 adds `is_current BOOLEAN DEFAULT TRUE` and sets it to `FALSE` on supersession. But the existing UNIQUE index `ON secrets_metadata(owner_kind, owner_id, field_path)` is not partial — it covers all rows including history rows. When `upsert_secret` inserts a new row for an already-seen `(owner_kind, owner_id, field_path)` tuple (even after marking the old row `is_current = FALSE`), the INSERT fails with a UNIQUE constraint violation because the history row still carries the same tuple.

**Consequence:** R2-F03 and R7-F04 cannot both be applied without modifying the index. The migration as specified is internally inconsistent. The INSERT-new-row strategy is structurally blocked by the existing UNIQUE constraint.

**Suggested mitigation:** Replace the full unique index with a partial unique index:
```sql
CREATE UNIQUE INDEX IF NOT EXISTS secrets_metadata_owner_field_current
    ON secrets_metadata(owner_kind, owner_id, field_path)
    WHERE is_current = TRUE;
```
SQLite supports partial unique indexes. Add `is_current = FALSE` to the supersession UPDATE before the new-row INSERT. Document that the partial index allows multiple history rows per `(owner_kind, owner_id, field_path)` and that `is_current = TRUE` is the invariant enforced at the application level.

---

### R8-F05 — LOW: Duplicate `field_path` entries in the `SchemaRegistry` produce a misleading `409 secret_field_conflict` error instead of a schema validation error

**Category:** Logic Flaws

**Trigger:** If `SchemaRegistry::register` is called twice with the same entity type and overlapping `field_path` values (a schema registration bug, not a concurrent mutation), the second registration produces a duplicate in the in-memory map. When `extract_secrets` visits the duplicate paths, `upsert_secret` succeeds for the first visit and hits the (partial) UNIQUE constraint on the second visit. The error surfaces as `StorageError::SecretFieldConflict`, which maps to `409 secret_field_conflict` in the API response — the same code used for concurrent-mutation conflicts. The operator receives a 409 with no indication the root cause is a schema registration bug.

**Consequence:** Schema registration bugs are invisible at registration time and surface as misleading concurrency errors at mutation time. Operators debugging intermittent 409s may not check schema registration as a cause.

**Suggested mitigation:** `SchemaRegistry::register` must detect duplicate `field_path` entries for the same entity type and panic (or return `Err`) at registration time. Registration is a startup-phase operation; a panic here is appropriate and surfaces the bug before any traffic is served. Add a test: `register_duplicate_field_path_panics`.

---

## Non-findings (explicit)

**Probe 9 — ULID recycling collision:** ULID generation uses cryptographic randomness for its random component (80 bits). For a table with 10^6 rows, birthday-paradox collision probability is ~10^{-18}. Unreachable in practice. No finding.

**Probe 10 — JSON BLOB storage overhead:** Storing `Ciphertext` as a JSON BLOB (algorithm tag, nonce, ciphertext, key_version) adds ~120 bytes per row versus raw binary. For 10,000 secrets, overhead is ~1.2 MB. Well within SQLite's practical limits. No finding.

**Probe 11 — Per-apply decrypt performance:** For a Caddy config with 100 secret-bearing fields, `resolve_secret_refs` performs 100 XChaCha20-Poly1305 decryptions. Benchmark data for similar operations: ~1 µs/op on modern hardware → 100 µs total. Sub-millisecond; negligible versus Caddy's HTTP round-trip. No finding.

**Probe 12 — `JsonPointer` normalization collision:** Two field paths that normalize to the same JSON Pointer (e.g., `/a/b` and `/a/b/`) are prevented by `JsonPointer`'s validation, which rejects trailing slashes. This is enforced at the type level. No finding.

**Probe 13 — `AlgorithmTag` exhaustiveness:** `AlgorithmTag` is a Rust enum with a single variant (`XChaCha20Poly1305`). Match arms on `AlgorithmTag` are exhaustive at compile time. Adding a new algorithm requires adding a new variant, at which point the compiler flags all non-exhaustive matches. No runtime gap. No finding.

---

## Summary

**Critical:** 0 · **High:** 1 · **Medium:** 3 · **Low:** 1 · **Non-findings:** 5

**Top concern:** R8-F01 — the `Zeroizing<Vec<u8>>` wrapper proposed by R7-F02 is a structural no-op due to `reqwest::Body::from(Vec<u8>)` performing a zero-copy ownership transfer. The mitigation must be redesigned or explicitly accepted as a known residual.

**Design-space assessment:** After 8 rounds and 81 findings (15 confirmed non-findings), the design space is substantially exhausted. R8-F04 (partial unique index) is the last structural inconsistency. R8-F01 requires an explicit acceptance note in the decision doc. R8-F02 and R8-F03 refine the `persist: false` enforcement strategy. After these four are incorporated into the design, **recommend `--final`**.
