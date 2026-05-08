# Adversarial Review — Phase 6 — Round 5

## Summary

0 critical · 2 high · 3 medium · 2 low

## Round 4 Closure

| ID | Status | Notes |
|----|--------|-------|
| F301 | Closed | `record_audit_event` retired; grep CI test added |
| F302 | Closed | UFCS form `<AuditEvent as strum::EnumCount>::COUNT` specified |
| F303 | Closed | `SecretsRevealed` blocked until Phase 10; placeholder registry with non-empty assertion |
| F304 | Closed | `AUDIT_KINDS` deleted; single vocabulary |
| F305 | Closed | `AuditEvent` enum documented as sole vocabulary gate |
| F306 | Closed | Startup guard queries `tbl_name = 'audit_log'` in `sqlite_master` |

---

## New Findings

### F401 — HIGH — `chain::verify` return value on all-ZERO_SENTINEL (empty post-filter) table is undefined

**Category:** Logic flaw

**Attack:** After migration 0006 runs on an existing database, every pre-existing row has `prev_hash = ZERO_SENTINEL`. The startup guard calls `chain::verify` via a paginated `ORDER BY rowid ASC` loop. Every row is skipped as pre-epoch. The iterator is exhausted with zero non-sentinel rows processed. The design specifies `Err(ChainError::EmptyHash)` for `""` and skip for ZERO_SENTINEL, but is silent on what `verify` returns when the remaining sequence is empty after filtering. If the implementation returns any error variant, every daemon startup on a migrated system that has not yet written a single Phase-6 row logs a chain-verification error permanently, and operators cannot distinguish "legitimate chain break" from "brand new chain."

**Why the design doesn't prevent it:** The `chain::verify` API contract covers two terminal states (`""` → error; ZERO_SENTINEL → skip) but omits the third: empty iterator after filtering.

**Mitigation required:** Add an explicit contract to the `chain::verify` task: "returning an empty iterator after filtering ZERO_SENTINEL rows is `Ok(())` — it means the chain has not started yet, which is a valid state." Add a test that calls `verify` on a slice of ZERO_SENTINEL-only rows and asserts `Ok(())`.

---

### F402 — HIGH — Dedicated `SqliteConnection` enters unrecoverable state after mid-transaction failure; all subsequent audit writes fail permanently

**Category:** Single point of failure / cascade failure

**Attack:** `AuditWriter::record` acquires the mutex, issues `BEGIN IMMEDIATE`, queries the last hash, computes and INSERTs, then issues `COMMIT`. If `COMMIT` returns an error (disk-full, I/O error, SQLite internal error), the function issues `ROLLBACK` and returns `Err`. However, if `ROLLBACK` itself fails, or if the connection's internal state is left dirty, the connection is now inside a failed transaction. The next `BEGIN IMMEDIATE` on the same connection returns `SQLITE_ERROR: cannot start a transaction within a transaction`. Every subsequent call to `record` fails permanently for the process lifetime. The daemon continues running — serving requests, applying mutations — with a completely silenced audit trail. The startup guard on next boot does not detect this because no rows were written.

**Why the design doesn't prevent it:** The design specifies `ROLLBACK` on error before returning but does not specify what happens if `ROLLBACK` itself fails or if the connection is in an irrecoverable state. The `Mutex<SqliteConnection>` design has no recovery path.

**Mitigation required:** After any `COMMIT` or `ROLLBACK` failure, close and reopen the dedicated connection before releasing the mutex, ensuring the next `record` call starts from a clean connection state. Specify this in the `AuditWriter` task: "on any transaction-level error, close the connection (`conn.close()` or drop + reopen) and store the new clean connection in the mutex before returning the error."

---

### F403 — MEDIUM — `Actor::Bootstrap` has no `actor_id` field; `actor_id TEXT NOT NULL` in schema has no specified value for this variant

**Category:** Assumption violation

**Attack:** An `AuditRow` is constructed with `actor: Actor::Bootstrap`. `record` serialises `Actor` to `actor_kind`/`actor_id`. `Bootstrap` has no associated id field. The schema has `actor_id TEXT NOT NULL`. If the implementer uses `NULL`, the NOT NULL constraint fires and the INSERT fails — every bootstrap-phase audit event is unwritable. If the implementer uses `""`, the insert succeeds but an empty string is indistinguishable from a missing value and breaks `actor_id = ?` filter queries. The design does not specify which convention to use.

**Why the design doesn't prevent it:** The `Actor` enum design task does not specify the `actor_id` value for the `Bootstrap` variant.

**Mitigation required:** Add to the `Actor` design task: `Actor::Bootstrap` serialises as `actor_kind = "system"`, `actor_id = "bootstrap"`. This is a one-line design decision; make it before implementation.

---

### F404 — MEDIUM — CI grep test for `record_audit_event` removal is self-defeating unless the pattern is split

**Category:** Logic flaw

**Attack:** If the grep test is implemented as a Rust `#[test]` containing the literal string `"record_audit_event"`, the test source file itself contains the string and the grep will always report at least one match — itself. The test either always fails, gets `#[ignore]`-ed, or the implementer introduces a `concat!()` split that is non-obvious to reviewers.

**Why the design doesn't prevent it:** The design says "Add CI grep test: no `record_audit_event` references remain" without specifying the implementation method.

**Mitigation required:** Specify that the check is a `just` recipe running `grep -r --include="*.rs" "record_audit_event" core/crates/` rather than a Rust test file, which sidesteps the self-reference problem. The `just check-rust` recipe can call it. This is cleaner than a Rust test with a `concat!` workaround.

---

### F405 — MEDIUM — `AUDIT_KINDS` deletion must be verified to have no kinds absent from `AUDIT_KIND_VOCAB` that existing tests write

**Category:** State manipulation

**Attack:** `AUDIT_KINDS` (47 entries) contains 7 strings absent from `AUDIT_KIND_VOCAB` (41 entries). If any test writes one of these 7 kinds to `InMemoryStorage` via `record_audit_event`, deleting `AUDIT_KINDS` and retiring `record_audit_event` will break those tests. The design does not include a pre-deletion audit step to confirm no such tests exist.

**Why the design doesn't prevent it:** The vocabulary consolidation task does not include a verification step before deletion.

**Mitigation required:** Add a one-step pre-deletion check to the vocabulary consolidation task: grep for each of the 7 orphaned strings in the test files and confirm zero hits before deleting `AUDIT_KINDS`. This is a manual confirmation step, not a code change.

---

### F406 — LOW — 10 MB audit query soft cap has no specified measurement or enforcement mechanism

**Category:** Resource exhaustion

**Attack:** 1000 rows × 15 KB `redacted_diff_json` each = 15 MB loaded into memory before the cap is checked. The cap is described as "soft" with no enforcement specification.

**Why the design doesn't prevent it:** The design specifies the threshold but not whether it is enforced at serialisation time (after allocation) or at row-fetch time (preventing allocation).

**Mitigation required:** Specify that the cap is enforced at row-accumulation time: accumulate a running `length(redacted_diff_json)` sum per row fetched (can be done in the SQL projection), stop fetching when the total exceeds 10 MB, return `truncated: true`. This prevents the full allocation before truncation.

---

### F407 — LOW — `AUDIT_KINDS` divergence will resolve on deletion; no action needed beyond confirming deletion is atomic

**Category:** Documentation

**Attack:** See Round 4 findings summary. This resolves automatically when `AUDIT_KINDS` is deleted in the same commit as the vocabulary redirect. No separate action needed.

**Why the design doesn't prevent it:** N/A — design already addresses this.

**Mitigation required:** Confirm deletion happens atomically. No design change needed.
