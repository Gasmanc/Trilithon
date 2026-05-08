# Adversarial Review — Phase 6 — Round 8

## Summary

0 critical · 2 high · 1 medium · 1 low

## Round 7 Closure

| ID | Status | Notes |
|----|--------|-------|
| F601 | Closed | `#[serde(rename_all = "lowercase")]` mandated on `AuditOutcome`; live INSERT per-variant required in acceptance criteria |
| F602 | Closed | Corpus test acceptance criteria expanded: per-outcome, per-actor, chain verify, redaction_sites assertions all specified |
| F603 | Closed | `caddy_instance_id` sourcing contract documented; `"local"` scoped to bootstrap-only |
| F604 | Closed | Cancel-safety doc comment updated to distinguish happy path (cancel-safe) from error-recovery path (not cancel-safe) |
| F605 | Closed | `ChainError` enum explicitly defined: `EmptyHash { row_id: String }` and `ChainBroken { row_id: String, expected: String, actual: String }` |

---

## New Findings

### F801 — HIGH — `BEGIN IMMEDIATE` on the dedicated audit connection has no timeout; a contended write lock stalls all audit writes indefinitely under the Mutex

**Category:** Cascade failure / Resource exhaustion

**Attack:** `AuditWriter::record` acquires the `Mutex<Option<SqliteConnection>>` and then issues `BEGIN IMMEDIATE` on the dedicated connection. In SQLite's locking protocol, `BEGIN IMMEDIATE` requests a `RESERVED` lock immediately. If another connection on the same database file currently holds a `RESERVED` lock (i.e., any other business transaction is in the middle of a write — mutations, snapshot inserts, session updates), `BEGIN IMMEDIATE` blocks until that writer commits or rolls back. SQLite's default busy timeout is 0 (fail immediately) but sqlx sets it via `busy_timeout`; the design does not specify a value. If `busy_timeout` is not set or is very long, the dedicated audit connection can block inside `BEGIN IMMEDIATE` while holding the `Mutex`. Because the `Mutex` is held, ALL subsequent calls to `record` queue behind it. A single slow business transaction writing a large snapshot (potentially megabytes) serialises the entire audit trail for its duration. If the business transaction hangs (e.g., waiting on an I/O flush), audit writes stall forever — the daemon continues serving requests but no audit events are written. There is no circuit breaker, no timeout returned to callers, and no observability event emitted.

**Why the design doesn't prevent it:** The `AuditWriter` implementation task specifies `BEGIN IMMEDIATE` and the `Mutex` but says nothing about the `busy_timeout` on the dedicated connection or a per-`record`-call timeout. The design assumes the dedicated connection's `BEGIN IMMEDIATE` acquires the lock promptly, which is only true if no other writer is active.

**Mitigation required:** Add to the `AuditWriter` task: "The dedicated audit `SqliteConnection` MUST be opened with `PRAGMA busy_timeout = 5000` (5 seconds). If `BEGIN IMMEDIATE` returns `SQLITE_BUSY` after the timeout, `record` MUST release the Mutex and return `Err(AuditError::BusyTimeout)` rather than blocking indefinitely. Log `tracing::error!(kind = %row.kind, "audit: write timed out — database busy")` before returning the error." This caps the Mutex hold time and gives callers and operators a signal when the audit trail is being delayed by write contention.

---

### F802 — HIGH — `RedactedDiff` has no specified public accessor; `adapters::AuditWriter::record` cannot bind the inner string to SQL, and a serde-based workaround risks double-encoding

**Category:** Schema/type mismatch / Composition failure

**Attack:** `RedactedDiff` is defined as `pub struct RedactedDiff(String)` in `crates/core` with the explicit constraints "No `From<String>`, no public field." `AuditWriter::record` is in `crates/adapters` and must bind the inner string to the `redacted_diff_json` column via sqlx. Because the inner field is private and no public accessor is specified, `record` cannot read the value. An implementer who follows the letter of the design will reach for one of two workarounds:

(a) Derive `serde::Serialize` on `RedactedDiff` as a transparent newtype. When sqlx binds a `serde_json::Value` via `Json(redacted_diff)`, it serialises to `"\"the diff content\""` — the string is JSON-encoded a second time, storing a doubly-quoted string in `redacted_diff_json`. Queries that parse `redacted_diff_json` as JSON will receive an unexpected string type instead of a JSON object, silently corrupting every diff stored in the audit log.

(b) Add `pub(crate)` visibility, which works within `core` but not from `adapters`.

(c) Add a Rust feature or re-export that exposes the inner value — no design guidance means every implementer decides independently.

The compound effect: the type boundary that ADR-0009 mandated to prevent unredacted diffs from reaching the writer also prevents the writer from reading the value it needs to store. The design simultaneously requires the type to be opaque and requires the adapters crate to extract its content.

**Why the design doesn't prevent it:** The `RedactedDiff` definition task forbids `From<String>` and a public field but specifies no public read accessor. The `AuditWriter` task does not mention how the inner string is extracted for the SQL bind. The gap is in the junction between two tasks.

**Mitigation required:** Add to the `RedactedDiff` definition task: "`RedactedDiff` MUST expose exactly one public read accessor: `pub fn as_str(&self) -> &str { &self.0 }`. This is the only way for `adapters` to extract the value for SQL binding. `record` MUST bind the result of `.as_str()` directly as a TEXT value — NOT via `serde_json::to_string` or `Json(…)`, which would double-encode the string." This one accessor preserves the opacity invariant (callers cannot construct a `RedactedDiff` from a raw string) while giving the writer the read access it needs.

---

### F803 — MEDIUM — The empty-`correlation_id` error log does not include the generated `synth:` value; the audit row is permanently un-findable from the error log

**Category:** Observability gap

**Attack:** The `AuditWriter::record` task specifies: when `correlation_id` is empty, emit `tracing::error!(kind = %row.kind, "audit: empty correlation_id")`, replace with `synth:<new_ulid>`, write the row. The error log message carries `kind` but not the generated `synth:` value. An operator who sees this error in logs cannot determine which audit row it corresponds to — the log says "some event of kind X had an empty correlation_id" but does not say what `synth:` value was assigned. The audit row exists with `correlation_id = "synth:01J..."` but the log message carries no `correlation_id` field. The row cannot be found from the log line without scanning the entire audit table for `synth:` prefixes. Compare this to the no-active-span case (R3-F206, now closed), where the design specifies `tracing::warn!(correlation_id = %synth_id, …)` — the synth value is included. The empty-string path was specified separately and the synth value was omitted.

**Why the design doesn't prevent it:** The `AuditWriter` task specifies the error log for the empty-string case without including the `correlation_id = %synth_id` structured field that was added for the no-span case. The two paths were amended at different rounds and diverged.

**Mitigation required:** Update the `AuditWriter` task to read: "emit `tracing::error!(correlation_id = %synth_id, kind = %row.kind, "audit: empty correlation_id")` where `synth_id` is the replacement value that will be written to the row." This mirrors the no-span path and ensures the log and the audit row share a common identifier.

---

### F804 — LOW — Test criterion (f) for `AuditWriter` requires simulating a COMMIT failure against an in-memory SQLite with no specified mechanism; the test is likely to be vacuous

**Category:** Test coverage gap

**Attack:** The `AuditWriter` implementation task requires test (f): "a test simulates a forced COMMIT failure (by corrupting the in-memory DB) and asserts the next `record` call succeeds (connection was reopened)." SQLite in-memory databases do not support I/O errors that cause COMMIT failures. The only mechanism available to force a COMMIT failure on an in-memory SQLite is to close or corrupt the connection object itself — which is not a SQLite I/O error and produces a different error variant than a real disk-level COMMIT failure. An implementer who cannot construct a realistic COMMIT failure will either: (a) skip the test and mark it `#[ignore]`, (b) write a test that verifies `record` succeeds after a `None` slot (which tests `ConnectionLost` recovery, not `COMMIT` failure recovery), or (c) use a file-based test database and truncate the WAL file — a fragile and environment-dependent approach. In all cases the actual recovery branch (error-recovery after step (8) `COMMIT` failure) is not exercised.

**Why the design doesn't prevent it:** The acceptance criterion names the mechanism ("corrupting the in-memory DB") but this mechanism does not cause a COMMIT failure in practice. The criterion was authored to describe intent, not a working implementation path.

**Mitigation required:** Replace acceptance criterion (f) with a mechanism that actually exercises the recovery path: "test (f): construct an `AuditWriter` backed by a file-based SQLite; write one row successfully; then delete the database file from disk to force the next `record` call's `BEGIN IMMEDIATE` to fail; assert `record` returns an error; assert the next `record` call after recovery succeeds (connection was reopened against a new temp file, or the error path correctly stores `None` and returns `ConnectionRecoveryFailed`)." Alternatively: "test (f): use `AuditWriter` configured with a `SqliteConnection` where the underlying file is replaced with a zero-byte file after the first successful write; assert subsequent write returns error and slot is `None` or `Some(new_conn)` depending on recovery success." The key requirement is that the test actually triggers the error-recovery branch code path.

---

## No findings (categories with nothing concrete to raise)

- **Authentication & authorization** — `AuditWriter` is server-internal; no auth surface exposed.
- **Abuse cases** — No new abuse vector beyond what prior rounds have covered. Rate limiting is out of scope for an append-only audit log.
- **Race conditions** — `Mutex<Option<SqliteConnection>>` + `BEGIN IMMEDIATE` closes all concurrent-write races identified in prior rounds.
- **State machine violations** — Migration step ordering (ALTER → UPDATE → trigger CREATE) is correct; no state machine gap found.
- **Eventual consistency** — Single-process SQLite; no multi-store consistency gap.
- **Rollbacks** — Audit writes are intentionally out-of-band; no rollback semantics for audit rows by design.
- **Orphaned data** — Immutability triggers prevent cleanup by design; no new orphan accumulation path.
- **Timeouts & retries** — No retry loop in `record`; error returned to caller. (F801 covers the lock-acquisition timeout gap, which is the remaining exposure.)
- **Migration hazards** — Migration step order is correctly specified (UPDATE before triggers); no partial-apply hazard under sqlx transactional migrations.
- **Logic flaws** — Compile-time assertion `<AuditEvent as strum::EnumCount>::COUNT == AUDIT_KIND_VOCAB.len()` and the display-strings test cover variant/vocab divergence. ZERO_SENTINEL vs `""` distinction is correctly specified. `chain::verify` all-ZERO_SENTINEL return `Ok(())` is specified and tested.
- **Documentation traps** — The visually similar `auth.bootstrap-credentials-created` vs `auth.bootstrap-credentials-rotated` variant names are protected by the no-duplicate-display-strings test; no new documentation trap found beyond F803.
