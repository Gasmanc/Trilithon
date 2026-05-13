# Phase 6 — Aggregate Review Plan

**Generated:** 2026-05-13
**Reviewers:** adversarial, codex, gemini (failed), glm, kimi, minimax, qwen, scope_guardian, security
**Raw findings:** 41 across 8 reviewers (gemini-cli crashed; no findings)
**Unique findings:** 32 after clustering
**Consensus:** 0 unanimous · 2 majority (5+ reviewers) · 10 partial (2–4) · 20 single
**Conflicts:** 0
**Superseded (already fixed):** 0

---

## How to use this document

Feed this file to `/review-remediate` to drive the fix cycle. Each finding has a
unique ID (F001, F002, …) that `review-remediate` uses to track state. Do not
renumber or delete findings — append `SUPERSEDED` status instead.

---

## CRITICAL Findings

### F001 · [CRITICAL] Migration 0006 fallback CREATE TABLE strips required columns
**Consensus:** PARTIAL (2/8) · flagged by: adversarial (CRITICAL), codex (HIGH)
**File:** `core/crates/adapters/migrations/0006_audit_immutable.sql` · **Lines:** 9–25
**Description:** The `CREATE TABLE IF NOT EXISTS audit_log` block in migration 0006 defines only 15 of the 17 columns that `record_audit_event` writes — it omits `prev_hash` (NOT NULL) and `caddy_instance_id` (NOT NULL DEFAULT 'local'), both present in 0001. If 0006 is ever applied to a blank database without 0001 (single-migration tooling, partial application, bootstrap-from-0006), every subsequent `INSERT` fails. The idempotency test runs all migrations in sequence from `:memory:`, so this path is never exercised.
**Suggestion:** Remove the `CREATE TABLE IF NOT EXISTS audit_log` block from 0006 — the triggers are the only new artefact and should stand alone. Alternatively, make the fallback byte-for-byte compatible with 0001 (include `prev_hash` and `caddy_instance_id` with defaults).
**Claude's assessment:** Agree. The fallback is defence-in-depth that becomes a footgun under any non-sequential migration application. Removing the `CREATE TABLE` block is the right call; the triggers reference `audit_log` and will fail loudly if the table doesn't exist, which is the correct behaviour.

---

### F002 · [CRITICAL] Diff redaction bypasses redact_diff envelope shape
**Consensus:** SINGLE · flagged by: codex
**File:** `core/crates/adapters/src/audit_writer.rs` · **Lines:** 147–149, 173–175
**Description:** `AuditWriter` calls `SecretsRedactor::redact` on the entire `diff` value. For the documented diff envelope shape `{added, removed, modified}`, secret paths sit under top-level keys (`added`, `removed`, `modified`) and no longer match schema pointers, so plaintext secrets land in `redacted_diff_json` unredacted.
**Suggestion:** Use `SecretsRedactor::redact_diff` (which already understands the envelope), or detect the envelope shape explicitly and walk each key. Add an integration test that proves secrets under `added`/`modified` are redacted.
**Claude's assessment:** Agree, high-priority security correctness gap. Needs verification: confirm whether `redact_diff` exists on `SecretsRedactor` and whether the audit pipeline actually produces envelope-shaped diffs. If yes, this is a confirmed plaintext-leak path.

---

### F003 · [CRITICAL] Silent `"null"` fallback on serde_json::to_string of redacted diff
**Consensus:** MAJORITY (5/8) · flagged by: qwen (CRITICAL), security (HIGH), adversarial (WARNING), minimax (WARNING), codex (SUGGESTION)
**File:** `core/crates/adapters/src/audit_writer.rs` · **Lines:** 162, 173–177
**Description:** `serde_json::to_string(&redacted).unwrap_or_else(|_| "null".to_owned())` stores the 4-character literal `"null"` when serialization fails. The schema has `redacted_diff_json TEXT` (nullable). Two consequences: (a) consumers using `IS NULL` cannot detect failures, and (b) consumers parsing as JSON receive `JSON null` instead of the actual diff. Because audit rows are immutable, this corruption is permanent. The fallback also bypasses any error variant — `AuditWriteError` has no `Serialization` case.
**Suggestion:** Add `AuditWriteError::Serialization(serde_json::Error)` and propagate. At minimum emit `tracing::error!` so the corruption is observable. Consider failing-fast with `serde_json::to_value` pre-serialisation.
**Claude's assessment:** Agree, and the consensus is decisive. Five reviewers independently flagged the same line — this is a clear zero-debt violation (silent error swallowing in a production path that writes immutable records).

---

## HIGH Findings

### F004 · [HIGH] TLS correlation_id not restored on panic inside CorrelationSpan::poll
**Consensus:** PARTIAL (3/8) · flagged by: adversarial, kimi, qwen
**File:** `core/crates/adapters/src/tracing_correlation.rs` · **Lines:** ~113–135
**Description:** `CorrelationSpan::poll` installs `correlation_id` into the `CURRENT_CORRELATION_ID` thread-local, polls the inner future, then restores. If the inner future panics, the restore line never executes. tokio catches panics on worker threads, so the next future polled on that worker sees the stale id and silently attributes its audit events to the wrong correlation.
**Suggestion:** Use an RAII drop-guard: `struct RestoreGuard(Option<Ulid>); impl Drop for RestoreGuard { fn drop(&mut self) { CURRENT_CORRELATION_ID.with(|c| *c.borrow_mut() = self.0); } }`. Restoration runs on unwind as well as normal return.
**Claude's assessment:** Agree, clean RAII fix. Three independent reviewers spotted the same omission — the drop-guard pattern is the textbook fix and adds minimal complexity.

---

### F005 · [HIGH] Bypass guard does not cover `cli/` or `core/` crates
**Consensus:** PARTIAL (2/8) · flagged by: adversarial, qwen
**File:** `core/crates/adapters/tests/audit_writer_no_bypass.rs` · **Lines:** 72–79
**Description:** `no_direct_record_audit_event_outside_audit_writer` scans only `adapters/src/` and `adapters/tests/`. The `cli` crate (Phase 9 HTTP handlers, background tasks) and `core` crate are uncovered. A contributor obtaining a `Storage` reference and calling `.record_audit_event()` directly from `cli` bypasses the redactor, the clock, and ULID generation — and the guard does not fire.
**Suggestion:** Extend the walk to `../../cli/src` and `../../core/src` (or do a workspace-wide grep). A stronger option is to make `record_audit_event` private on `SqliteStorage` and reachable only via `AuditWriter` — compile-time enforcement.
**Claude's assessment:** Agree. Test-time enforcement is appropriate now; compile-time enforcement via visibility is the more durable answer once Phase 9 wires HTTP handlers. Fix in two steps.

---

### F006 · [HIGH] Slice 6.2 types (`audit::row::*`) not wired into adapter path — dead duplicate hierarchy
**Consensus:** MAJORITY (4/8) · flagged by: adversarial, glm, qwen, scope_guardian (×2 findings)
**File:** `core/crates/core/src/audit/row.rs` and `core/crates/core/src/storage/types.rs` · **Lines:** row.rs:85,123; types.rs:136,187
**Description:** Slice 6.2 specifies `AuditEventRow`, `AuditSelector`, `ActorRef`, `AuditOutcome` as the canonical wire types between core and Storage. The diff adds them in `core::audit::row`, but every production adapter (`audit_writer.rs`, `sqlite_storage.rs`, `storage_sqlite/audit.rs`) still uses the pre-existing `storage::types::*` versions, which diverge structurally:
- `audit::row::AuditEventRow` uses `actor: ActorRef` (enum); `storage::types::AuditEventRow` uses flat `actor_kind` + `actor_id` strings plus `prev_hash`/`caddy_instance_id`.
- `audit::row::AuditSelector` has `event: Option<AuditEvent>` + `limit`; `storage::types::AuditSelector` has `kind_glob: Option<String>` and no `limit`.
- `audit_writer.rs` redeclares its own `ActorRef` enum at the adapter boundary with a comment explicitly acknowledging the duplication.
- No `From`/`Into` exists between the parallel selector types. Phase 9 HTTP handlers will need a manual translation; `event.to_string()` vs `kind_glob` glob shape is a silent-no-op trap.

**Suggestion:** Wire `core::audit::row::*` as the single canonical type through `Storage::record_audit_event` and `Storage::tail_audit_log`. Migrate `audit_writer.rs` and `sqlite_storage.rs` to consume the spec types. Delete the redeclared `ActorRef` in adapters; re-export via `pub use trilithon_core::audit::ActorRef`. If `prev_hash`/`caddy_instance_id` must persist on the row, separate them from the spec row (e.g., an internal chain-metadata struct).
**Claude's assessment:** Agree, and this is the most structurally important finding in the phase. Slice 6.2's exit was "types exist"; the spec intent was "types are the wire surface". The current arrangement guarantees future drift — the two type sets will be maintained independently and Phase 9 will pick the wrong one. Fix before any downstream phase consumes these types.

---

### F007 · [HIGH] actor_kind/outcome canonical JSON uses `Debug` repr instead of shared string helper
**Consensus:** SINGLE · flagged by: adversarial
**File:** `core/crates/core/src/storage/helpers.rs` · **Lines:** 26–52
**Description:** `canonical_json_for_audit_hash` serialises `actor_kind` and `outcome` via `format!("{:?}", ...).to_lowercase()`. This happens to match the SQLite adapter's `actor_kind_str`/`outcome_str` today because variant names are single words. Adding a multi-word variant (e.g., `ServiceAccount` → SQL `service_account`) breaks the hash chain silently: the canonical JSON used for hashing diverges from the string stored in SQL, producing a chain break that only manifests at verification time.
**Suggestion:** Define a shared `to_audit_string(&self) -> &'static str` method on each enum (or replace `format!("{:?}")` with calls to `actor_kind_str`/`outcome_str`). Adopt the shared helper at both call sites.
**Claude's assessment:** Agree. Cheap to fix, expensive to discover later. The `Debug` reliance is a classic foot-gun.

---

### F008 · [HIGH] RFC 6901 JSON Pointer decoding has incorrect order (~1~0 → `/~~` instead of `/~`)
**Consensus:** SINGLE · flagged by: kimi
**File:** `core/crates/core/src/schema/mod.rs` · **Lines:** 96–103
**Description:** `decoded_segments` runs `seg.replace("~1", "/").replace("~0", "~")` sequentially. For encoded segments like `~1~0` (which RFC 6901 decodes to `/~`), the naive replacement order produces `/~~`. This breaks `SchemaRegistry::is_secret_field` for any path whose segments legitimately contain `~` or `/`. A field configured at such a path will not be recognised as secret and will leak unredacted into the audit log.
**Suggestion:** Replace the two `.replace` calls with a single left-to-right scan: emit `~` on `~0`, `/` on `~1`, pass other characters through verbatim.
**Claude's assessment:** Agree, textbook decoding bug. Add a unit test covering `~0~1`, `~1~0`, and double-tilde inputs.

---

### F009 · [HIGH] No production `CiphertextHasher` implementation exists outside tests
**Consensus:** SINGLE · flagged by: security
**File:** `core/crates/core/src/audit/redactor.rs`, `core/crates/adapters/src/audit_writer.rs` · **Lines:** redactor.rs:24–28; audit_writer.rs:140–157
**Description:** `CiphertextHasher` is a public trait with three implementations — all three live in `#[cfg(test)]` blocks (`Sha256Hasher`, `PlaintextHasher`, `ZeroHasher`). When Phase 9 wires this up, a developer could unknowingly supply a low-quality or constant hasher with no compile-time guard. The `ZeroHasher` (always outputs zero) is already used in adapter integration tests and shows the failure mode: every redacted field becomes `***000000000000`.
**Suggestion:** Provide a concrete `Sha256AuditHasher` in `adapters` (not in tests) wrapping `sha2::Sha256`. Either gate `AuditWriter::new` to accept only that type, or assert at construction that the hasher is not a constant. Document the security contract on the trait.
**Claude's assessment:** Agree. The test-only hashers should be `#[cfg(test)]`-gated already (which is what `ZeroHasher` is doing), but a production hasher needs to exist before any production caller can wire `AuditWriter`. Closely tied to Phase 9 readiness.

---

## WARNING Findings

### F010 · [WARNING] `correlation_layer()` returns Identity stub — Phase 9 callers silently no-op
**Consensus:** PARTIAL (3/8) · flagged by: adversarial, glm, qwen
**File:** `core/crates/adapters/src/tracing_correlation.rs` · **Lines:** 161–163
**Description:** `correlation_layer()` currently returns `tower::layer::util::Identity::new()` — a no-op. Phase 9 wiring that attaches it to the router will silently do nothing: every audit event from an HTTP handler calls `current_correlation_id()`, finds no TLS value, emits `correlation_id.missing`, and generates a fresh unrelated ULID. The named return type also makes any future change to `CorrelationIdLayer<S>` a breaking API change for callers that name it.
**Suggestion:** Either (a) complete the implementation in this slice (the scaffolding `CorrelationSpan`, `with_correlation_span`, `correlation_id_from_header` is already present), or (b) mark the placeholder with `#[deprecated]`/`todo!()` and change the return type to `impl Layer<...>` so the opaque type can be swapped later without breaking callers.
**Claude's assessment:** Agree. Returning `impl Layer<...>` is essentially free and prevents the Phase 9 API break. Completing the layer this slice is preferable if the time budget allows.

---

### F011 · [WARNING] `notes` and `target_id` fields bypass redaction entirely
**Consensus:** PARTIAL (2/8) · flagged by: adversarial, qwen
**File:** `core/crates/adapters/src/audit_writer.rs` · **Lines:** 113–114, 193–200
**Description:** `AuditAppend.notes: Option<String>` and `AuditAppend.target_id: Option<String>` go to the row verbatim. The `diff` field is the only one passed through `SecretsRedactor`. A caller stuffing an error message into `notes` that contains a Bearer token, or using a secret-bearing string as `target_id`, writes plaintext into the immutable log.
**Suggestion:** Document the caller contract clearly in `AuditAppend`. Optionally run a coarse regex scan for `Bearer `, `Basic `, PEM headers, etc. before storing and either reject or redact. Add a length cap (see F030).
**Claude's assessment:** Agree, document the contract first; a regex pre-filter is reasonable defence-in-depth but should not replace caller discipline.

---

### F012 · [WARNING] Secret-field patterns require exact path-length match (no recursive matching)
**Consensus:** PARTIAL (2/8) · flagged by: adversarial, qwen
**File:** `core/crates/core/src/schema/secret_fields.rs`, `core/crates/core/src/schema/mod.rs`
**Description:** `segments_match` requires `pattern.len() == path.len()`. The four Tier 1 patterns cover specific fixed depths. If Caddy nests a known secret one level deeper (e.g., inside a handler array: `/handle/0/auth/basic/users/0/password`), the pattern misses and the redactor passes plaintext through. The self-check only verifies registered paths.
**Suggestion:** Document the fixed-depth assumption at the registration site. Add a test that a nested variant of each pattern does NOT match (so future contributors must explicitly add the deeper pattern). Consider a `/**` recursive-wildcard form with a separate matcher.
**Claude's assessment:** Agree, primarily about making the assumption visible. The current behaviour is correct given the current Caddy structure; the fix protects against future drift.

---

### F013 · [WARNING] Hash-prefix oracle — 12-char hex prefix leaks 48 bits of SHA-256
**Consensus:** SINGLE · flagged by: security
**File:** `core/crates/core/src/audit/redactor.rs` · **Lines:** 14–17, 161–167
**Description:** Redacted strings render as `***<12-char-lowercase-hex>` — 48 bits of SHA-256. For low-entropy secrets (PINs, short numeric API keys, fixed-format tokens), this prefix is enough to brute-force the original offline. SHA-256 has no work factor and no salt. An attacker with read access to the audit log can enumerate candidates.
**Suggestion:** For password-class fields use HMAC-SHA256 with a deployment-specific server-side key (gives stable correlation without an offline oracle), or Argon2id/bcrypt if stable correlation is not required. Document the low-entropy caveat prominently on `CiphertextHasher`.
**Claude's assessment:** Agree, this is the most subtle of the security findings. The HMAC-with-server-key approach gives the operational property the codebase wants (deterministic, stable across rows) without the brute-force weakness. Pairs naturally with F009 (concrete production hasher).

---

### F014 · [WARNING] correlation_id accepted from untrusted `X-Correlation-Id` header without validation
**Consensus:** SINGLE · flagged by: security
**File:** `core/crates/adapters/src/tracing_correlation.rs` · **Lines:** 135–145
**Description:** `correlation_id_from_header` accepts any valid-ULID `X-Correlation-Id` and binds it to the request span and audit rows. A malicious client can set the header to a ULID they previously observed in an audit log (or response header), chaining their request to a legitimate prior event and confusing forensic reconstruction.
**Suggestion:** Accept the header only from trusted internal services (mTLS-verified or bearer-authenticated). For external HTTP, always generate a fresh ULID server-side; echo the client value in a separate `X-External-Correlation-Id` for cross-system tracing.
**Claude's assessment:** Agree. The Phase 9 surface area that wires this matters — for now, document the threat model and add the trust-boundary check before Phase 9 exposes HTTP handlers.

---

### F015 · [WARNING] Immutability triggers bypassable via writable_schema or direct WAL access
**Consensus:** SINGLE · flagged by: security
**File:** `core/crates/adapters/migrations/0006_audit_immutable.sql` · **Lines:** 31–42
**Description:** `BEFORE UPDATE`/`BEFORE DELETE` triggers are application-layer guards, not a security boundary. Anyone with write access to the DB file can use `PRAGMA writable_schema = ON;` to disable triggers, or manipulate the WAL journal to remove rows. No read-time hash-chain verification exists in this diff to detect such tampering.
**Suggestion:** Add a `verify_audit_chain` function that walks the chain and re-computes each row's `prev_hash` against the canonical JSON of the prior row. Expose it as a periodic health check or operator CLI. Document the filesystem-access caveat in the threat model.
**Claude's assessment:** Agree. The hash chain is the right defence; surfacing it as an operator-callable verifier closes the loop. This is also infrastructure Phase 9 / Phase 10 will need.

---

### F016 · [WARNING] Read-side kind validation breaks on binary rollback
**Consensus:** SINGLE · flagged by: glm
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** ~537
**Description:** `audit_row_from_sqlite` rejects any row whose `kind` is not in the current `AUDIT_KINDS` list. If a future phase adds a kind, deploys, writes rows, then rolls back the binary, those rows become unreadable. The audit log is meant to be immutable and durable across rollbacks; this gate makes reads version-sensitive.
**Suggestion:** Remove kind validation from the read path (insert-time validation via `validate_kind` is sufficient). Alternatively, return the row with the unknown kind intact rather than erroring — treat unknown-kind as "future variant, surface as-is".
**Claude's assessment:** Agree. Read-path validation of a vocabulary that is allowed to grow is the wrong place to enforce the contract.

---

### F017 · [WARNING] `phase_6_fixed.md` has multiple YAML frontmatter blocks
**Consensus:** PARTIAL (2/8) · flagged by: glm, kimi
**File:** `docs/In_Flight_Reviews/Fixed/phase_6_fixed.md` · **Lines:** 1–63
**Description:** The file contains three consecutive `---`-delimited YAML frontmatter blocks. This violates the F0 schema (one finding per file) and will break `xtask audit-finding-schema`.
**Suggestion:** Split each finding into its own file per the one-finding-per-file rule, or merge into one canonical frontmatter block.
**Claude's assessment:** Agree, docs cleanup. Split into per-finding files to match the F0 invariant.

---

### F018 · [WARNING] Migration named `0006` but TODO specifies `0003`
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/adapters/migrations/0006_audit_immutable.sql`
**Description:** Slice 6.4 specifies `0003_audit_immutable.sql` (and "schema version 2 → 3"). The delivered file is `0006_audit_immutable.sql` — likely a pragmatic adjustment for numbering collisions with 0003–0005 that pre-existed, but undocumented. TODO and architecture cross-references still cite the wrong number.
**Suggestion:** Update the TODO and architecture cross-references to `0006_audit_immutable.sql` and version 6, or add a comment in the migration file explaining the renaming so spec/impl drift is visible.
**Claude's assessment:** Agree. Update the docs — the migration filename is correct given the pre-existing numbering.

---

### F019 · [WARNING] `AUDIT_KIND_REGEX` defined but unused — pattern duplicated inline in adapter
**Consensus:** PARTIAL (4/8) · flagged by: codex (WARNING), scope_guardian (WARNING), glm (SUGGESTION), qwen (SUGGESTION)
**File:** `core/crates/core/src/audit/event.rs`, `core/crates/adapters/src/storage_sqlite/audit.rs` · **Lines:** event.rs:~248–265; audit.rs:15–43
**Description:** `AUDIT_KIND_REGEX` is exported from core but referenced only in unit tests in `event.rs`. The storage-side `validate_kind_pattern` reimplements the dotted-kind segment check manually (with a comment "avoids a `regex` dependency in adapters"). The two implementations must be kept in sync by hand.
**Suggestion:** Export a shared pure function `validate_audit_kind_pattern(kind: &str) -> bool` in core that both call. If keeping the manual implementation in adapters is necessary, add a unit test that asserts behavioural parity against the regex.
**Claude's assessment:** Agree. A shared pure function is cleaner than divergent regex/manual implementations. The "avoids regex dep in adapters" rationale doesn't apply if the helper lives in core.

---

### F020 · [WARNING] `AuditEvent` enum contains variants beyond Tier 1 spec (44 vs 22)
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/core/src/audit/event.rs` · **Lines:** 86–125
**Description:** Slice 6.1 specifies a closed Tier 1 set of 22 variants across 5 groups. The delivered enum has 44 variants, adding `AuthBootstrapCredentialsCreated`, `DriftDeferred`, `DriftAutoDeferred`, `ConfigRebased`, `MutationRebased*`, plus new groups (Policy Presets, Import/Export, Tool Gateway, Docker, Proposals). `#[non_exhaustive]` exists precisely so later phases extend the vocabulary alongside the emit sites.
**Suggestion:** Revert to the 22 Tier 1 variants specified by 6.1. Defer additional vocabulary to the phases that emit those events, so the vocabulary and the emitting code land together.
**Claude's assessment:** Partially agree — the scope point is valid (each extra variant is dead vocabulary until something emits it). However, removing variants that downstream phase TODOs already plan to emit may just churn the file. Recommend: keep only the 22 Tier 1 variants in this slice; track the additional ones via a "deferred vocabulary" appendix that downstream phases consult when wiring their emit sites.

---

### F021 · [WARNING] Slice 6.7 exit unmet — `correlation_layer` not registered in CLI
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/crates/cli/src/main.rs`
**Description:** Slice 6.7 specifies: "core/crates/cli/src/main.rs — register the layer at subscriber init." The module ships but `main.rs` contains zero references to `correlation_layer`, `with_correlation_span`, or `tracing_correlation`. Slice 6.7 also says background loops call `with_correlation_span(Ulid::new(), "system", component_name, fut)` once per iteration — not wired into the daemon entry point. The `correlation_layer()` no-op (F010) makes registration a no-op anyway, but the wrapping for background tasks IS needed now.
**Suggestion:** Add `with_correlation_span(Ulid::new(), "system", "daemon", ...)` wrappers around background task entry points in `cli/src/run.rs` (or equivalent) per slice 6.7 step 4. When F010 is fixed, also register the real layer at subscriber init.
**Claude's assessment:** Agree. The background-task path needs the span wrapping now; the layer registration follows F010.

---

### F022 · [WARNING] Phase exit checklist — `core/README.md` not updated
**Consensus:** SINGLE · flagged by: scope_guardian
**File:** `core/README.md`
**Description:** Phase exit checklist requires `core/README.md` to record the audit pipeline and redactor invariant, citing ADR-0009. The diff contains no change to that file. The audit pipeline, `AuditWriter` single-path invariant, and `SecretsRedactor` guarantee are unrecorded in operator-visible docs.
**Suggestion:** Add a section to `core/README.md` describing the audit pipeline (`AuditWriter` is the only write path; `SecretsRedactor` gates every diff; triggers enforce immutability), citing ADR-0009.
**Claude's assessment:** Agree, straightforward docs fix.

---

### F023 · [WARNING] Incomplete secret-field coverage — TLS private keys, mTLS keys, bearer tokens not covered
**Consensus:** SINGLE · flagged by: security
**File:** `core/crates/core/src/schema/secret_fields.rs` · **Lines:** 11–16
**Description:** `TIER_1_SECRET_FIELDS` covers four paths: `password`, `forward_auth/secret`, `Authorization`, `api_key`. The codebase already references `mtls_key_path` (config/types.rs:60,248,300,419). Candidates not covered: `/tls/*/private_key`, `/upstreams/*/auth/token`, `/upstreams/*/auth/bearer`, JWT fields. A diff embedding a PEM key value, a bearer token, or a JWT under any unregistered name passes the redactor unredacted.
**Suggestion:** Audit Caddy JSON schema fields that can carry secret material and add them. Make this a living registry with a documented review gate when new upstream auth schemes are added.
**Claude's assessment:** Agree. Pair with F012 — F012 makes the depth assumption visible; F023 expands the breadth. Both should be addressed together.

---

### F024 · [WARNING] In-memory cursor pagination order drift from SQLite
**Consensus:** SINGLE · flagged by: codex
**File:** `core/crates/core/src/storage/in_memory.rs` · **Lines:** 235–244
**Description:** Cursor filtering uses `row.id < cursor_before`, but result ordering is insertion order (`.rev()`) rather than `id DESC` like SQLite. When insertion order and ULID order diverge (e.g., concurrent writers, clock skew), in-memory tests can pass while production skips or duplicates rows.
**Suggestion:** Sort filtered rows by `id` descending before `take(limit)` to match `SqliteStorage::tail_audit_log` semantics.
**Claude's assessment:** Agree. Insertion order is almost always the wrong invariant; matching SQLite's `ORDER BY id DESC` is the canonical fix.

---

### F025 · [WARNING] `AUDIT_KIND_VOCAB` cardinality not asserted against enum variants
**Consensus:** SINGLE · flagged by: codex
**File:** `core/crates/core/src/audit/event.rs` · **Lines:** 110–112
**Description:** `AuditEvent::kind_str()` is exhaustive, but no compile-time or test-level assertion verifies that `all_variants().len() == AUDIT_KIND_VOCAB.len()`. If they drift, tests may still pass if they don't exercise the specific variant.
**Suggestion:** Add a test that asserts `all_variants().len() == AUDIT_KIND_VOCAB.len()` and that each variant's `kind_str()` appears in `AUDIT_KIND_VOCAB`.
**Claude's assessment:** Agree, low-cost invariant test.

---

### F026 · [WARNING] correlation_id not cross-referenced against current_correlation_id at write
**Consensus:** SINGLE · flagged by: codex
**File:** `core/crates/adapters/src/audit_writer.rs` · **Lines:** 155–170
**Description:** `AuditWriter::record` accepts `correlation_id: Ulid` from `AuditAppend` and writes it directly, never cross-checking `current_correlation_id()`. A caller could pass a correlation id different from the active tracing span's, producing an audit trail that doesn't match the request lifecycle.
**Suggestion:** Either assert/log-warn when `append.correlation_id != current_correlation_id()`, or document that the caller is responsible for the invariant (and add a builder helper that fills it from the TLS).
**Claude's assessment:** Agree, prefer the builder-helper approach: provide an `AuditAppend::with_current_correlation()` constructor that reads the TLS, leaving callers free to override explicitly when needed.

---

### F032 · [WARNING] Bypass-guard allowlist is stem-based and fragile to naming drift
**Consensus:** PARTIAL (2/8) · flagged by: minimax (WARNING), codex (SUGGESTION)
**File:** `core/crates/adapters/tests/audit_writer_no_bypass.rs` · **Lines:** 31–43
**Description:** `ALLOWED_CALL_STEMS` is a manually-curated list of test file stems. Adding a new test file without updating the list fires the guard; a production file with a similar stem could be incorrectly allowed.
**Suggestion:** Express the allowlist in terms of directory location (`tests/` vs `src/`) rather than file-name stem matching, so the invariant is structural.
**Claude's assessment:** Agree, pair with F005 (cli/core coverage). Once the walk covers cli/core and the directory predicate replaces stem matching, the guard becomes structural and resilient.

---

## SUGGESTION / LOW Findings

### F027 · [SUGGESTION] `caddy_instance_id` hardcoded to `"local"` in audit rows
**Consensus:** SINGLE · flagged by: kimi
**File:** `core/crates/adapters/src/audit_writer.rs` · **Lines:** 130
**Description:** `AuditWriter::record` always writes `caddy_instance_id: "local".to_owned()`. In multi-instance deployments there's no way to distinguish which instance produced an event.
**Suggestion:** Accept `caddy_instance_id` as a constructor parameter or read it from config so deployments can set a meaningful identifier.
**Claude's assessment:** Agree, defer to whichever phase introduces multi-instance deployment. Tag this as a known limitation in the meantime.

---

### F028 · [SUGGESTION] Document hash-chain isolation contract on `BEGIN IMMEDIATE`
**Consensus:** SINGLE · flagged by: adversarial
**File:** `core/crates/adapters/src/sqlite_storage.rs` · **Lines:** 773–864
**Description:** `record_audit_event` relies on `BEGIN IMMEDIATE` serialising writers so `prev_hash` reflects the actual last committed row. The correctness contract isn't documented at the call site — a future switch to WAL with concurrent writers or a non-serialising isolation mode could silently break the chain.
**Suggestion:** Add a comment to the `BEGIN IMMEDIATE` block that explicitly names the isolation guarantee the hash-chain correctness depends on, so future pool/isolation changes are flagged in review.
**Claude's assessment:** Agree, free documentation that prevents a real future hazard.

---

### F029 · [SUGGESTION] `redact_diff` lacks depth limit — deep JSON can stack-overflow
**Consensus:** SINGLE · flagged by: security
**File:** `core/crates/core/src/audit/redactor.rs` · **Lines:** 99–117, 123–153
**Description:** `SecretsRedactor::walk` and `self_check` are mutually recursive over arbitrarily deep `serde_json::Value` trees with no depth limit. A 10,000-level nested payload would stack-overflow on the tokio worker, crashing the audit-writing task.
**Suggestion:** Add a `max_depth` parameter, default to a safe value (e.g., 64), and return `RedactorError::DepthExceeded` when exceeded.
**Claude's assessment:** Agree. Cheap to add; protects against accidental and adversarial inputs.

---

### F030 · [SUGGESTION] `notes` and `target_id` have no length bounds
**Consensus:** SINGLE · flagged by: security
**File:** `core/crates/adapters/src/audit_writer.rs` · **Lines:** 103–114
**Description:** `notes` and `target_id` are `Option<String>` with no length validation. An internal caller passing an unbounded string (e.g., full request body in `notes`) inserts a very large row that immutability prevents cleaning up — potentially filling disk over time.
**Suggestion:** Add length caps (e.g., 4 KB for `notes`, 256 bytes for `target_id`) in `AuditWriter::record` before constructing the row, returning `AuditWriteError` on excess.
**Claude's assessment:** Agree, pair with F011 (redaction).

---

### F031 · [SUGGESTION] Unused `http` and `tower` deps in adapters until Phase 9
**Consensus:** SINGLE · flagged by: qwen
**File:** `core/crates/adapters/Cargo.toml` · **Lines:** 24–25
**Description:** `http` and `tower` are pulled into adapters for the `correlation_layer()` stub that currently returns `Identity`. These deps are unused until Phase 9.
**Suggestion:** Gate behind a `feature = ["tracing-middleware"]` flag, or accept as scaffolding debt until Phase 9 wires them.
**Claude's assessment:** Disagree on the feature flag — the additional complexity isn't worth the savings. Accept as scaffolding debt; remove if Phase 9 ends up using a different mechanism.

---

## CONFLICTS (require human decision before fixing)

None.

---

## Out-of-scope / Superseded

| ID | Title | Reason |
|----|-------|--------|
| — | — | No prior remediation history for phase 6 |

---

## Summary statistics

| Severity   | Majority (5+) | Partial (2–4) | Single | Total |
|------------|---------------|---------------|--------|-------|
| CRITICAL   | 1             | 1             | 1      | 3     |
| HIGH       | 0             | 3             | 3      | 6     |
| WARNING    | 0             | 6             | 12     | 18    |
| SUGGESTION | 0             | 0             | 5      | 5     |
| **Total**  | **1**         | **10**        | **21** | **32** |

Note: 8 reviewers configured; gemini-cli crashed on the 7,400-line diff and produced no findings. Counts use the 8-reviewer denominator. "Majority" = 5+ reviewers; "Partial" = 2–4; "Single" = 1.
