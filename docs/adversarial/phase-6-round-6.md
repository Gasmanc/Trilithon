# Adversarial Review — Phase 6 — Round 6

## Summary

0 critical · 2 high · 3 medium · 0 low

## Round 5 Closure

| ID | Status | Notes |
|----|--------|-------|
| F401 | Closed | `chain::verify` returns `Ok(())` on all-ZERO_SENTINEL iterator; explicit test added |
| F402 | Closed | Connection recovery: close+reopen on any transaction failure; `Mutex<Option<SqliteConnection>>` |
| F403 | Closed | `Actor::Bootstrap` → `actor_kind = "system"`, `actor_id = "bootstrap"` specified |
| F404 | Closed | `just` recipe (not Rust test) avoids self-reference; recipe added to `check-rust` |
| F405 | Closed | Pre-deletion grep of 7 orphaned strings added to vocabulary consolidation acceptance criteria |
| F406 | Closed | Cap enforced at row-accumulation time using `length(redacted_diff_json)` per-row sum |
| F407 | Closed | Deletion is atomic; no design change needed |

---

## New Findings

### F501 — HIGH — Future cancellation after `guard.take()` and before restore permanently loses the dedicated connection

**Category:** Cascade failure / cancellation safety

**Attack:** `AuditWriter::record` does:
```
(2) lock mutex → guard
(3) conn = guard.take().ok_or(…)?  // Option is now None
(4) conn.execute("BEGIN IMMEDIATE").await?   // suspension point
…
(9) *guard = Some(conn)             // restore
```
If the caller drops the future between step (3) and step (9) — for example via `tokio::time::timeout(…, writer.record(row)).await` or a `select!` branch that fires first — Rust's async drop machinery runs. The mutex guard is dropped, releasing the lock. `conn` is also dropped. The connection slot (`guard.as_deref()`) was `None` at the moment of drop. From this point forward every call to `record` returns `Err(AuditError::ConnectionLost)` permanently — no error recovery path runs, because recovery only runs on `BEGIN`/INSERT/`COMMIT` failure, not on cancellation. The daemon continues running with a silenced audit trail.

**Why the design doesn't prevent it:** The design specifies `guard.take()` in the happy path without acknowledging that any `.await` between `take` and restore creates a cancellation window.

**Mitigation required:** Two options, either is acceptable:
(a) **Non-take happy path**: use `guard.as_mut().ok_or(AuditError::ConnectionLost)?` to borrow the connection for the entire write without vacating the slot. The `Option` is `None` only during the error-recovery reopen (where the future is already errored, so cancellation in recovery is acceptable). Document in the `AuditWriter` doc comment that `record` is NOT cancel-safe for the error-recovery path only.
(b) **Document and forbid cancellation entirely**: add to `AuditWriter`'s doc comment: "**Cancel safety**: this method is NOT cancel-safe. Callers MUST NOT drop the future mid-await (e.g., via `tokio::time::timeout` or `select!`). Doing so may leave the connection slot as `None`, causing all subsequent writes to fail permanently." Add a test that verifies recovery (step 10) restores `Some` even after a simulated mid-transaction failure, confirming the ordinary error path is safe.

---

### F502 — HIGH — ZERO_SENTINEL skip discards the row's computed hash; first non-sentinel row's `prev_hash` is unverifiable

**Category:** Logic flaw

**Attack:** The design specifies: skip rows where `prev_hash == ZERO_SENTINEL`. If "skip" means "do not hash this row and do not update `last_computed_hash`," then after N sentinel rows the verifier has `last_computed_hash = None`. The first non-sentinel row has `prev_hash = sha256(canonical_json(last_sentinel_row))` — written correctly by `record`. The verifier has no accumulated hash to compare against. It either:
(a) treats the first non-sentinel row as a chain start (accepting any `prev_hash` value) — a tampered sentinel row is silently accepted, and a tampered `prev_hash` on the first real row goes undetected; or
(b) errors because `last_computed_hash` is `None` — every migrated database with any pre-migration rows fails verification permanently.

The design says "skip" without specifying whether the hash is computed and tracked.

**Why the design doesn't prevent it:** The `chain::verify` task says "skipped" without specifying that hash tracking still happens for sentinel rows.

**Mitigation required:** Clarify in the `chain::verify` task: "For a ZERO_SENTINEL row, **compute `sha256(canonical_json(row))` and record it as `last_computed_hash`**, but do NOT assert that `row.prev_hash` matches any predecessor. The first non-sentinel row's `prev_hash` MUST equal the accumulated `last_computed_hash` from the last sentinel row." This ensures a tampered sentinel row (its hash changes) is detected by the first real row's `prev_hash` check. Add the following test to the `chain::verify` acceptance criteria: "a tamper-sentinel test: sentinel rows followed by a real row with correct `prev_hash`; mutate a sentinel row's canonical JSON; `chain::verify` returns `Err(ChainBroken)` because the first real row's `prev_hash` no longer matches the mutated sentinel's computed hash."

---

### F503 — MEDIUM — `grep-no-record-audit-event` recipe is not specified to be wired into `check-rust`

**Category:** Logic flaw

**Attack:** The design specifies the `just` recipe `grep-no-record-audit-event` but says the recipe is "called from `just check-rust`" only in the retirement task's prose. The actual `justfile` snippet in the acceptance criteria shows only the recipe definition, not the `check-rust` dependency line. An implementer following the acceptance criteria literally will add the recipe but not wire it into `check-rust`. The grep check then runs only when explicitly invoked and is never enforced by CI.

**Why the design doesn't prevent it:** The acceptance criteria shows the recipe but not the `check-rust` wiring.

**Mitigation required:** Add an explicit acceptance criterion: "The `check-rust` recipe in `justfile` MUST list `grep-no-record-audit-event` as a dependency (e.g., `check-rust: … grep-no-record-audit-event`)." The `just check` invocation then enforces it in CI automatically.

---

### F504 — MEDIUM — `length(redacted_diff_json)` returns character count not byte count; multi-byte UTF-8 sequences undercount

**Category:** Logic flaw

**Attack:** SQLite `length(X)` for a TEXT column returns the number of characters (Unicode code points), not bytes. A `redacted_diff_json` containing multi-byte UTF-8 characters (e.g., JSON values with accented characters, CJK characters, or escaped Unicode) has `length(col) < actual_byte_size`. The 10 MB cap is checked against character count. An attacker or mis-behaving caller can store 10 MB of CJK characters in `redacted_diff_json` while `length(col)` reports ~3 MB (since each CJK character is 1 code point but 3 bytes). The cap is bypassed. On large deployments the query returns >10 MB of JSON, potentially causing OOM.

**Why the design doesn't prevent it:** The design specifies `length(redacted_diff_json)` without qualifying the unit.

**Mitigation required:** Replace `length(redacted_diff_json)` with `length(CAST(redacted_diff_json AS BLOB))` in the SQL projection. `CAST(TEXT AS BLOB)` returns the UTF-8 encoding; `length(BLOB)` returns byte count. This gives the correct byte-level measurement for the cap.

---

### F505 — MEDIUM — Error recovery after transaction failure silently stores a broken connection if `SqliteConnection::connect()` itself fails

**Category:** Cascade failure

**Attack:** Step (10) of `AuditWriter::record`: "close and reopen the dedicated connection, store new connection in `*guard = Some(new_conn)`." If `SqliteConnection::connect(path)` returns `Err(e2)`, the current implementation (as specified) returns `Err(original_error)` and leaves `*guard` in whatever state it was before recovery — which is `None` (vacated by `guard.take()` at step 3). The next call to `record` returns `AuditError::ConnectionLost` (step 3 finds `None`) with no indication that recovery failed. The original transaction error and the recovery failure are both invisible in the returned error. Operators see only "ConnectionLost" indefinitely with no root cause.

**Why the design doesn't prevent it:** The design says "close and reopen" without specifying what happens when `connect()` fails.

**Mitigation required:** Add to the `AuditWriter` task: "If `SqliteConnection::connect()` fails during recovery, store `*guard = None` (leave the slot explicitly empty — do not store a broken connection), and return `Err(AuditError::ConnectionRecoveryFailed { write_error: Box<…>, connect_error: Box<…> })` so both errors are surfaced. This is the same outcome as cancellation-induced loss, but now the returned error type distinguishes it from `ConnectionLost`."
