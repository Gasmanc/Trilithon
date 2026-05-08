# Adversarial Review — Phase 6 — Round 7

## Summary

0 critical · 1 high · 2 medium · 2 low

## Round 6 Closure

| ID | Status | Notes |
|----|--------|-------|
| F501 | Closed | `as_mut()` replaces `take()` in happy path; cancel-safety documented; `take()` used only in error-recovery branch |
| F502 | Closed | Sentinel rows hash-but-don't-validate; first real row verified against accumulated sentinel hash; tamper-sentinel test added to acceptance criteria |
| F503 | Closed | `check-rust` recipe now lists `grep-no-record-audit-event` as a dependency; wiring made explicit in acceptance criteria |
| F504 | Closed | `length(CAST(redacted_diff_json AS BLOB))` substituted for `length(redacted_diff_json)` |
| F505 | Closed | `connect()` failure during recovery stores `None` and returns `ConnectionRecoveryFailed { write_error, connect_error }` |

---

## New Findings

### F601 — HIGH — `AuditOutcome` serde representation unspecified; default derive produces PascalCase strings that violate the schema CHECK constraint

**Category:** Schema/type mismatch

**Attack:** The design specifies `AuditOutcome` as an enum with variants `Ok`, `Error`, `Denied` and says "derive `serde::Serialize/Deserialize`". The `audit_log` table has `outcome TEXT NOT NULL CHECK (outcome IN ('ok', 'error', 'denied'))` — all lowercase. Rust's `#[derive(Serialize)]` on an enum by default serialises variant names verbatim: `Ok` → `"Ok"`, `Error` → `"Error"`, `Denied` → `"Denied"`. When `AuditWriter::record` binds `row.outcome` to the `outcome` column, SQLite evaluates the CHECK constraint. `"Ok"` is not in `('ok', 'error', 'denied')`. Every INSERT fails with `CHECK constraint failed: audit_log`. All audit writes return an error and the audit trail is permanently silent. The integration tests for the happy path will catch this only if the test actually reads back the `outcome` column or inspects the error — a test that merely asserts `record(…).is_ok()` will catch it, but a test that constructs and writes an `AuditRow` without asserting the insert succeeded (e.g. using `?` with no check) will miss it.

**Why the design doesn't prevent it:** The `AuditRow` definition task says "derive `serde::Serialize/Deserialize`" for `AuditOutcome` without specifying `#[serde(rename_all = "lowercase")]` or per-variant `#[serde(rename = "ok")]`. The serde round-trip test for `AuditRow` would catch this only if it also binds to a live SQLite connection with the CHECK constraint active — a unit-level serde test (no DB) would not.

**Mitigation required:** Add `#[serde(rename_all = "lowercase")]` to the `AuditOutcome` enum definition in the `AuditRow` task, and add an explicit acceptance criterion: "A live INSERT of each `AuditOutcome` variant through `AuditWriter::record` must succeed without a CHECK constraint error." The `Actor` enum has its serialisation mapping specified explicitly (variant → `actor_kind`/`actor_id` strings); `AuditOutcome` must be equally explicit.

---

### F602 — MEDIUM — `naïve-diff corpus through writer` test has no acceptance criteria; implementation can satisfy it trivially

**Category:** Test coverage gap

**Attack:** The task "Naïve-diff corpus through writer" has only: "Done when: `cargo test -p trilithon-adapters audit::naive_corpus` passes." There is no specification of what cases the corpus must cover, what events are exercised, or what assertions the test makes. An implementer can write `#[test] fn naive_corpus() {}` — an empty test — and the criterion is satisfied. Compare to the redactor corpus test, which specifies: every path in `PHASE6_REGISTRY` exercised, placeholder format assertion, and count assertion `PHASE6_REGISTRY.0.len() == corpus_cases.len()`. The writer corpus has no equivalent. The risk is that the corpus provides no coverage of the chain linearity guarantee, the `redaction_sites` count propagation, or the `actor_kind`/`actor_id` split — all of which could be silently wrong.

**Why the design doesn't prevent it:** The task was added to close R3-F201 concerns about concurrent writes but its acceptance criteria were never completed.

**Mitigation required:** Add concrete acceptance criteria to the naïve-diff corpus test: "(a) corpus MUST include at least one row for each `AuditOutcome` variant; (b) corpus MUST include one row for each `Actor` variant and assert the stored `actor_kind`/`actor_id` values match the specified mapping; (c) after all corpus rows are written, `chain::verify` over the written rows returns `Ok(())`; (d) `redaction_sites` stored in the DB equals the count returned by `RedactedDiff::new` for that row." Without (c), the corpus test provides zero coverage of chain correctness.

---

### F603 — MEDIUM — `caddy_instance_id` on `AuditRow` is a required `String` with no specified value; call sites will diverge silently

**Category:** Documentation trap / assumption violation

**Attack:** `AuditRow` carries `caddy_instance_id: String` as a required field (the schema has `DEFAULT 'local'` but `record` binds the struct field, not the default). The design specifies the field exists but never states what value call sites should supply. In integration tests where the daemon is configured against a single local instance, every test will use `"local"` (matching the seed row in migration `0003`). In a future context where multiple `caddy_instance_id` values exist, a call site that hard-codes `"local"` when `row.caddy_instance_id` should be the actual instance ID produces audit rows attributed to the wrong instance. The immutability triggers prevent correction. The design is silent on this field's sourcing contract, meaning each implementer decides independently — some may use the active instance from context, others will hard-code `"local"`.

**Why the design doesn't prevent it:** The `AuditRow` task lists `caddy_instance_id: String` in the field inventory without a sourcing note. No other task specifies "populate `caddy_instance_id` from X at call sites."

**Mitigation required:** Add to the `AuditRow` task: "`caddy_instance_id` MUST be populated from the active `CaddyInstance.id` in context at the call site; the literal string `'local'` is acceptable only for bootstrap-phase events where no instance context exists yet. Document this at the field definition." This is a one-line clarification that prevents a class of silent attribution bugs.

---

### F604 — LOW — Cancel-safety doc comment overstates the restriction; `as_mut()` fix makes the happy path cancel-safe, but the comment warns against ALL cancellation

**Category:** Documentation trap

**Attack:** The `AuditWriter` doc comment, as specified, reads: "This method is NOT cancel-safe. Callers MUST NOT drop the future mid-await via `tokio::time::timeout` or `select!`. Doing so during the error-recovery reopen may leave the connection slot as `None`." After the F501 fix, `as_mut()` is used in the happy path — the Option slot is never vacated during normal operation. Cancellation during the happy path (before any error) is now safe: the Mutex guard is dropped, the lock is released, the connection slot remains `Some`. Only the error-recovery branch (where `guard.take()` is used) is not cancel-safe. A future contributor reading "MUST NOT drop the future mid-await" will apply `tokio::time::timeout` avoidance to ALL callers, including those that only use the happy path. This is overly restrictive — callers implementing per-audit-event timeouts for latency control will be blocked by the warning. Worse, a contributor who reads the comment and then looks at the code may see `as_mut()` and conclude the comment is wrong and remove it entirely, eliminating the real warning about the error-recovery path.

**Why the design doesn't prevent it:** The doc comment was written when `take()` was used in the happy path (the pre-F501 design). The F501 fix changed the happy path but the doc comment language was not updated to reflect the narrower restriction.

**Mitigation required:** Update the doc comment to: "Cancel safety: the happy path (when no transaction error occurs) IS cancel-safe — the connection slot remains `Some` throughout. The error-recovery path (entered only on `BEGIN`/INSERT/`COMMIT` failure) is NOT cancel-safe: if the future is dropped while the recovery `connect()` is in progress, the slot may be left as `None`. Callers wrapping this method in `tokio::time::timeout` or `select!` should be aware of this distinction."

---

### F605 — LOW — `ChainError` variants are referenced in tests but never defined in the design; implementer-invented variants will diverge across the codebase

**Category:** Documentation trap / schema mismatch

**Attack:** The design specifies several tests that assert specific `ChainError` variants: `Err(ChainError::EmptyHash { row_id })` (F120/F203 closure test), `Err(ChainBroken)` (tamper-sentinel test in F502 acceptance criteria). Neither variant is defined in any design task. The acceptance criteria reference them as if they exist, but no task specifies the `ChainError` enum, its variants, or their fields. An implementer creating `ChainError` will invent variant names and field types independently — `ChainBroken` vs `ChainIntegrityViolation`, `row_id: String` vs `row_id: Ulid` vs `row_id: u64`. If any other crate or test matches on these variants, the mismatch causes compile errors or, worse, a test that pattern-matches on the wrong branch silently passes. The design's tests reference `ChainError::EmptyHash { row_id }` (with a named field) and `Err(ChainBroken)` (bare variant, no field) — these are already inconsistent with each other.

**Why the design doesn't prevent it:** No task defines the `ChainError` enum. The variants appear in test acceptance criteria but are never formally specified.

**Mitigation required:** Add a definition to the `chain::verify` task: "`ChainError` MUST be a `thiserror`-derived enum with variants: `EmptyHash { row_id: String }` (row with `prev_hash = ""` detected) and `ChainBroken { row_id: String, expected: String, actual: String }` (computed hash does not match stored `prev_hash`). Both variants include `row_id` as a `String` (ULID in its canonical 26-char representation). The `ChainBroken` variant includes `expected` (the hash `record` should have computed from the predecessor) and `actual` (the value stored in the row's `prev_hash` column) to enable forensic diagnosis."

---

## No findings

The following categories produced no concrete scenario after analysis:

- **Authentication & authorization** — `AuditWriter` is a server-internal type; no auth surface is exposed by the design.
- **Race conditions** — the `Mutex<Option<SqliteConnection>>` + `BEGIN IMMEDIATE` design from R3-F201/R5-F402 closes the concurrent-write race; no new race is introduced.
- **Resource exhaustion** — the 10 MB byte-accurate cap (F504) and paginated chain verify (R2-F108) address the main vectors.
- **State machine violations** — migration order (ALTER then UPDATE then triggers) is correctly sequenced; no state machine gap found.
- **Migration hazards** — SQLite DDL is transactional; partial migration rolls back cleanly; no new hazard found.
- **Single points of failure** — the connection recovery path (R5-F402, R6-F505) addresses permanent connection loss.
- **Rollbacks** — audit writes are intentionally out-of-band; no rollback semantics are defined for audit rows by design.
- **Orphaned data** — the immutability triggers prevent orphan cleanup but this is by design (append-only); no orphan accumulation path found.
- **Timeouts & retries** — `AuditWriter` does not retry; error is returned to caller; no retry loop found.
- **Eventual consistency** — single-process SQLite; no multi-store consistency gap.
