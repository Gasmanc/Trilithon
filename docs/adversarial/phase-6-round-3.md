# Adversarial Review — Phase 6 — Round 3

## Summary

3 critical · 2 high · 3 medium · 1 low

## Round 2 Closure

| ID | Status | Notes |
|----|--------|-------|
| F101 | Partially closed | SAVEPOINT within `Transaction<'_, Sqlite>` proposed, but does not serialise across concurrent WAL connections — see F201 |
| F102 | Closed | Migration backfills `0000…0000`; `chain::verify` skips leading empty rows |
| F103 | Closed | `correlation_id` kept on `AuditRow` as required field; call sites supply from span |
| F104 | Closed | Trigger body specified as unconditional `RAISE(ABORT, …)`; startup guard checks `sql` contains `RAISE` |
| F105 | Closed | Compile-time assertion + exhaustive test replaces runtime check |
| F106 | Closed | Signature changed to `&mut Transaction<'_, Sqlite>` |
| F107 | Closed | `secret:v1:` prefix + `REDACTION_FORMAT_VERSION = 1` |
| F108 | Closed | `chain::verify` takes `impl Iterator`, paginated batch 500 |
| F109 | Closed | `synth:` prefix distinguishes fallback IDs |
| F110 | Closed | `occurred_at` derived at bind time; migration 0007 added for DB-layer constraint |

---

## New Findings

### F201 — CRITICAL — SAVEPOINT does not serialise the prev_hash read-insert pair across concurrent WAL connections

**Category:** Race condition / composition failure

**Attack:** Two tokio tasks call `AuditWriter::record` concurrently with separate `Transaction<'_, Sqlite>` handles drawn from the pool. Both transactions begin before either issues `SAVEPOINT audit_write`. In SQLite WAL mode, `BEGIN` (sqlx default: deferred) does not acquire a write lock until the first write. Both tasks issue `SAVEPOINT audit_write`, read the most-recent row's hash (both see row N, both compute `prev_hash = hash(row_N)`), then INSERT a new row with `prev_hash = hash(row_N)`. Two rows at position n+1 now share the same `prev_hash`. `chain::verify` detects a broken link — but both rows are already durably committed and immutable (triggers block UPDATE/DELETE). The corrupted chain persists forever.

`SAVEPOINT` is scoped to a single connection. It has no cross-connection locking semantics in SQLite WAL mode. The SAVEPOINT approach only prevents interleaving on a single connection; it does not help when two distinct pool connections race.

**Why the design doesn't prevent it:** The design says "Issues `SAVEPOINT audit_write` — serialises read-insert pair." This is only true for single-connection serialisation. The design's choice to require `Transaction` from the caller (for business-transaction atomicity) creates a structural conflict with cross-connection write serialisation.

**Mitigation required:** Choose one of two approaches and document the decision: (a) **Dedicated audit connection**: route all `record` calls through a single `Mutex<SqliteConnection>` dedicated to audit writes, opened with `BEGIN IMMEDIATE` per write. Audit writes are then out-of-band from business transactions — document this as the deliberate trade-off and remove the "rollback leaves no row" guarantee; or (b) **Caller holds `BEGIN IMMEDIATE`**: require the caller to have opened their `Transaction` with `sqlx::Connection::begin_immediate()` (not the default `begin()`), so the write lock is held from the transaction start. Document this as a contract on every `record` call site. The SAVEPOINT approach as currently specified must be removed.

---

### F202 — CRITICAL — `ALTER TABLE … ADD CHECK` is not valid SQLite syntax; migration 0007 will fail at deployment

**Category:** Logic flaw / composition failure

**Attack:** The developer writes `0007_audit_occurred_at_check.sql` with content such as:
```sql
ALTER TABLE audit_log ADD CHECK (occurred_at = occurred_at_ms / 1000);
```
SQLite's `ALTER TABLE` supports only `ADD COLUMN`, `RENAME COLUMN`, `RENAME TO`, and (3.35.0+) `DROP COLUMN`. It does not support `ADD CONSTRAINT` or `ADD CHECK`. This migration fails at runtime with a syntax error. sqlx's `MIGRATOR.run(pool)` returns an error, and the daemon refuses to start. Every deployment and CI run fails after `0006` is applied.

If the developer instead implements a full table-rebuild migration, they must work around the immutability triggers (which block DELETE on `audit_log`), and must handle the case where existing rows have `occurred_at != occurred_at_ms / 1000` (which would violate the new CHECK and cause the data-copy step to fail).

**Why the design doesn't prevent it:** The design specifies `0007_audit_occurred_at_check.sql` without acknowledging SQLite's `ALTER TABLE` limitations, and without acknowledging that existing rows may not satisfy the constraint.

**Mitigation required:** Remove migration 0007 entirely. Enforce `occurred_at = occurred_at_ms / 1000` at the application layer only: (a) `record` derives `occurred_at` at bind time — already in the design, (b) add an integration test that confirms `occurred_at = occurred_at_ms / 1000` for every row after a write via `record`, (c) add a startup validation query `SELECT COUNT(*) FROM audit_log WHERE occurred_at != occurred_at_ms / 1000` (cheap, index-scannable) and log an error if any row fails.

---

### F203 — CRITICAL — `chain::verify` skips rows where `prev_hash == ""` but migration backfills `0000…0000`; the skip condition and backfill value are inconsistent, leaving post-migration rows with empty hash undetected

**Category:** Logic flaw / state manipulation

**Attack:** The design specifies two things:
1. Migration 0006 backfills all existing rows with `0000…0000` (64-char all-zero hex).
2. `chain::verify` skips leading rows where `prev_hash == ""` (the DEFAULT value for the column before backfill).

These are inconsistent. After the backfill, no legitimate row has `prev_hash = ""` — all pre-migration rows have `0000…0000`, and all post-migration rows have a real SHA-256 hash. Therefore the skip condition `prev_hash == ""` never fires for any backfilled row. It only fires if a new row is inserted with `prev_hash = ""` — i.e., if `record` fails to set the field. This means a bug in `record` that leaves `prev_hash` empty produces a row that `chain::verify` silently skips rather than flagging as a broken link. The skip logic intended to handle the pre-migration case becomes a permanent bypass for any write-path bug that produces an empty hash.

The correct skip condition should be `prev_hash == "0000…0000"` (the backfill sentinel), not `prev_hash == ""`. `prev_hash = ""` should be treated as an error by `chain::verify`.

**Why the design doesn't prevent it:** The design's task 7 says "skips leading rows where `prev_hash == ''"` but the migration in task 13 backfills with `0000…0000`. The two values are different. The mismatch means either the skip never fires (benign but wasteful) or the skip fires on records it should not (dangerous).

**Mitigation required:** Make the skip condition and backfill value consistent: either (a) skip rows where `prev_hash == "0000…0000"` (the backfill sentinel — this is the correct approach), and treat `prev_hash == ""` as a chain error; or (b) backfill with `""` and skip rows where `prev_hash == ""`. Option (a) is safer: `""` is always an error after migration. Specify this explicitly in both the migration task (13) and the chain-verify task (7).

---

### F204 — HIGH — `Transaction<'_, Sqlite>` in sqlx 0.8 panics or errors when a raw `SAVEPOINT` SQL statement is issued on the same connection it manages

**Category:** Composition failure

**Attack:** sqlx 0.8's SQLite driver tracks transaction nesting depth. `pool.begin().await` at depth 0 issues `BEGIN`; at depth 1 (nested) issues `SAVEPOINT _sqlx_0`. If `record` issues `SAVEPOINT audit_write` via raw SQL while the caller's `Transaction` is active, sqlx's internal depth counter and the SQLite savepoint stack are out of sync. When the `Transaction` is later committed or rolled back, sqlx issues the wrong command (either `COMMIT` when it should release a savepoint, or `ROLLBACK` to a savepoint it does not know exists), producing either a SQLite error or silent incorrect behaviour.

Even if F201 is resolved by removing the SAVEPOINT approach, this finding remains relevant as a warning: do not mix raw savepoint SQL with sqlx-managed transactions on the same connection.

**Why the design doesn't prevent it:** The SAVEPOINT approach is specified without analysis of sqlx's internal transaction-nesting bookkeeping.

**Mitigation required:** If savepoint nesting is needed, use sqlx's nested transaction API (`tx.begin().await`), which correctly increments the depth counter. Better: adopt one of the F201 mitigations (dedicated audit connection or `BEGIN IMMEDIATE` caller contract) and avoid raw SAVEPOINT SQL entirely.

---

### F205 — HIGH — Compile-time `VARIANT_COUNT` assertion requires `AuditEvent::VARIANT_COUNT` to exist, but Rust enums have no built-in variant count; a manually maintained constant defeats the purpose

**Category:** Logic flaw

**Attack:** The design specifies `const _: () = assert!(AuditEvent::VARIANT_COUNT == AUDIT_KIND_VOCAB.len())`. Rust enums do not expose a `VARIANT_COUNT` associated constant. The developer must either: (a) add `const VARIANT_COUNT: usize = 44` manually to `AuditEvent` — in which case the assertion only fires if both `VARIANT_COUNT` and `AUDIT_KIND_VOCAB` are updated, but adding a variant to the enum without updating either constant causes no compile error; or (b) use `strum::EnumCount` to derive it automatically. The design does not specify how `VARIANT_COUNT` is derived. If option (a) is used, the "compile-time" assertion is no stronger than the existing runtime count test.

**Why the design doesn't prevent it:** The design says "compile-time assertion" without specifying the mechanism. A manually maintained constant defeats the purpose.

**Mitigation required:** Use `strum::EnumCount` (already a likely dependency given the exhaustive test proposal) to derive `AuditEvent::COUNT` automatically. The assertion becomes `const _: () = assert!(AuditEvent::COUNT == AUDIT_KIND_VOCAB.len())` and fires whenever a variant is added or removed from the enum without updating `AUDIT_KIND_VOCAB`. Explicitly add `strum = { features = ["derive"] }` to `core/Cargo.toml` and derive `EnumCount` on `AuditEvent`.

---

### F206 — MEDIUM — `synth:` correlation ID warn message does not include the generated value; the audit row is permanently un-correlatable to its warn log line

**Category:** Data exposure / logic flaw

**Attack:** The design specifies: "emit `tracing::warn!('audit: no active span, synthetic correlation_id assigned')`". If the warn message does not include the generated `synth:<ulid>` value, an operator cannot locate the audit row from the warn log line or correlate the warn to a specific row. The audit row has the `synth:` value; the warn does not. They are permanently disconnected.

**Why the design doesn't prevent it:** The warn message content is not specified. An implementer will write it without the synth value unless told explicitly.

**Mitigation required:** Specify the warn message as `tracing::warn!(correlation_id = %synth_id, "audit: no active span, synthetic correlation_id assigned")` — include the generated value as a structured field. This ensures the warn and the audit row share the same identifier and can be joined in log aggregation.

---

### F207 — MEDIUM — `chain::verify` pagination order unspecified; `ORDER BY occurred_at ASC` with duplicate timestamps produces non-deterministic chain validation

**Category:** Logic flaw

**Attack:** The design specifies "paginated loop (batch 500)" without specifying `ORDER BY`. If the query uses `ORDER BY occurred_at ASC` or `ORDER BY id ASC`, rows with identical `occurred_at` or same-millisecond ULIDs may be returned in different orders across runs. `prev_hash` was written based on the row that was physically inserted immediately before — i.e., insertion order (`rowid` order). If verify reads in a different order, it computes `hash(row_{X-1})` against a row that was not actually the predecessor at insert time, producing false chain failures or false positives.

**Why the design doesn't prevent it:** The design does not specify an ORDER BY clause.

**Mitigation required:** `chain::verify` MUST paginate with `ORDER BY rowid ASC`. `record` MUST use `rowid` as the key for "most-recent row" queries (`SELECT prev_hash FROM audit_log ORDER BY rowid DESC LIMIT 1`). Document that `rowid` is the canonical chain ordering and MUST NOT be overridden (do not use `WITHOUT ROWID` on `audit_log`).

---

### F208 — MEDIUM — `SecretFieldRegistry` will be empty at Phase 6 runtime; `SecretsRevealed` events emit unredacted diffs into the immutable audit log before Phase 10

**Category:** State manipulation

**Attack:** Phase 10 (secrets vault) is not implemented. `SecretFieldRegistry` in `core` is populated from a static list, but no Phase 6 task specifies what paths are in that list. The `secrets_metadata` table exists but the field-path registry is not derived from it (which would require I/O, forbidden in `core`). At runtime, if the registry is empty, `RedactedDiff::new` produces a diff with zero redaction sites. A `SecretsRevealed` audit event — the one event that most requires redaction — will contain the plaintext secret value in `redacted_diff_json` in the immutable audit log. The immutability triggers prevent correction.

**Why the design doesn't prevent it:** No Phase 6 task specifies the initial contents of `SecretFieldRegistry`. The corpus test can pass trivially with `registry.len() == 0`. No test exercises the `SecretsRevealed` path with a non-empty registry.

**Mitigation required:** Either (a) block `SecretsRevealed` events at `record` until Phase 10 populates the registry (return `Err(AuditError::RegistryEmpty)` for that variant), or (b) hard-code at least the known secret field paths into `SecretFieldRegistry` as a Phase 6 constant (even if incomplete) so the corpus test is non-vacuous. Specify this registry content in the task.

---

### F209 — LOW — `record` returns `Err(AuditError::EmptyCorrelationId)` for an empty correlation ID, but this causes the audit event to be silently dropped if the caller ignores the error

**Category:** Abuse case

**Attack:** Task 9 specifies `record` returns `Err(AuditError::EmptyCorrelationId)` if `row.correlation_id` is empty. An empty `correlation_id` in `AuditRow` is a programming error. If the caller propagates the error upward, the business transaction will roll back (because the caller's `Transaction` is dropped on error), and the business operation will appear to have failed from the user's perspective — for what is actually a code defect in the audit path. Alternatively, if any call site suppresses the error (`let _ = record(tx, row).await`), the audit event is silently dropped.

**Why the design doesn't prevent it:** The design uses `Result` return to signal programming errors that should never occur if the call site is correct. In production, these errors should be impossible — but when they occur (due to bugs), they either cause spurious user-visible failures or silent audit gaps.

**Mitigation required:** Consider making empty `correlation_id` a `debug_assert!` (panics in debug, no-op in release) rather than a `Result` — since the `synth:` fallback should have been applied at the call site already, a genuinely empty string means a bug slipped through both the call site and the type system. Alternatively, `record` could auto-assign a `synth:` value as a last resort rather than returning an error, with a `tracing::error!` (not warn) to make the bug visible without disrupting the user.
