# Adversarial Review — Phase 6 — Round 10

## Summary

0 critical · 2 high · 3 medium · 1 low

## Round 9 Closure

| ID | Status | Notes |
|----|--------|-------|
| F901 | Closed | `canonical_json(row)` defined as named function in `core/src/audit.rs`; spec includes all columns, `prev_hash`, NULL→null, sorted keys, no whitespace; stability test added |
| F902 | Closed | `occurred_at_ms > 0` guard in `record`; zero or negative → `AuditError::InvalidTimestamp { occurred_at_ms }` |
| F903 | Closed | `chain::verify` returns `Err(ChainBroken)` on first broken link; early return specified; test (i) added confirming no finding reported for row 3 after break at row 2 |
| F904 | Closed | `PHASE6_REGISTRY` location clarified: `crates/core/src/audit.rs`; `SecretsRevealedNotYetSupported` guard lives only in `AuditWriter::record`; test code that calls `RedactedDiff::new` directly documented as bypassing the guard |
| F905 | Closed | Documentation note added: LIKE prefix scan on `synth:` IDs; efficiency caveat noted in `core/README.md` documentation task |
| F906 | Closed | `variant_count_matches_expected` test updated to assert `all_variants().len() == <AuditEvent as strum::EnumCount>::COUNT` |

---

## New Findings

### F1001 — HIGH — `canonical_json(row)` has no specified serialisation for the `AuditEvent` kind field; `Display` output and serde `Serialize` produce different strings, causing hash divergence between `record` and `chain::verify` if either uses the wrong encoding

**Category:** Schema/type mismatch

**Attack:** `canonical_json(row: &AuditRow)` is now a named function that includes every column of `audit_log`. The `kind` column stores the `AuditEvent::Display` string (e.g., `"mutation.applied"`). However, `AuditEvent` is specified to derive `serde::Serialize/Deserialize` — the serialisation form for serde is not specified in the design (prior rounds focused on `AuditOutcome`'s `#[serde(rename_all = "lowercase")]` gap but did not address `AuditEvent`'s serde form). If an implementer derives `#[derive(Serialize)]` on `AuditEvent` without `#[serde(rename_all = "kebab-case")]` or without per-variant `#[serde(rename = "...")]` annotations, serde produces PascalCase variant names (`"MutationApplied"`) rather than the kebab-case display strings (`"mutation.applied"`).

`canonical_json(row)` serialises the `AuditRow` struct to produce a JSON object. The `kind` field on `AuditRow` is typed as `AuditEvent`. If `canonical_json` uses `serde_json::to_value(&row)` or any serde path, the `kind` field in the canonical JSON is `"MutationApplied"`. But the `kind` column value stored in the database (bound via `.to_string()` / `Display`) is `"mutation.applied"`. The two serialisations disagree.

Concrete sequence: `record` calls `canonical_json(predecessor)` to compute `prev_hash`. `canonical_json` serialises `predecessor.kind` via serde as `"MutationApplied"`. The computed hash is H_a. Row N+1 is inserted with `prev_hash = H_a`. On startup, `chain::verify` calls `canonical_json(row_N)`, also serialises via serde, produces `"MutationApplied"`, computes H_a — chain verifies. This is internally consistent but produces a hash over a string that differs from the stored DB value.

The danger is if Phase 10 or any diagnostic tool ever re-derives `canonical_json` from the raw DB row (where `kind = "mutation.applied"` as TEXT). That tool will compute a hash that disagrees with every stored `prev_hash`, reporting the entire chain as broken — a false positive that cannot be corrected because the rows are immutable.

**Why the design doesn't prevent it:** The `canonical_json` spec says "include every column of `audit_log`" but does not specify the serialisation form of the `kind` field. The design fixed `AuditOutcome`'s serde form (F601) but omitted `AuditEvent`. Any implementation that serialises `AuditRow` via `#[derive(Serialize)]` on the struct and derives `Serialize` on `AuditEvent` without explicit rename annotations will produce canonical JSON that diverges from the DB column value for `kind`.

**Mitigation required:** Add to the `canonical_json` function spec: "The `kind` field in the canonical JSON object MUST be the `AuditEvent::Display` string (e.g., `\"mutation.applied\"`), NOT the serde-derived variant name. The implementation MUST either (a) construct the JSON object manually using `row.kind.to_string()` for the `kind` field, or (b) derive `#[serde(rename_all = "kebab-case")]` on `AuditEvent` and also add `#[serde(rename = \"...\")]` per-variant where the Display string contains dots (since kebab-case produces hyphens, not dots). Option (a) is safer and should be the default." Also add an acceptance criterion: "the `canonical_json` stability test MUST assert that the `kind` field value in the output equals `row.kind.to_string()` (the Display string), not a PascalCase or kebab-case serde variant name."

---

### F1002 — HIGH — The `AuditRow.id` field is typed as `Ulid` but the schema stores it as `TEXT PRIMARY KEY`; `canonical_json(row)` includes `id` in the JSON object; the string representation of a ULID is specified by crate version, and a crate upgrade can change formatting and silently break all chain hashes permanently

**Category:** State machine violation / orphaned data

**Attack:** `AuditRow.id: Ulid` is bound to `audit_log.id TEXT PRIMARY KEY` as the ULID's canonical 26-character uppercase Base32 string. `canonical_json(row)` includes `id` in the JSON object. The ULID spec mandates uppercase Crockford Base32. The `ulid` crate (used in this project) encodes as uppercase. However:

1. If a future version of the `ulid` crate changes the `Display` implementation (e.g., to lowercase, or adds padding), the canonical JSON changes for any row whose `Ulid` is formatted at `canonical_json` time. But the stored `prev_hash` values in the DB were computed with the old format. Every row after the crate upgrade computes a different `canonical_json` for predecessor rows → `chain::verify` reports `Err(ChainBroken)` for the entire chain starting from the first row written after the upgrade.

2. The `ChainError` variant `EmptyHash { row_id: String }` and `ChainBroken { row_id: String, ... }` store `row_id` as `String`. The spec says "ULID in its canonical 26-char string representation." If `canonical_json` formats `id` via `serde_json::to_value()` and the `Ulid` type's serde implementation differs from its `Display` implementation (e.g., serde serialises as a 128-bit integer or a hyphenated UUID-like string), the canonical JSON includes an `id` value that does not match the `TEXT` value stored in the DB column — same problem as F1001 but for the `id` field.

3. `canonical_json` is called at two sites: `record` (at write time) and `chain::verify` (at verify time). If `record` is compiled with crate version A and `chain::verify` with crate version B (possible in incremental deployment or if the function is re-evaluated after a dependency bump), they produce different canonical JSON for the same row.

**Why the design doesn't prevent it:** The `canonical_json` spec says "include every column of `audit_log`" and the stability test asserts byte-for-byte repeatability "across multiple calls" — but both calls in the test are in the same binary build, with the same crate version. The spec does not pin the string representation of `Ulid` or state that the function must produce the same bytes regardless of crate version.

**Mitigation required:** Add to the `canonical_json` spec: "The `id` field in the canonical JSON object MUST be the ULID's 26-character uppercase Crockford Base32 string (the `Display` representation). The implementation MUST NOT use serde to serialise the `Ulid` value — it MUST use `row.id.to_string()` explicitly, so the format is fixed to the `Display` contract and is independent of any serde implementation choices in the `ulid` crate." Add a test assertion: "the `id` field in the output of `canonical_json` equals `row.id.to_string()` and has length exactly 26."

---

### F1003 — MEDIUM — The `10 MB soft cap` accumulates `length(CAST(redacted_diff_json AS BLOB))` per row but adds a fixed 512-byte per-row overhead; the overhead is a guess, not a measurement; a row with many non-diff fields can exceed the cap silently

**Category:** Resource exhaustion

**Attack:** The design specifies (as of F406/F504 closure): the 10 MB cap is enforced at row-accumulation time using `length(CAST(redacted_diff_json AS BLOB))` per row with a "fixed per-row overhead of 512 bytes." The 512-byte figure accounts for non-diff columns in an `AuditRow` response. However, the non-diff fields include `notes: Option<String>` — which has no length constraint — and `error_kind: Option<String>`. An `AuditRow` where `notes` contains a 5 KB string and `redacted_diff_json` is NULL contributes 512 bytes to the cap counter but may actually contribute ~5.5 KB to the response. Under adversarial input (an operator deliberately storing verbose notes), 100 rows × 5 KB notes each = 500 KB (measured) but actually 500 KB (correct in this case). But `redacted_diff_json` could be NULL for these rows, so the diff column contributes 0 bytes to the cap — only the 512-byte overhead is counted. 10 MB / 512 = 20,000 such rows could be returned before hitting the cap, at 5 KB notes each = 100 MB response — 10× over the intended limit.

The underlying flaw: the 512-byte overhead is a proxy for non-diff payload, but the actual non-diff payload is unbounded (`notes` has no column constraint in the schema).

**Why the design doesn't prevent it:** The `notes` and `error_kind` columns have no length constraints in `0001_init.sql` or in the Phase 6 migration. The per-row overhead was sized to account for typical non-diff fields but not for adversarially large `notes` values.

**Mitigation required:** Either (a) add a write-time length constraint on `notes` in `AuditWriter::record` (e.g., truncate `notes` to 1024 chars) and document this in the `AuditRow` task, or (b) replace the 512-byte per-row overhead in the cap calculation with `512 + length(CAST(COALESCE(notes, '') AS BLOB)) + length(CAST(COALESCE(error_kind, '') AS BLOB))` in the SQL projection, so non-diff columns are byte-counted accurately. Option (b) requires no write-time constraint but makes the SQL projection slightly heavier. Note: the `notes` column currently lacks any size enforcement; this should be documented as a known gap even if the fix is deferred.

---

### F1004 — MEDIUM — The startup chain-verify paginated loop is specified as `ORDER BY rowid ASC` with batch 500, but `record` queries `SELECT prev_hash FROM audit_log ORDER BY rowid DESC LIMIT 1` at write time; these two orderings can diverge in WAL mode if a concurrent writer commits between startup batches

**Category:** Race condition

**Attack:** The startup guard calls `chain::verify` via a paginated `ORDER BY rowid ASC` loop while the `AuditWriter` is simultaneously available for writes (the design specifies "broken chain logs error, daemon starts" — meaning the daemon does not wait for verification to complete before accepting writes). During the verify loop, between reading batch K and batch K+1:

1. `chain::verify` has read rows 1–K*500 and accumulated `last_computed_hash = H_{K*500}`.
2. A new write via `record` reads `SELECT prev_hash FROM audit_log ORDER BY rowid DESC LIMIT 1` — this returns row K*500's hash, correctly.
3. `record` inserts row K*500+1 with `prev_hash = sha256(canonical_json(row_{K*500})) = H_{K*500}`.
4. `chain::verify` reads batch K+1 (rows K*500+1 to K*1000+500). Row K*500+1 has `prev_hash = H_{K*500}`. The verifier checks: does `prev_hash == last_computed_hash`? Yes. Chain OK.

This is actually safe in the normal case. The race concern is subtler: if `chain::verify` emits `tracing::error!` on a broken link at row 200 and then the daemon starts accepting writes at row N=5000 (the current tip), the startup error log states "chain broken at row 200" — but subsequent writes build a valid chain from row 5000 forward. On the next startup, `chain::verify` will again see the break at row 200 and log the error. The daemon starts every time with a permanent error log entry that operators cannot remediate (rows are immutable). This is a known design trade-off, but the design does not specify whether the broken-chain log is emitted ONCE at first detection and suppressed thereafter (by storing the break point), or emitted on EVERY startup.

If emitted every startup, operators on a long-running system with a single historical chain break accumulate an ever-growing alarm that they cannot silence. Eventually operators ignore the "chain broken" log because it is always present, defeating the detection purpose entirely (the "boy who cried wolf" failure mode).

**Why the design doesn't prevent it:** The startup guard task specifies "broken chain logs `tracing::error!`, daemon starts" with no mention of deduplication, suppression, or whether the break point is persisted to prevent repeated false alarming. The design also does not address whether a break detected at startup means "tampered" or "pre-Phase-6 gap" — the ZERO_SENTINEL logic handles the pre-migration case, but a break in the Phase-6 section (after the first real `prev_hash`) is indistinguishable from a bug in `record` vs. an actual tamper.

**Mitigation required:** Add to the startup guard task: "If `chain::verify` returns `Err(ChainBroken { row_id, … })`, log `tracing::error!(chain_break_at = %row_id, "audit: chain integrity violation detected at startup")`. The daemon continues to start (existing behaviour). No deduplication or suppression is required in Phase 6 — but document in `core/README.md` that this log may recur on every startup until the break is investigated. Operators who have confirmed the break is a known historical gap (e.g., from a pre-Phase-6 migration issue) must note this externally; the audit log itself cannot be updated. Phase 11 may add a `chain_exceptions` table to suppress known breaks without modifying the immutable rows." This is a documentation-only change, but without it operators will not know why the error recurs.

---

### F1005 — MEDIUM — The `per_row_overhead = 512` constant in the audit query API is a magic number defined nowhere; an implementer will choose a different value, and two implementations will produce incompatible pagination boundaries

**Category:** Documentation trap

**Attack:** The design specifies the 10 MB cap as: "accumulate a running byte total with a fixed per-row overhead of 512 bytes; stop fetching rows when the total exceeds 10 MB." The value `512` appears only in the prose of the design doc. It is not defined as a named constant in any code task. An implementer will write `const PER_ROW_OVERHEAD: usize = 512` or `let overhead = 512usize` in their implementation — or may use a different estimate (256, 1024, etc.) based on their own reading of the schema. A future implementer of a compatible client or a second AuditWriter implementation will choose independently. Two implementations with different overhead values produce different pagination boundaries for identical datasets, making query results non-reproducible across implementations.

**Why the design doesn't prevent it:** The design specifies the value as prose rather than as a named constant requirement. No task says "define `pub const AUDIT_QUERY_ROW_OVERHEAD_BYTES: usize = 512` in [location]."

**Mitigation required:** Add to the audit query API task: "The per-row overhead MUST be a named constant `pub const AUDIT_QUERY_ROW_OVERHEAD_BYTES: usize = 512` defined in `crates/adapters/src/audit_query.rs` (or equivalent module). The integration test for the size-truncation path MUST reference this constant by name, not the literal `512`, so that a change to the constant is immediately reflected in the test." This is a two-line addition that eliminates the magic-number ambiguity.

---

### F1006 — LOW — `AuditWriter::record` step ordering places the `SecretsRevealed` guard (step 1) before the mutex lock (step 2), but after adding the `occurred_at_ms > 0` guard (step R9-F902); the ordering of early-return guards is unspecified; a transposed implementation could take the mutex before validating, holding it during validation and making every zero-timestamp call contend with valid writes

**Category:** Logic flaw

**Attack:** The design specifies the following steps for `AuditWriter::record`:
(1) if `row.kind == AuditEvent::SecretsRevealed`, return `Err(AuditError::SecretsRevealedNotYetSupported)`
(2) lock mutex
(3) borrow `conn = guard.as_mut().ok_or(AuditError::ConnectionLost)?`
(4) `BEGIN IMMEDIATE`
...

The `occurred_at_ms > 0` guard was added to the `AuditRow` task (R9-F902 closure) but its position in the step sequence was not specified relative to the mutex lock. An implementer may reason: "I should validate everything before taking the mutex, so I add the timestamp guard before step (1)." Or: "I validate inside the lock to ensure atomicity with the write." If the timestamp guard is placed after step (2) (mutex locked), a caller that repeatedly passes `occurred_at_ms = 0` acquires and releases the `Mutex` for each rejected call — serialising the rejections through the write path and blocking valid concurrent writes during the (cheap) validation.

This is unlikely to cause a production outage, but the design's step sequence has two unordered guards with no stated placement rationale, creating implementation ambiguity.

**Why the design doesn't prevent it:** The `AuditRow` task specifies the `occurred_at_ms > 0` guard as a contract on `record` without specifying it in the numbered step sequence of the `AuditWriter` task. The two tasks are partially overlapping in their specification of `record`'s behaviour.

**Mitigation required:** Add a step (0) to the `AuditWriter::record` numbered sequence: "(0) Pre-lock validation: if `row.kind == AuditEvent::SecretsRevealed`, return early; if `row.occurred_at_ms <= 0`, return `Err(AuditError::InvalidTimestamp { occurred_at_ms: row.occurred_at_ms })`. Both guards are checked before acquiring the mutex, so invalid inputs do not contend with valid concurrent writes." This one-sentence clarification unifies the two tasks' guard specifications into the canonical step sequence.

---

## No findings (categories with nothing concrete to raise)

- **Authentication & authorization** — No new auth surface. `AuditWriter` remains server-internal.
- **Abuse cases** — Rate limiting is out of scope for an append-only log; 10 MB cap + per-row overhead close the main vector (with F1003 noting the notes-column gap). No new abuse path found beyond those findings.
- **Race conditions** — `Mutex<Option<SqliteConnection>>` + `BEGIN IMMEDIATE` closes concurrent-write races (R3-F201 closed). F1004 raises the startup alarm recurrence issue but not a new correctness race.
- **Single points of failure** — Connection recovery (R5-F402, R6-F505, R7-F604) addresses permanent connection loss. No new SPOF found.
- **Rollbacks** — Audit writes are intentionally out-of-band; no rollback semantics for audit rows by design.
- **Orphaned data** — Immutability triggers prevent cleanup by design; no new orphan accumulation path.
- **Migration hazards** — Migration step order (ALTER → UPDATE → trigger CREATE) is correctly sequenced. No partial-apply hazard under transactional SQLite DDL.
- **State machine violations** — Migration sequence 0001–0006 is well-ordered; no state machine gap found.
- **Eventual consistency** — Single-process SQLite; no multi-store consistency gap.
- **Timeouts & retries** — `busy_timeout = 5000` closes the indefinite-stall path (R8-F801). No retry loop introduced.

---

## Verdict

Not yet ready — 2 blocker(s) remain (F1001, F1002).

F1001 and F1002 both describe scenarios where `canonical_json` produces a byte sequence that diverges from the stored DB column value for `kind` or `id`, causing `chain::verify` to produce false chain-break reports on reads from the raw DB — either immediately (if serde is used naively) or after a crate upgrade. Because audit rows are immutable, any canonicalisation mismatch baked into the first production deployment is permanent and uncorrectable. These must be specified before implementation of `canonical_json` begins.
