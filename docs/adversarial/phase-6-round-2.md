# Adversarial Review — Phase 6 — Round 2

## Summary

3 critical · 4 high · 3 medium · 1 low

## Round 1 Closure

| ID | Status | Notes |
|----|--------|-------|
| F001 | Partially closed | Chain defined, but `prev_hash` computation inside `record` while caller owns transaction creates a TOCTOU gap — see F101 |
| F002 | Closed | `RedactedDiff` newtype with private constructor addresses this |
| F003 | Closed | SHA-256 of plaintext UTF-8 bytes with `secret:` prefix is now specified |
| F004 | Closed | Renumbered to `0006` |
| F005 | Closed | `AuditRow` fields now match schema |
| F006 | Closed | Three missing variants added |
| F007 | Partially closed | Tracing span approach specified, but extraction from `Span::current()` in adapters removes `correlation_id` from `AuditRow`, creating silent fallback paths — see F103 |
| F008 | Partially closed | Startup guard added, but `DEFAULT ''` on `prev_hash` means pre-migration rows have empty hash, permanently breaking `verify` — see F102 |
| F009 | Partially closed | Caller-owned transaction documented, but bare `&mut SqliteConnection` signature doesn't enforce it — see F106 |
| F010 | Closed | 10 MB cap with truncation added |
| F011 | Closed | Registry-bound constructor closes this |
| F012 | Closed | `thread_local!` / `static` explicitly prohibited |
| F013 | Closed | `Actor` enum with typed variants defined |

---

## New Findings

### F101 — CRITICAL — prev_hash race: concurrent callers produce duplicate hashes

**Category:** Race condition / composition failure

**Attack:** Two requests finish near-simultaneously and both call `AuditWriter::record` on different `SqliteConnection` handles drawn from the pool. Both execute "query most-recent row's hash" before either has committed their INSERT. Both read the same `prev_hash` value `H_n`. Both then insert a new row with `prev_hash = H_n`. The chain now has two rows at position n+1 with the same `prev_hash`. `chain::verify` will detect a broken link on the second row — but after both rows are durably committed. The startup guard calls `chain::verify` and emits `tracing::error!` but does not prevent startup, so the corrupted chain silently persists in production.

**Why the design doesn't prevent it:** Task 8 says "computes `prev_hash` by querying most-recent row before inserting (within same connection)." SQLite in WAL mode allows concurrent readers, so two connections can both read `H_n` before either writes. No serialisation mechanism is specified — no `EXCLUSIVE` lock, no application-level mutex, no `SERIALIZABLE` isolation assertion. The caller-owned transaction does not help unless the caller also holds a write lock across the read-then-insert gap.

**Mitigation required:** `AuditWriter::record` must acquire an exclusive write-lock around the read-then-insert pair. The correct approach: issue `BEGIN IMMEDIATE` internally as an inner savepoint (or require the caller to hold a `BEGIN IMMEDIATE` transaction). The design must specify this explicitly — leaving it to callers guarantees at least one call site gets it wrong.

---

### F102 — CRITICAL — `DEFAULT ''` on `prev_hash` permanently breaks `chain::verify` for pre-migration rows

**Category:** State manipulation / cascade failure

**Attack:** Migration `0006_audit_immutable.sql` adds `prev_hash TEXT NOT NULL DEFAULT ''`. Any row already in `audit_log` at migration time gets `prev_hash = ''`. The all-zero 64-char sentinel applies only to the first newly inserted row. When `chain::verify` walks rows in order, it hashes each row's canonical JSON to compute what the *next* row's `prev_hash` should be — but pre-migration rows have `prev_hash = ''`, not the SHA-256 of the previous row. `verify` will flag every pre-migration row as a broken link. The startup guard emits `tracing::error!` but allows startup, so this fires on every startup of every deployment that ran migrations on an existing DB, forever — with no way to distinguish "chain was attacked" from "chain predates the feature."

**Why the design doesn't prevent it:** Task 6 defines the all-zero sentinel for the first row and `chain::verify` as a simple slice walk, but says nothing about the pre-migration row population. No migration-epoch marker is specified.

**Mitigation required:** The migration MUST backfill `prev_hash` for all existing rows with a deterministic sentinel (the all-zero digest for every pre-existing row). `chain::verify` MUST treat consecutive all-zero `prev_hash` values as a "pre-chain epoch" it skips with a logged note. Document this in the migration comment and in the `chain::verify` doc. Option (b): `chain::verify` accepts an optional epoch cursor — rows before the first non-empty `prev_hash` are skipped entirely.

---

### F103 — CRITICAL — Removing `correlation_id` from `AuditRow` and injecting it from `Span::current()` creates silent fallback for every non-request callsite

**Category:** Assumption violation / architecture violation

**Attack:** Task 10 has `AuditWriter::record` extract `correlation_id` from `tracing::Span::current()` and removes `correlation_id` from the caller-constructed `AuditRow`. Any callsite outside a request handler — daemon startup events, background tasks, startup chain-verify, unit tests — runs outside a span that has a `correlation_id` field. The fallback generates a new ULID with `tracing::warn!`. This means every operational event outside a request context has a random, ungroupable correlation ID. Worse: with `correlation_id` removed from `AuditRow`, there is no compile-time enforcement that it is provided — the silent fallback is the only path, and it fails silently in tests that don't inspect tracing output.

**Why the design doesn't prevent it:** The design removes `correlation_id` as a first-class field on `AuditRow` and replaces it with ambient span extraction. There is no compile-time guarantee that a span with `correlation_id` is present.

**Mitigation required:** Keep `correlation_id: String` as a required field on `AuditRow` (not `Option`). The caller at the `cli`/entry layer injects the correlation ID from the span before constructing `AuditRow`. `record` takes the value from the struct, not from ambient span state. This keeps `core` types self-contained and makes missing correlation IDs a compile error rather than a silent runtime fallback.

---

### F104 — HIGH — Trigger body unspecified; implementation could protect only some columns, leaving `prev_hash` writable

**Category:** State manipulation

**Attack:** Task 12 names the trigger type (`BEFORE UPDATE`, `BEFORE DELETE`) but does not specify the trigger body. If an implementer writes the trigger to fire only on changes to specific columns — e.g. to allow a one-time backfill of `prev_hash` — the trigger would leave `prev_hash` writable after migration. The startup guard checks that the trigger *exists* (via `sqlite_master`) but does not verify the trigger *body*. An attacker with direct DB access could `UPDATE audit_log SET prev_hash = '...'` without error if the trigger allows it.

**Why the design doesn't prevent it:** The trigger body is unspecified. Task 11 asserts presence, not correctness.

**Mitigation required:** Specify the trigger body explicitly: `RAISE(ABORT, 'audit_log rows are immutable')` with no column conditions — any `UPDATE` or `DELETE` on `audit_log` is rejected unconditionally. The startup guard should also assert the trigger body contains `RAISE` (a substring check on the `sqlite_master` SQL column, ~5 lines).

---

### F105 — HIGH — Runtime `AUDIT_KIND_VOCAB` check in `record` silently drops audit events on vocab/enum divergence instead of failing loud

**Category:** Logic flaw

**Attack:** Task 8 validates `row.kind.to_string()` against `AUDIT_KIND_VOCAB` at write time and returns `adapters::Error::Audit(AuditError::UnknownKind)` for unknown kinds. But `AuditEvent` is an enum — its `Display` output is always a valid variant name for any successfully-constructed value. The only scenario where the check fires is if `AUDIT_KIND_VOCAB` is out of sync with the enum (e.g. a new variant added to the enum but not the vocab). When this check fires, the audit event is silently dropped (the caller receives an error, discards it, and the event is never retried). This means a maintenance error causes silent data loss in production.

**Why the design doesn't prevent it:** The design uses runtime vocabulary validation as a substitute for compile-time invariants. The count assertion in task 1 helps but does not prevent the vocab/enum divergence from being caught only at the first write of the new variant.

**Mitigation required:** Replace the runtime check in `record` with a compile-time assertion and a `#[cfg(test)]` exhaustive test that iterates every `AuditEvent` variant and asserts it is present in `AUDIT_KIND_VOCAB`. Remove the runtime check from `record` — it adds latency on every write and catches only what the compile-time check already catches.

---

### F106 — HIGH — `record(conn: &mut SqliteConnection)` signature does not enforce caller-owned transaction at the type level

**Category:** Assumption violation

**Attack:** Task 8 specifies the caller owns the transaction and the test "business transaction rollback leaves no audit row" validates this. But the signature `record(conn: &mut SqliteConnection, row: AuditRow)` accepts a bare connection, not a `Transaction<Sqlite>`. Any caller can call `record` without a surrounding transaction, or in a separate transaction from the business write. A call site that wraps only the audit write in its own transaction will commit the audit row even when the business transaction later rolls back — the inverse of the intended guarantee. Documentation does not prevent future contributors from writing incorrect call sites.

**Why the design doesn't prevent it:** The design relies on convention rather than types for transaction enforcement.

**Mitigation required:** Change the signature to `record(tx: &mut Transaction<'_, Sqlite>, row: AuditRow)`. `sqlx::Transaction` cannot be constructed without `BEGIN`; this makes it impossible to call `record` outside a transaction. The caller must commit the `Transaction` after both the business write and the audit write, enforcing atomicity at the type level.

---

### F107 — HIGH — `RedactedDiff::new` placeholder format has no version tag; Phase 10 vault-hash change creates permanent format split

**Category:** Assumption violation

**Attack:** Task 4 specifies placeholder format `secret:<sha256-of-plaintext-hex>` with `REDACTED_PREFIX = "secret:"`. ADR-0014 anticipates the vault-backed variant in Phase 10. When Phase 10 changes the hash to something derived from the ciphertext (or an HMAC), existing rows have `secret:<sha256>` and new rows have a different format. Log consumers that parse placeholders by prefix cannot distinguish them. Cross-row comparison of secret field values — the primary reason for a stable placeholder — becomes ambiguous. There is no version marker in the format.

**Why the design doesn't prevent it:** Task 4 specifies the format without a version prefix and with no migration path for Phase 10.

**Mitigation required:** Use `secret:v1:<sha256-hex>` as the Phase 6 format. Add `const REDACTION_FORMAT_VERSION: u8 = 1` to `core::audit`. This costs 3 bytes and makes format negotiation possible when Phase 10 arrives without touching existing rows.

---

### F108 — MEDIUM — `chain::verify(rows: &[AuditRow])` loads all rows into memory at startup; will OOM or time out on large deployments

**Category:** Resource exhaustion

**Attack:** Task 6 specifies `chain::verify(rows: &[AuditRow])` takes a full slice. Task 11 calls this at daemon startup. An instance running for months may have tens of thousands of audit rows; each row includes `redacted_diff_json` (potentially kilobytes). Loading all rows into a `Vec<AuditRow>` at startup blocks the daemon from accepting requests until verify completes and may exhaust memory on resource-constrained hardware.

**Why the design doesn't prevent it:** The API takes a slice, implying full in-memory loading. No streaming, batching, or row-count limit is specified.

**Mitigation required:** Change the startup verify to a streaming walk: query rows in batches of N (e.g. 500), verify the chain segment, carry forward only the last hash and row index to the next batch. `chain::verify` should accept an iterator or be called in a paginated loop rather than receiving a pre-loaded slice. Add a configurable `--skip-chain-verify` startup flag for deployments that want fast restarts after a prior clean verification.

---

### F109 — MEDIUM — Fallback-generated correlation IDs are indistinguishable from real request IDs in audit query results

**Category:** Data exposure / logic flaw

**Attack:** Task 10 specifies that if no span with `correlation_id` is present, `record` generates a new ULID and emits `tracing::warn!`. The generated ULID is stored in `audit_log.correlation_id` as a normal value. An operator querying the audit log by correlation ID will get correct results for request-driven events, but background events each have a unique random correlation ID — making them ungroupable and indistinguishable from actual request IDs. The `tracing::warn!` is ephemeral and not queryable.

**Why the design doesn't prevent it:** The fallback ULID is stored identically to a real correlation ID with no distinguishing prefix or flag.

**Mitigation required:** Prefix fallback-generated correlation IDs with `synth:` (e.g. `synth:01J...`). This makes them queryable (`WHERE correlation_id LIKE 'synth:%'`) and distinguishable. Alternatively, treat a missing correlation ID as a hard error (return `Err`) rather than fabricating one — forcing all call sites to be explicit.

---

### F110 — LOW — `occurred_at` and `occurred_at_ms` have no schema or type-level enforcement of their relationship

**Category:** Logic flaw

**Attack:** `AuditRow` has `occurred_at: i64` (Unix seconds) and `occurred_at_ms: i64` (Unix milliseconds). Nothing in the type, `record`, or the schema enforces that `occurred_at_ms / 1000 == occurred_at`. A caller that accidentally passes `occurred_at_ms` in microseconds or swaps the two fields writes a row where the timestamps are orders of magnitude apart. `chain::verify` hashes both fields as-is and will not detect the inconsistency.

**Why the design doesn't prevent it:** There is no `CHECK` constraint in the schema and no validation in `record`. The two fields are independent `i64` values linked only by naming convention.

**Mitigation required:** Either (a) add a SQLite `CHECK (occurred_at_ms >= occurred_at * 1000 AND occurred_at_ms < (occurred_at + 1) * 1000)` constraint in a migration, or (b) remove `occurred_at` from `AuditRow` entirely and compute it as `occurred_at_ms / 1000` in the query layer, storing only `occurred_at_ms`. Option (b) eliminates the redundancy and is strictly safer.
