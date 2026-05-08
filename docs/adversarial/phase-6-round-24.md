# Adversarial Review — Phase 6 — Round 24

**Design summary:** Phase 6 adds a tamper-evident, append-only audit log to Trilithon: an `AuditWriter` with a dedicated serialised `tokio::sync::Mutex<Option<SqliteConnection>>`, a SHA-256 `prev_hash` chain verified via a stateful batch API (`ChainVerifyState` / `verify_batch` / `verify_finish`), a `RedactedDiff` newtype for secrets-safe diffs, an `Actor` enum with `from_kind_id` reverse-mapping and unknown-kind fallback, startup integrity checks, and a query API with a 10 MB soft cap.

**Prior rounds:** Rounds 1–23 reviewed. R23 findings assessed below.

- R23-H1 (`COUNT == AUDIT_KIND_VOCAB.len()` compile-time assertion fails because `#[strum(disabled)]` does not exclude `Unknown` from `EnumCount`): CLOSED in task body — line 14 now reads `assert!(<AuditEvent as strum::EnumCount>::COUNT - 1 == AUDIT_KIND_VOCAB.len())` with the explicit `-1` and a comment explaining it. The `all_variants()` test criterion also uses `COUNT - 1`. However, see M1 below — the sign-off checklist still carries the old form.
- R23-M1 (`PredecessorRow` field set underspecified — "all other columns as primitive types" was ambiguous for typed fields): CLOSED — line 64 now contains an explicit inline struct definition with all 16 fields named and typed (`id: String`, `actor_kind: String`, `actor_id: String`, `kind: String`, `outcome: String`, etc., with `occurred_at` absent). The startup paginator task (line 82) references the same struct. However, see M2 below — the conversion chain for `Ulid::from_str(&row.id)` has no specified error-handling path.

---

## Findings

### MEDIUM — Sign-off checklist (line 263) still carries the pre-R23 form of the `EnumCount` assertion (`COUNT == AUDIT_KIND_VOCAB.len()`, without `-1`), contradicting the corrected task body and creating an inconsistency that will cause the wrong assertion at implementation time

**Category:** Logic flaw

**Trigger:** The vocabulary task (line 14) was corrected in R23-H1 to read:

```rust
const _: () = assert!(<AuditEvent as strum::EnumCount>::COUNT - 1 == AUDIT_KIND_VOCAB.len());
// -1 because Unknown variant is included in COUNT but is not a fixed vocabulary entry
```

The design decisions section (line 231) also records the fix correctly: "`EnumCount::COUNT - 1 == AUDIT_KIND_VOCAB.len()` because `#[strum(disabled)]` does not exclude `Unknown` from `EnumCount`."

However, the sign-off checklist at line 263 still reads:

```
<AuditEvent as strum::EnumCount>::COUNT == AUDIT_KIND_VOCAB.len() compile-time assertion present in production code (not `#[cfg(test)]`); `AUDIT_KINDS` deleted.
```

This is the old form without `-1`. An implementer who uses the sign-off checklist as a quick-reference guide (a common practice for checking off completed work) will implement `COUNT == AUDIT_KIND_VOCAB.len()`, which does not compile when `Unknown` is present. Alternatively, an implementer who implements the correct `COUNT - 1` form will fail the sign-off checklist check because the checklist says `COUNT ==`, not `COUNT - 1 ==` — creating a false "not done yet" signal. A third path: the implementer who notices the discrepancy between the task body (`COUNT - 1`) and the checklist (`COUNT`) might attempt to reconcile them by adding a second assertion in both forms, causing a compile error (the `COUNT ==` form still fails).

Concrete failure sequence: implementer completes the vocabulary task correctly with `COUNT - 1 == AUDIT_KIND_VOCAB.len()`, runs `just check`, passes. Then reviews the sign-off checklist and reads `COUNT == AUDIT_KIND_VOCAB.len()`. Concludes this checklist item is not yet satisfied. Either (a) introduces a second incorrect assertion trying to satisfy the checklist, or (b) files the item as blocked. Both outcomes delay shipping a correct implementation.

**Consequence:** The sign-off checklist is a review artifact consulted at phase completion. A discrepancy between the checklist and the task body causes either a false blocked signal or a motivated attempt to satisfy the incorrect checklist form — which does not compile and triggers error-recovery behavior (removing the assertion) that degrades safety.

**Design assumption violated:** The design assumes the sign-off checklist is kept in sync with task body amendments. R23-H1 updated the task body and design decisions section but not the checklist.

**Suggested mitigation:** Update line 263 to match the corrected task body: `<AuditEvent as strum::EnumCount>::COUNT - 1 == AUDIT_KIND_VOCAB.len() compile-time assertion present in production code (not `#[cfg(test)]`); `AUDIT_KINDS` deleted.` The `-1` and an inline comment (`// Unknown variant counted by EnumCount but not in vocab`) should be present in both the assertion and the checklist.

---

### MEDIUM — `Ulid::from_str(&row.id)` in the `PredecessorRow → AuditRow` conversion chain has no specified error-handling path; a corrupted predecessor `id` permanently blocks all subsequent `record` calls

**Category:** State manipulation

**Trigger:** The `AuditWriter::record` step 5 (line 64) now specifies the full conversion chain from `PredecessorRow` to `AuditRow`:

> `Ulid::from_str(&row.id)`, `row.kind.parse::<AuditEvent>().unwrap_or_else(|_| AuditEvent::Unknown(row.kind.clone()))`, `Actor::from_kind_id(&row.actor_kind, &row.actor_id)` with fallback, `RedactedDiff::from_db_str(s)` for non-null `redacted_diff_json`

Three of the four conversions have specified fallbacks:
- `AuditEvent` parse failure: `.unwrap_or_else(|_| AuditEvent::Unknown(row.kind.clone()))` — always succeeds
- `Actor::from_kind_id` failure: substitute `Actor::System { component: format!("__unknown:{}", kind) }` + warn — always succeeds
- `RedactedDiff`: `from_db_str` is infallible — always succeeds

`Ulid::from_str(&row.id)` is the exception. `Ulid::from_str` returns `Result<Ulid, ulid::DecodeError>`. The design does not specify what to do when this returns `Err`. No `.unwrap_or_else`, no fallback, no propagation specification.

Concrete failure sequence:

1. An operator runs a direct SQL query on the database to investigate an incident (before the immutability triggers existed, or using `PRAGMA writable_schema` to bypass them, or via a backup that was restored before Phase 6 was deployed). A row's `id` column is set to a non-ULID string (e.g., `"corrupted"` or an empty string or a UUID without ULID encoding).

2. `AuditWriter::record` is called for the next legitimate write. Step 4: `BEGIN IMMEDIATE`. Step 5: the predecessor query fetches the row with `id = "corrupted"`. `Ulid::from_str("corrupted")` returns `Err(DecodeError::InvalidLength)`.

3. No fallback is specified. An implementer who propagates the error exits step 5 with an error, triggering step 9 (error-recovery path): `ROLLBACK`, `guard.take()`, close and reopen the connection. The next `record` call again reads the same predecessor row (it is still the last rowid), again fails `Ulid::from_str`, again enters error-recovery. This is a permanent write barrier — no audit row can ever be written again until the DB is manually repaired.

4. During the permanent write barrier, every `record` call triggers a connection close + reopen cycle, consuming file descriptors and producing error log noise, potentially masking the root cause.

The startup paginator has the same gap: when converting `PredecessorRow` to `AuditRow` for `chain::verify`, a `Ulid::from_str` failure on any row would stop paginated reconstruction at that row — leaving all subsequent rows unverified without the design specifying whether this is an error or a warning.

Note: the trigger does not require direct SQL access against the live DB. A database file from before Phase 6 (or a pre-Phase-6 backup restore) that has rows with non-ULID `id` values (if the Phase 5 schema used a different primary key format for some rows) would produce the same outcome on the first `record` call after upgrade.

**Consequence:** A single predecessor row with a malformed `id` string permanently blocks all audit writes. Since audit writes are out-of-band from business transactions, the application continues to operate, but no audit events are recorded — silently. Operators relying on the audit log for incident response would see a gap. The connection-recovery loop also masks the root cause, since each `record` attempt appears as a new connection error rather than as "Ulid parse failure on row X."

**Design assumption violated:** The design assumes that rows fetched from the `audit_log` table always have valid ULID `id` values, and therefore that `Ulid::from_str` is infallible in practice. This is consistent with normal operation (all writes go through `AuditWriter::record` which generates a fresh `Ulid`), but is violated by any pre-Phase-6 data, restored backups, or direct DB manipulation — all scenarios that the design acknowledges as possible contexts (startup guard, migration backfill, etc.).

**Suggested mitigation:** Specify a fallback for `Ulid::from_str` in the conversion chain, analogous to the `Actor::from_kind_id` fallback. Two options:

Option A (preferred — matches the pattern for other fallibles in the chain): if `Ulid::from_str(&row.id)` returns `Err`, emit `tracing::warn!(raw_id = %row.id, "audit: predecessor row has non-ULID id; using zero ULID for canonical_json")` and substitute `Ulid::nil()` (or a sentinel constant `FALLBACK_ULID`). This allows the write to continue and ensures `canonical_json` produces a deterministic output for the corrupted predecessor row. The next legitimate write's `prev_hash` will be computed against the canonical JSON that uses `Ulid::nil()` for the predecessor's `id` field — which is repeatable, so `chain::verify` will agree.

Option B (strict): propagate `Ulid::from_str` failure as a new `AuditError::MalformedPredecessorId { raw_id: String }` variant, add it to step 9's "transaction-level error" classification, and document that this error is non-recoverable without DB repair. This makes the failure explicit and operator-visible but still results in a write barrier.

The startup paginator conversion path needs the same specification: on `Ulid::from_str` failure, either substitute a sentinel (so verification continues) or propagate (so the operator knows which row is corrupted). Either choice must be documented.

---

## No findings (categories with nothing concrete to raise)

- **Authentication & authorization** — `AuditWriter` is server-internal; no new bypass vector.
- **Abuse cases** — 10 MB query cap (BLOB-accurate, COALESCE-wrapped, both columns), max 1000 rows, `AUDIT_QUERY_ROW_FIXED_OVERHEAD_BYTES` constant, `busy_timeout = 5000`, `occurred_at_ms > 0` guard before mutex lock. No new abuse vector.
- **Data exposure** — `RedactedDiff` newtype with controlled constructors; `from_db_str` is `pub` with doc comment and companion grep recipe. No new exposure vector.
- **Race conditions** — `tokio::sync::Mutex` + `BEGIN IMMEDIATE` serialises all writes. Concurrent chain test specified. No new race vector.
- **Resource exhaustion** — 500-row batch API bounds memory; no full-log preload; `busy_timeout`; 10 MB query cap. No new exhaustion vector.
- **Single points of failure** — Connection recovery (close + reopen), `ConnectionRecoveryFailed` surfacing both errors, `PRAGMA foreign_keys = ON` on recovery opens. No new SPOF.
- **Timeouts & retries** — `busy_timeout = 5000` + `BusyTimeout` return; test (h) verifies ~6 s bound. No retry amplification.
- **Eventual consistency** — Single-process SQLite; no multi-store gap.
- **Rollbacks** — Audit writes are out-of-band from business transactions by design; immutability by DB trigger. No rollback semantics for audit rows.
- **Rate limits** — `busy_timeout` + bounded query page sizes cover the query path. No new gap.
- **Orphaned data** — Immutability triggers prevent cleanup by design. No accumulation path during normal operation.
- **R23-H1 closure verification (task body and design decisions section)** — The task body (line 14) and the design decisions note (line 231) both correctly use `COUNT - 1`. The compile-time assertion will work as written for 44 named variants + 1 `Unknown` = 45 total, with `AUDIT_KIND_VOCAB.len() = 44`. The `all_variants()` test criterion also uses `COUNT - 1`. The task body is genuinely closed; only the sign-off checklist is stale (see M1 above).
- **R23-M1 closure verification** — `PredecessorRow` now has an explicit 16-field list with primitive types for all fields, `actor` split into two strings, `id`/`kind`/`outcome` as `String`, `occurred_at` absent. The startup paginator references the same struct by name. The implicit ambiguity that drove R23-M1 is resolved; the only remaining gap is the `Ulid::from_str` error path (see M2 above).
- **`canonical_json` key name consistency** — Key list in the `chain::verify` task (line 45) is explicit and sorted. `"redacted_diff_json"` (not `"redacted_diff"`) is specified. The key list in the task and in the design decisions section (line 199) are consistent. No divergence found.
- **`AuditOutcome` serde round-trip** — Derives `Deserialize` + `rename_all = "lowercase"`. The stability test asserts lowercase encoding. No new gap.
- **`verify_batch` / `verify_finish` semantics** — Fully specified with explicit per-row dispatch rule, empty-slice fast-return, `verify_finish` always `Ok(())`, tests (k) and (l). R21-M1, R21-M2, R21-L1, R22-M1, R22-L1, and R22-L2 all remain closed in the current design text.
- **`sentinel_count` test assertion** — Test criterion (c) now explicitly requires asserting `state.sentinel_count == N`. Closed.
- **Empty-slice state preservation test (j)** — Now operationally specified with explicit `state.last_computed_hash` assertion before and after the empty-slice call. Closed.

---

## Summary

**Critical:** 0  **High:** 0  **Medium:** 2  **Low:** 0

**Top concern:** M2 — `Ulid::from_str(&row.id)` in the `PredecessorRow → AuditRow` conversion chain has no fallback specified; a single predecessor row with a corrupted `id` string (possible after a pre-Phase-6 backup restore or any direct DB manipulation) permanently blocks all subsequent audit writes through an unbreakable error-recovery loop.

**Recommended action before proceeding:** Ready — 0 blockers. Both M1 and M2 are MEDIUM findings. M1 (sign-off checklist inconsistency) is a documentation error with no runtime consequence; the correct assertion is in the task body where it will be implemented. M2 (missing `Ulid::from_str` fallback) is a gap in the conversion chain specification that affects an uncommon error path (corrupted predecessor `id`). Neither rises to HIGH because: M1 does not affect the compiled code (only the checklist); M2 requires a pre-existing DB corruption that the immutability triggers (deployed in migration 0006) make impossible in steady-state operation. Both should be addressed before implementation begins, but neither blocks proceeding if the team accepts the risk.
