# Adversarial Review — Phase 10 (Secrets Vault) — Round 9

**Design summary:** Phase 10 adds a secrets vault (XChaCha20-Poly1305, master key from OS keychain or 0600 file) to Trilithon. The mutation pipeline substitutes `$secret_ref` markers before snapshot. `resolve_secret_refs` decrypts refs and POSTs the resolved plaintext to Caddy with `"admin": {"config": {"persist": false}}` injected on every call. The reveal endpoint is step-up authenticated with ownership checks and audit logging.

**Prior rounds:** 81 findings across rounds 1–8 (15 confirmed non-findings). All treated as known. This round attacks six specific structural surfaces not previously identified.

---

## Findings

### R9-F01 — HIGH: Drift detection compares Caddy's live plaintext against snapshot `$secret_ref` markers — permanent false-positive drift on every secret-bearing route

**Category:** Logic Flaws

**Trigger:** The drift-detection loop calls `get_running_config()` (which issues `GET /config/` to Caddy) and compares the result against the current `desired_state_json` from the snapshot. After Phase 10, the snapshot's `desired_state_json` stores `{"$secret_ref": "<ulid>"}` at secret-marked paths. Caddy's running config, loaded via `resolve_secret_refs` and `POST /load`, contains the resolved plaintext strings at those paths. `get_running_config()` returns plaintext; the snapshot contains markers. At every drift-detection tick, every secret-bearing field compares `{"$secret_ref": "01HZ..."}` (snapshot) against `"myActualPassword"` (Caddy live) and declares drift.

**Consequence:** Drift detection fires continuously on every route that contains a secret field. The drift-resolution loop re-applies the config, triggering another `resolve_secret_refs` + `POST /load`, immediately followed by another drift check that again finds divergence. The system enters a permanent drift/re-apply cycle for any deployment with secrets. `caddy.drift-detected` audit events are emitted continuously, flooding the audit log and obscuring real drift on non-secret fields.

**Design assumption violated:** The design assumes `desired_state_json` and Caddy's running config are in the same representation. After Phase 10's substitution they are structurally incompatible on secret fields: the snapshot is in marker form, Caddy holds plaintext.

**Suggested mitigation:** Drift detection must operate only on non-secret fields. The schema registry marks which paths are secret; the drift engine masks those paths in both the snapshot form and the live config form before comparing. This avoids periodic decrypt calls in the drift loop and never materialises plaintext on the drift path. Document explicitly that drift detection does not detect changes to secret field values — secret field drift is an accepted operational gap.

---

### R9-F02 — HIGH: Caddy's `GET /config/` returns resolved plaintext secrets with no auth, no audit row, and no ownership check — vault access controls entirely bypassed

**Category:** Authentication & Authorization

**Trigger:** After `resolve_secret_refs` posts the resolved config to Caddy, all plaintext secret values live in Caddy's in-memory running configuration. Caddy's admin API (`GET /config/`) returns the full running config as JSON to any HTTP client that can reach the admin socket. The socket is bound to `127.0.0.1` (or a Unix domain socket), so any local process can issue `curl http://127.0.0.1:2019/config/` and receive every plaintext secret Trilithon has ever pushed to Caddy — with no step-up authentication, no Argon2 re-verification, no audit row, and no ownership check. The drift-detection loop itself calls this endpoint on every tick (R9-F01), meaning plaintext is periodically re-materialised in Trilithon's own process without any audit trail.

**Consequence:** The vault's step-up auth, audit trail, and ownership checks are completely bypassed for the Caddy read path. In any environment where multiple OS users share the host (containers, shared servers, CI agents), any process with loopback access obtains all plaintext secrets with a single HTTP GET. ADR-0011's loopback-only binding reduces the network attack surface but does not restrict local OS-level access.

**Design assumption violated:** The design assumes plaintext secrets are accessible only through the vault's reveal endpoint. Caddy's admin API provides a parallel, unauthenticated read path to the same data.

**Suggested mitigation:** Three-part response: (1) Require Caddy admin API authentication — Caddy supports `BasicAuth` on the admin API via `admin.identity`; Trilithon must configure this and use the credential in its own admin API calls. Document this as a required configuration when Trilithon manages secrets. (2) Redesign drift detection per R9-F01 to operate on masked fields only, eliminating `get_running_config()` as a plaintext source on the drift path. (3) Document in ADR-0014 that the Caddy admin socket is a plaintext-read path and that socket permissions must be restricted to the Trilithon daemon's OS user.

---

### R9-F03 — HIGH: `UPDATE is_current = FALSE` succeeds but subsequent `INSERT` fails — no current row remains, breaking all future applies of that route's secrets

**Category:** State Manipulation

**Trigger:** `upsert_secret` executes two statements: (1) `UPDATE secrets_metadata SET is_current = FALSE WHERE owner_kind = ? AND owner_id = ? AND field_path = ? AND is_current = TRUE`, then (2) `INSERT INTO secrets_metadata (...) VALUES (...)`. If the UPDATE commits but the INSERT fails (e.g., `SQLITE_FULL`, I/O error), the table has zero `is_current = TRUE` rows for that `(owner_kind, owner_id, field_path)` tuple. The partial unique index from R8-F04 prevents re-inserting without first restoring a current row — but the failure path in `upsert_secret` is not specified to attempt recovery or rollback the UPDATE. If the R3-F01 savepoint wraps only the outer mutation pipeline and not the UPDATE+INSERT pair inside `upsert_secret`, the UPDATE's side effect persists after the savepoint rolls back.

**Consequence:** Every subsequent `resolve_secret_refs` call for a snapshot referencing this route's secret field returns `NotFound` (no current row). The applier fails on every apply cycle. The route becomes permanently unapplyable until an operator manually runs a recovery query. No structured error distinguishes this state from a legitimately deleted secret, and no recovery path is documented.

**Design assumption violated:** The design implies the UPDATE+INSERT sequence is atomic. This requires an explicit savepoint or transaction boundary covering both statements — which is not stated in the design and is easy to violate at the call-site level.

**Suggested mitigation:** Specify explicitly: "The `UPDATE SET is_current = FALSE` and the `INSERT` for the new row MUST execute within a single SQLite savepoint or transaction inside `upsert_secret`, such that a failure of the INSERT atomically rolls back the UPDATE." This is separate from the R3-F01 outer savepoint — it requires an inner savepoint. Add an acceptance test: inject a post-UPDATE INSERT failure (via a mock storage layer) and assert the old row's `is_current` is restored to `TRUE` after rollback.

---

### R9-F04 — MEDIUM: Admin block injection in `resolve_secret_refs` has no specified merge strategy — `persist: false` may silently overwrite operator-configured admin settings

**Category:** Logic Flaws

**Trigger:** R8-F02 and R8-F03 require that `"admin": {"config": {"persist": false}}` be embedded in every `POST /load` body. The snapshot's `desired_state_json` contains route/upstream config, not admin config. `resolve_secret_refs` must therefore merge the snapshot config with the admin block before posting. No design document specifies: (a) the canonical source of the admin block; (b) which key wins if `desired_state_json` already contains an `"admin"` key; (c) whether the merge is a deep merge or a top-level key overwrite. A top-level overwrite silently discards any admin settings present in the snapshot (listen address, TLS config, the BasicAuth identity from R9-F02's mitigation).

**Consequence:** If a future phase stores operator-configured admin settings in the snapshot under the `"admin"` key, every `resolve_secret_refs` call silently overwrites them with the hardcoded block. Operator admin settings are lost on every apply with no error.

**Suggested mitigation:** Specify: (1) The admin block constant `{"config": {"persist": false}}` is owned by `resolve_secret_refs` and never stored in snapshots. (2) The merge strategy is: `resolved_config["admin"] = deepmerge(resolved_config.get("admin").unwrap_or({}), {"config": {"persist": false}})` — ensuring `persist: false` wins while preserving other admin settings. Add a test asserting that a snapshot with a pre-existing `"admin"` key (e.g., `"listen": "127.0.0.1:2020"`) retains that setting after the merge and that `persist` is always `false`.

---

### R9-F05 — LOW: `SecretsVault::redact` remaining on the trait is a latent trap — future callers will use it for audit writes, silently violating the R5-F04 single-redactor invariant

**Category:** Data Exposure

**Trigger:** R5-F04 established that `DiffEngine::redact_diff` is the single canonical redaction path for audit-log writes and directed that `SecretsVault::redact` MUST NOT be called on any path that writes to `audit_log.redacted_diff_json`. This constraint is documentation-only — `SecretsVault::redact` remains a public method on the trait. A future implementer adding a new audit-producing code path finds `SecretsVault::redact` on the vault they already hold and calls it. The result is an audit row with a different token format than the canonical path produces, silently breaking cross-path token equality.

**Suggested mitigation:** Remove `redact` from the `SecretsVault` trait entirely. Move redaction to a standalone free function `fn redact_secret_value(plaintext: &str) -> RedactedToken` in `core/src/secrets/redact.rs`, exported as the single entry point. Both `DiffEngine::redact_diff` and any future code path call it. The `SecretsVault` trait becomes narrower: `encrypt`, `decrypt`, `rotate_master_key` only.

---

### R9-F06 — LOW: `last_revealed_at` / `last_revealed_by` UPDATE is not explicitly scoped inside the audit transaction — metadata may be permanently stale after a mid-sequence failure

**Category:** Orphaned Data

**Trigger:** The R2-F11 fix wraps the audit row INSERT in a transaction. The `secrets_metadata` columns `last_revealed_at` and `last_revealed_by` are updated on reveal. If this UPDATE executes as a separate statement outside the audit transaction (a natural but incorrect split), a crash or `SQLITE_BUSY` timeout between the transaction commit and the UPDATE leaves these columns at their prior values. The authoritative audit row exists, but `last_revealed_at` is incorrect.

**Consequence:** Operators using `last_revealed_at` for quick-look dashboards (without querying the full audit log) see incorrect last-access times. This is a usability defect, not a security breach, because the audit row is the authoritative record. However, inconsistency between the audit log and `last_revealed_at` can create confusion in forensic workflows.

**Suggested mitigation:** Specify that the `UPDATE secrets_metadata SET last_revealed_at, last_revealed_by` executes within the same SQLite transaction as the `INSERT INTO audit_log` row. The transaction covers: (1) INSERT audit_log row, (2) UPDATE secrets_metadata last_revealed_at/last_revealed_by, (3) COMMIT. Either both commit or neither does.

---

## Non-findings (explicit)

**Probe: Abuse cases / rate limiting** — R1-F10, concurrent mutation conflicts (R4-F01, R5-F01), and rotation contention (R7-F04) cover the relevant surfaces. No new concrete scenario.

**Probe: Resource exhaustion** — No new unbounded allocation. The drift-detection loop concern is a logic flaw (R9-F01), not a resource exhaustion finding.

**Probe: Single points of failure** — Covered across R1-F03, R5-F07, R5-F10. No new gap.

**Probe: Rollback atomicity beyond R9-F03** — R2-F03, R3-F01, and R4-F02 cover the main rollback surfaces. No additional gap found.

**Probe: Eventual consistency** — SQLite is the single store. No multi-store gap.

---

## Summary

**Critical:** 0 · **High:** 3 · **Medium:** 1 · **Low:** 2 · **Non-findings:** 5

**Top concern:** R9-F01 and R9-F02 are closely linked: `resolve_secret_refs` deposits plaintext into Caddy's in-memory config, which Caddy's unauthenticated `GET /config/` returns to any local process — and the drift-detection loop calls exactly this endpoint on every tick, creating both a permanent false-drift cycle and a periodic plaintext re-materialisation with no audit trail. These require a coordinated response: drift comparison on masked fields only + Caddy admin API authentication.

**R9-F03** (UPDATE+INSERT atomicity gap when INSERT fails) is independently HIGH and must be addressed with an explicit inner savepoint requirement and a recovery acceptance test.

**Design-space signal:** After 9 rounds and 87 findings (20 confirmed non-findings), the space is exhausted. No new attack categories produced concrete findings this round beyond the six surfaces identified at the start. All structural weaknesses are now documented.

**Recommend `--final`** after R9-F01, R9-F02, and R9-F03 are incorporated into the Phase 10 design document and ADR-0014.
