---
id: adversarial:phase-6-audit-log:structural-review
category: adversarial
kind: structural
location:
  area: phase-6-audit-log
  multi: true
finding_kind: adversarial-review
phase_introduced: 6
status: open
created_at: 2026-05-09
created_by: code-adversarial-reviewer
severity: mixed
do_not_autofix: false
---

# Phase 6 — Adversarial Review Findings

**Reviewer:** code_adversarial
**Phase:** 6 (Audit Log subsystem — Slices 6.1–6.7)
**Diff base:** `5e0b13f..HEAD`

---

## Findings

---

[CRITICAL] MIGRATION 0006 SCHEMA STRIPS REQUIRED COLUMNS
File: /Users/carter/Coding/Trilithon/core/crates/adapters/migrations/0006_audit_immutable.sql
Lines: 9-25
Description: The `CREATE TABLE IF NOT EXISTS audit_log` statement in migration 0006 defines only 15 of the 17 columns that `record_audit_event` expects. The `prev_hash` (NOT NULL) and `caddy_instance_id` (NOT NULL DEFAULT 'local') columns that are present in migration 0001 are omitted entirely. If 0006 runs on a blank database without 0001 having run first — any tooling that applies a single migration, runs a subset, or bootstraps from the 0006 file directly — the `INSERT` in `record_audit_event` will bind values for columns that do not exist and fail immediately. The comment "should never happen in practice" is not a guard. The idempotency test runs all migrations from 0001 forward using `:memory:`, so it never exercises this path.
Technique: Assumption Violation
Trigger: Any migration runner that does not enforce ordering, or any test that applies 0006 in isolation.
Suggestion: Remove the `CREATE TABLE IF NOT EXISTS audit_log` block from 0006 entirely, or add both `prev_hash` and `caddy_instance_id` columns to match the 0001 schema exactly. The triggers are the only new artifact; they should stand alone.

---

[HIGH] ACTOR_KIND AND OUTCOME CANONICAL-JSON REPRESENTATION DIVERGES FROM SQL STORAGE
File: /Users/carter/Coding/Trilithon/core/crates/core/src/storage/helpers.rs
Lines: 26-52
Description: `canonical_json_for_audit_hash` serialises `actor_kind` using `format!("{:?}", row.actor_kind).to_lowercase()` and `outcome` using `format!("{:?}", row.outcome).to_lowercase()`. These rely on the derived `Debug` representation. The SQLite adapter's `actor_kind_str` and `outcome_str` const functions produce the same strings today (`user`/`token`/`system`, `ok`/`error`/`denied`). They happen to match only because the enum variant names are single words. If a new `ActorKind` variant is added whose `Debug` output does not exactly match what `actor_kind_str` returns (for example, a variant named `ServiceAccount` where the string stored in SQL is `service_account`), the canonical JSON for hash computation would differ from the string stored in the database. Existing rows would still verify because they used the old code; new rows would produce a hash-chain break that only manifests during chain verification, not during insert. The failure would be silent and detected only by a hash-chain audit.
Technique: Composition Failure
Trigger: Adding an `ActorKind` or `AuditOutcome` variant where the Debug repr does not match the SQL storage string.
Suggestion: Replace the `format!("{:?}", ...)` calls with the same `actor_kind_str` / `outcome_str` helpers used by the SQLite adapter, or define a shared `to_audit_string(&self) -> &'static str` method on each enum and call it from both the canonical JSON builder and the SQLite helper.

---

[HIGH] BYPASS GUARD DOES NOT COVER CLI OR CORE CRATES
File: /Users/carter/Coding/Trilithon/core/crates/adapters/tests/audit_writer_no_bypass.rs
Lines: 72-79
Description: The bypass guard that enforces "all audit writes go through `AuditWriter::record`" scans only `adapters/src/` and `adapters/tests/`. It does not scan `crates/cli/src/` or `crates/core/src/`. The `cli` crate holds `run.rs`, `observability.rs`, and other wiring files that will call adapters directly as Phase 9 and beyond wire HTTP handlers and background tasks. Any code in `cli` that obtains a `Storage` reference and calls `.record_audit_event()` directly bypasses the redactor, the clock, and the ULID generation. There is currently no code doing this, but the guard provides no protection against it.
Technique: Assumption Violation
Trigger: A Phase 9 or later contributor wiring a new HTTP handler or background task in the `cli` crate who calls the storage method directly rather than through `AuditWriter`.
Suggestion: Extend `no_direct_record_audit_event_outside_audit_writer` to also walk `../../cli/src` and `../../core/src` (or alternatively, enforce this via a compile-time wrapper by making `record_audit_event` private on the concrete `SqliteStorage` and accessible only through `AuditWriter`).

---

[HIGH] TLS CORRELATION_ID NOT RESTORED ON PANIC INSIDE POLL
File: /Users/carter/Coding/Trilithon/core/crates/adapters/src/tracing_correlation.rs
Lines: 113-125
Description: `CorrelationSpan::poll` installs the correlation id into `CURRENT_CORRELATION_ID` (thread-local), delegates to the inner future's `poll`, then restores the previous value. If the inner `poll` panics — which tokio catches via `catch_unwind` on worker threads — the restore at line 123 never executes. The thread-local is left holding the `id` from the aborted span. The next future polled on that worker thread will call `current_correlation_id()` and receive the stale id from the crashed span, silently attributing subsequent audit events to the wrong correlation. This is not a hypothetical: tokio worker threads catch panics to isolate task failures, and the panic exits `poll` before the restore runs.
Technique: Cascade
Trigger: A panic anywhere inside a `CorrelationSpan`-wrapped future, including panics inside library code called transitively.
Suggestion: Use a RAII guard (drop-based restore) instead of explicit restore after poll: `struct RestoreGuard(Option<Ulid>); impl Drop for RestoreGuard { fn drop(&mut self) { CURRENT_CORRELATION_ID.with(|c| *c.borrow_mut() = self.0); } }`. This ensures restoration even on unwind.

---

[HIGH] TWO PARALLEL AUDITSELECTOR TYPES WITHOUT A DEFINED CONVERSION
File: /Users/carter/Coding/Trilithon/core/crates/core/src/audit/row.rs (line 123) and /Users/carter/Coding/Trilithon/core/crates/core/src/storage/types.rs (line 187)
Lines: general
Description: Phase 6 ships two structurally different `AuditSelector` types. `storage::types::AuditSelector` (the one the `Storage` trait accepts) has a `kind_glob: Option<String>` field and no `limit` field; `audit::row::AuditSelector` (the one `NormalisedAuditSelector` wraps) has an `event: Option<AuditEvent>` field and a `limit` field but no `kind_glob`. There is no `From`/`Into` implementation converting between them. When Phase 9 HTTP handlers accept an `audit::row::AuditSelector` from the request and need to call `Storage::tail_audit_log`, the developer must manually translate `event` (a typed enum) to `kind_glob` (a string pattern). If that translation is omitted or wrong — for example passing `event.to_string()` where a glob like `"config.*"` is expected — the filter silently returns incorrect rows or ignores the event filter entirely. The `limit` field from `audit::row::AuditSelector` is also orphaned: `tail_audit_log` takes a separate `limit: u32` parameter, and `NormalisedAuditSelector` is never accepted by the `Storage` trait.
Technique: Composition Failure
Trigger: Phase 9 code that accepts an HTTP query parameter mapped to `audit::row::AuditSelector` and passes it to `Storage::tail_audit_log` without a verified conversion.
Suggestion: Define a single `AuditSelector` used at both boundaries, or write and test an explicit `impl From<audit::row::NormalisedAuditSelector> for (storage::types::AuditSelector, u32)` conversion that maps `event` to `kind_glob` via `AuditEvent::kind_str()` and surfaces the `limit`.

---

[WARNING] CORRELATION_LAYER RETURNS IDENTITY STUB — EVENTS WILL USE FALLBACK ULIDS
File: /Users/carter/Coding/Trilithon/core/crates/adapters/src/tracing_correlation.rs
Lines: 161-163
Description: `correlation_layer()` returns `tower::layer::util::Identity::new()` — a no-op. The function is documented as a Phase 9 placeholder. Any Phase 9 code that calls `correlation_layer()` and attaches it to the axum router expecting it to stamp spans with `correlation_id` will silently do nothing. Every audit event written from an HTTP handler in Phase 9 will call `current_correlation_id()`, find no TLS value, emit a `correlation_id.missing` warning, and generate a fresh ULID that has no relationship to the inbound request's `X-Correlation-Id` header. This will produce audit logs where every row has a different, unrelated correlation id, defeating cross-event tracing.
Technique: Abuse Case
Trigger: Phase 9 wiring that attaches `correlation_layer()` to the router assuming it is functional.
Suggestion: Add a compile-time or runtime assertion (e.g., a `#[deprecated = "stub only: implement before Phase 9"]` attribute, or a `todo!()` in the function body) to guarantee the placeholder is not silently used in production. Alternatively, complete the real implementation in this slice since the scaffolding (`CorrelationSpan`, `with_correlation_span`, `correlation_id_from_header`) is already present.

---

[WARNING] SERDE_JSON SERIALIZATION FAILURE STORES STRING "null" AS TEXT INSTEAD OF SQL NULL
File: /Users/carter/Coding/Trilithon/core/crates/adapters/src/audit_writer.rs
Lines: 175
Description: `serde_json::to_string(&redacted).unwrap_or_else(|_| "null".to_owned())` stores the literal four-character string `"null"` in `redacted_diff_json` when serialization fails. The schema declares `redacted_diff_json TEXT` (nullable). A reader that checks `IS NULL` will not match rows where serialization failed; a reader that parses the stored text as JSON will receive `JSON null` rather than an absent diff. The two states (`NULL` column and `"null"` string) are semantically identical in JSON but distinct in SQL, breaking any query that uses `WHERE redacted_diff_json IS NOT NULL` to identify rows with diffs. While `serde_json::to_string` on a `Value` returned from earlier JSON operations cannot fail in practice, the code establishes a precedent for silent corruption.
Technique: Composition Failure
Trigger: Any future code path that produces a `serde_json::Value` with non-string map keys (invalid JSON object) or `f64::NAN`/`INFINITY` values and passes it as a diff.
Suggestion: Remove the `unwrap_or_else` fallback; propagate a new `AuditWriteError::DiffSerialisation` variant and let the caller decide whether to proceed without a diff or abort.

---

[WARNING] REDACTOR SECRET FIELD PATTERNS ARE FIXED-DEPTH ONLY
File: /Users/carter/Coding/Trilithon/core/crates/core/src/schema/secret_fields.rs and /Users/carter/Coding/Trilithon/core/crates/core/src/schema/mod.rs
Lines: general
Description: `segments_match` requires `pattern.len() == path.len()`. The four Tier 1 patterns cover `/auth/basic/users/*/password`, `/forward_auth/secret`, `/headers/*/Authorization`, and `/upstreams/*/auth/api_key`. If a future Caddy JSON structure nests these fields one level deeper (for example, inside a handler array: `/handle/0/auth/basic/users/0/password`), the pattern will not match because the lengths differ. There is no recursive or prefix-anchored matching. A diff payload that contains a secret at an unregistered depth will pass through the redactor untouched and the self-check will not flag it, because the self-check only checks paths registered in the registry.
Technique: Assumption Violation
Trigger: A Caddy JSON structure change or a new Phase adding an event whose diff embeds secrets at a path depth not covered by the current four patterns.
Suggestion: Document the fixed-depth assumption explicitly at the registration site and add a test that verifies a nested variant of each pattern does NOT match, so that future contributors adding nested structures are forced to also add the deeper pattern. Consider adding a `/**` recursive wildcard form with a separate matching function.

---

[WARNING] NOTES AND TARGET_ID FIELDS BYPASS REDACTION ENTIRELY
File: /Users/carter/Coding/Trilithon/core/crates/adapters/src/audit_writer.rs
Lines: 113-114, 193-200
Description: `AuditAppend.notes` (`Option<String>`) and `AuditAppend.target_id` (`Option<String>`) are stored verbatim in the audit log without any redaction. The `diff` field is the only field passed through the `SecretsRedactor`. A caller that includes a secret in the `notes` free-text field (e.g., logging an error message that contains a token) or in `target_id` (e.g., using a raw API key as an entity identifier) will write plaintext into the immutable audit log with no detection. The immutability trigger means this cannot be corrected after the fact.
Technique: Abuse Case
Trigger: Any `AuditWriter::record` call where the caller populates `notes` with error details that include a credential string, or `target_id` with a secret-bearing value.
Suggestion: Add a `notes_max_len` enforcement (truncate or reject overly long notes) and document that `notes` and `target_id` must never contain secret material. Optionally run a simple regex scan for PEM headers, `Bearer`, and `Basic` in these fields before storing.

---

[SUGGESTION] HASH CHAIN DOES NOT SURVIVE ROLLBACK OF CONCURRENT WRITE
File: /Users/carter/Coding/Trilithon/core/crates/adapters/src/sqlite_storage.rs
Lines: 773-864
Description: `record_audit_event` uses `BEGIN IMMEDIATE` to serialise writes and computes `prev_hash` from the last committed row. If the `COMMIT` succeeds for transaction A but is then followed by transaction B failing and rolling back, the chain is unbroken. However, if the pool returns a connection with a stale implicit transaction state (which SQLite should auto-rollback, but depends on the pool's connection teardown), the `SELECT ... LIMIT 1` in the next transaction could theoretically see a different "last row" than expected. This is a defence-in-depth concern rather than a current path to failure, because `BEGIN IMMEDIATE` on SQLite serialises all writers and the pool's connection health is managed by sqlx. The finding is worth noting for any future migration to WAL mode with concurrent writers.
Technique: Assumption Violation
Trigger: Multiple concurrent writers combined with a pool that reuses connections with uncommitted state, or a future switch to a non-serialising isolation mode.
Suggestion: Add a comment to the `BEGIN IMMEDIATE` block explicitly stating the isolation guarantee that the hash-chain correctness depends on, so future changes to pool configuration or isolation level are flagged during review.

---

## Techniques with no findings

No findings:
- None — all four techniques produced findings for this diff.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-13 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | [CRITICAL] Migration 0006 schema strips required columns | OK Fixed | pending | - | 2026-05-13 | F001 - fallback CREATE TABLE removed |
| 2 | [HIGH] actor_kind / outcome canonical JSON uses Debug repr | OK Fixed | pending | - | 2026-05-13 | F007 - shared as_audit_str |
| 3 | [HIGH] Bypass guard does not cover CLI or core crates | SUPERSEDED | dde9dc5 | - | 2026-05-09 | F005 - cli/src/ in scan path |
| 4 | [HIGH] TLS correlation_id not restored on panic | SUPERSEDED | dde9dc5 | - | 2026-05-09 | F004 - CorrelationGuard RAII |
| 5 | [HIGH] Two parallel AuditSelector types | DEFERRED | - | - | - | F006 - Slice 6.2 type-system refactor |
| 6 | [WARNING] correlation_layer returns Identity stub | OK Fixed | pending | - | 2026-05-13 | F010 - opaque impl Layer return type |
| 7 | [WARNING] serde_json serialization failure stores "null" | SUPERSEDED | dde9dc5 | - | 2026-05-09 | F003 - Serialization variant |
| 8 | [WARNING] Redactor secret-field patterns are fixed-depth only | OK Fixed | pending | - | 2026-05-13 | F012 - fixed-depth assumption test |
| 9 | [WARNING] Notes and target_id fields bypass redaction | OK Fixed | pending | - | 2026-05-13 | F011/F030 - contract + length caps |
| 10 | [SUGGESTION] Hash chain rollback under concurrent write | OK Fixed | pending | - | 2026-05-13 | F028 - BEGIN IMMEDIATE invariant comment |
