# Adversarial Review — Phase 6 — Round 13

**Design summary:** Phase 6 adds a tamper-evident, append-only audit log to Trilithon: an `AuditWriter` with a dedicated serialised SQLite connection, a SHA-256 `prev_hash` chain, a `RedactedDiff` newtype for secrets-safe diffs, an `Actor` enum with `from_kind_id` reverse-mapping, startup integrity checks, and a query API with a 10 MB soft cap.

**Prior rounds:** Rounds 1–12 reviewed. All R12 findings (H1 `prev_hash` bind ambiguity, H2 `Actor` deserialization, M1 `FromStr` mechanism) are closed in the current design text. R11-F1103 and R11-F1104 are also closed. No open items carried forward from prior rounds.

---

## Round 12 Closure Table

| ID | Severity | Title | Status |
|----|----------|-------|--------|
| R12-H1 | HIGH | `AuditRow.prev_hash` bind ambiguity | CLOSED — Design Decisions §R12-H1 and `record` step 7 now explicitly state "bind `new_prev_hash` (step 6) to the `prev_hash` column — NOT `row.prev_hash`" |
| R12-H2 | HIGH | `Actor` deserialization path unspecified | CLOSED — `Actor` task now mandates `fn from_kind_id(kind: &str, id: &str) -> Result<Actor, AuditError>` with full reverse-mapping table; `String` (not `&'static str`) for `System { component }` |
| R12-M1 | MEDIUM | `AuditEvent FromStr` mechanism ambiguous | CLOSED — Design now mandates `strum::EnumString` + `strum::EnumIter`; round-trip test uses `AuditEvent::iter()` (exhaustive by construction) |
| R11-F1103 | MEDIUM | `canonical_json` actor encoding contradicted `AuditRow.actor: Actor` | CLOSED — `canonical_json` task now specifies "extract `actor_kind`/`actor_id` via `row.actor.to_kind_id()`"; stability test (criterion h) asserts the values match |
| R11-F1104 | MEDIUM | `canonical_json` `Option<RedactedDiff>` encoding unspecified | CLOSED — task now specifies "use `row.redacted_diff.as_ref().map(RedactedDiff::as_str)` and encode as JSON string or null — do NOT parse as nested object" |

---

## Findings

### HIGH — `record` step 5 fetches only `SELECT prev_hash … LIMIT 1` but step 6 implicitly requires `canonical_json(predecessor)` which needs all predecessor columns; the two steps are incompatible and produce two divergent chain implementations

**Category:** Logic flaw / documentation trap

**Trigger:** `AuditWriter::record` specifies a numbered step sequence. Step 5: "query `SELECT prev_hash FROM audit_log ORDER BY rowid DESC LIMIT 1`." Step 6: "compute `new_prev_hash`." The general chain description says "Newly inserted rows: `prev_hash = sha256(canonical_json(predecessor))`."

`canonical_json(predecessor)` requires a fully-populated `AuditRow` — it includes every column: `id`, `occurred_at_ms`, `actor`, `kind`, `correlation_id`, `redacted_diff`, `outcome`, `notes`, `prev_hash`, etc. Step 5 retrieves exactly one column of the predecessor: `prev_hash`. The other columns are not fetched.

An implementer reading steps 5 and 6 in sequence reaches one of two conclusions:
- (a) Step 6 means `new_prev_hash = sha256(prev_hash_string_from_step5)` — hash the predecessor's `prev_hash` field value directly (a chain of hashes of hashes). This is consistent with what step 5 provides.
- (b) Step 6 means `new_prev_hash = sha256(canonical_json(predecessor_full_row))` — consistent with the general description but inconsistent with step 5, which does not fetch the full row.

A developer who implements (a) and a developer who implements (b) produce entirely different `prev_hash` values for every row. `chain::verify` in both codebases uses `canonical_json(full_row)` (since `chain::verify` operates on a complete iterator of `AuditRow`), so only implementation (b) would actually verify correctly. Implementation (a) would produce a chain that is internally self-consistent but where `chain::verify` reports `ChainBroken` on the second row.

Because the `canonical_json` task says to include `prev_hash` in the JSON object, and because `chain::verify` calls `canonical_json(row)` on full rows, option (b) is the intended interpretation. But the step 5 query does not support it — step 5 would need to be `SELECT * FROM audit_log ORDER BY rowid DESC LIMIT 1` (or an equivalent full-row fetch) to allow `canonical_json(predecessor)` to be computed.

**Consequence:** Two correct-looking implementations diverge on chain construction from the very first write. The `chain::verify` test (criterion b: "chain intact after ten appended rows") would pass for implementation (b) but produce `Err(ChainBroken)` on the second row for implementation (a). Because rows are immutable, a production deployment running implementation (a) cannot be corrected without dropping the entire chain.

**Design assumption violated:** The design assumes that "compute `new_prev_hash`" in step 6 is unambiguous given only step 5's output, but step 5 provides a single string value while `canonical_json(predecessor)` requires the full predecessor row.

**Suggested mitigation:** Replace step 5 with: "query `SELECT id, caddy_instance_id, correlation_id, occurred_at_ms, actor_kind, actor_id, kind, target_kind, target_id, snapshot_id, redacted_diff_json, redaction_sites, outcome, error_kind, notes, prev_hash FROM audit_log ORDER BY rowid DESC LIMIT 1` and reconstruct a predecessor `AuditRow` from the result; if no row exists, `new_prev_hash = ZERO_SENTINEL`." Then step 6: "compute `new_prev_hash = sha256(canonical_json(predecessor_row))`." Alternatively, if a full-row fetch in the hot path is undesirable, replace `prev_hash = sha256(canonical_json(predecessor))` with `prev_hash = sha256(predecessor.prev_hash || sha256(canonical_json(row_being_inserted)))` and document the simplified formula — but this would require updating `chain::verify`'s algorithm and all existing test criteria.

---

### MEDIUM — `canonical_json` JSON key for `redacted_diff` is never specified; DB column is `redacted_diff_json` but `AuditRow` field is `redacted_diff`; an implementer choosing either produces a different SHA-256 hash

**Category:** Documentation trap

**Trigger:** `canonical_json(row: &AuditRow) -> Vec<u8>` must include "every column of `audit_log`" with sorted keys. The DB column name is `redacted_diff_json`. The `AuditRow` struct field is `redacted_diff`. The design spec for `canonical_json` refers to the field as `redacted_diff` when describing the encoding rule ("use `row.redacted_diff.as_ref().map(RedactedDiff::as_str)` and encode the resulting `Option<&str>` as a JSON string or null") but says the function should include "every column of `audit_log`" — which would suggest the key should be `"redacted_diff_json"` to match the column name.

The design never states which name to use as the JSON object key. An implementer who matches struct field names uses `"redacted_diff"`. An implementer who matches DB column names uses `"redacted_diff_json"`. Both are equally defensible given the spec.

Concrete consequence: with lexicographic key sorting, both keys sort into the `"r"` range — but `"redacted_diff"` (13 chars) sorts before `"redaction_sites"` (15 chars), while `"redacted_diff_json"` (18 chars) also sorts before `"redaction_sites"`. However, `"redacted_diff"` sorts before `"redacted_diff_json"`. An implementation using `"redacted_diff"` produces a different byte sequence — and therefore a different SHA-256 hash — than one using `"redacted_diff_json"`. Every `prev_hash` in one deployment is invalid from the perspective of the other.

The stability test (criterion h) asserts byte-for-byte repeatability across multiple calls in the same binary — it does not assert the specific key names used, so neither implementation fails the test.

**Design assumption violated:** The design assumes that "every column of `audit_log`" as the key convention is unambiguous, but there is a split between the DB column name and the Rust struct field name for this one column.

**Suggested mitigation:** Add a single sentence to the `canonical_json` spec: "JSON object keys MUST match the `audit_log` column names exactly — not the `AuditRow` field names. The key for `redacted_diff: Option<RedactedDiff>` MUST be `\"redacted_diff_json\"`, matching the DB column." Alternatively, rename the `AuditRow` field to `redacted_diff_json: Option<RedactedDiff>` to eliminate the discrepancy at the type level. Add a stability test assertion: "the output of `canonical_json` contains the key `\"redacted_diff_json\"` (not `\"redacted_diff\"`)."

---

### LOW — `canonical_json` for ZERO_SENTINEL rows at startup uses `row.prev_hash = ZERO_SENTINEL` as part of the hash input; a pre-migration row that was tampered and then its `prev_hash` column backfilled to ZERO_SENTINEL via a different mechanism evades detection

**Category:** Assumption violation

**Trigger:** The design states: for rows where `prev_hash == ZERO_SENTINEL`, "compute `sha256(canonical_json(row))` and record it as `last_computed_hash`" — the hash includes the `prev_hash` field itself (value: ZERO_SENTINEL). The intent is that tampering with any field of a pre-migration row (other than `prev_hash`) changes `canonical_json(row)` and therefore changes `last_computed_hash`, which the first non-sentinel row's `prev_hash` would not match.

The attack: an adversary who can access the database directly (the threat model for which the audit chain exists) tampers with a pre-migration row's data columns AND updates `prev_hash` to ZERO_SENTINEL (which the immutability triggers would block — but the triggers are installed in migration 0006 which also backfills `prev_hash = ZERO_SENTINEL`; if the attacker acts before migration 0006 runs, or drops and recreates triggers, they can write any `prev_hash` value including ZERO_SENTINEL). The sentinel row's `canonical_json` now includes the tampered data columns and `prev_hash = ZERO_SENTINEL`, producing a new `last_computed_hash`. The first non-sentinel row's stored `prev_hash` was computed over the original sentinel row. These will disagree — so the tamper is detected.

However: if an attacker tampers with a sentinel row and ALSO tampers with the first non-sentinel row's `prev_hash` to match the new `canonical_json` (possible if the attacker can write arbitrary DB values before the triggers are installed), `chain::verify` passes. The design's threat model states triggers prevent this — but there is a window between migration 0005 and 0006 where no triggers exist.

More concretely and within the design's own scope: the "sentinel rows hash-but-don't-validate" design (R6-F502) means there is no check that any two adjacent sentinel rows' `last_computed_hash` values are consistent with each other. If there are multiple sentinel rows and an attacker reorders two of them (changes the rowid ordering — which is possible via dump-and-restore before migration 0006), the accumulated `last_computed_hash` after the sentinel sequence is different, breaking the first real row's `prev_hash` — but `chain::verify` would report a broken chain, not a tamper on the sentinels themselves. This is the expected detection behaviour, not a bypass.

**Consequence:** This is a low-severity observation rather than a concrete attack path within the threat model. The design correctly detects tampering through the first real row. The window before migration 0006 is a narrow timeline concern, not a runtime gap.

**Design assumption violated:** The design assumes that pre-migration sentinel rows cannot be tampered with and have their `prev_hash` simultaneously reset to ZERO_SENTINEL, because the backfill and trigger-install are atomic in one migration. This assumption holds under normal sqlx migration semantics (each file is a transaction) but is worth documenting.

**Suggested mitigation:** Add to the documentation task: "The ZERO_SENTINEL backfill and trigger installation in migration 0006 are atomic (sqlx wraps each migration file in a transaction by default). An attacker who can write to the DB file directly before migration 0006 has applied can forge sentinel rows — but this is outside the tamper-evidence model, which begins after migration 0006 completes. Document this scope boundary: the audit chain guarantees tamper detection for rows written after migration 0006; pre-migration rows are covered only insofar as tampering them invalidates the first real row's `prev_hash`."

---

## No findings (categories with nothing concrete to raise)

- **Authentication & authorization** — `AuditWriter` is server-internal; no new auth surface added in round 13.
- **Abuse cases** — 10 MB cap (byte-accurate for both `redacted_diff_json` and `notes`), `busy_timeout = 5000`, and max 1000 row limit close the main abuse vectors. No new vector found.
- **Race conditions** — `Mutex<Option<SqliteConnection>>` + `BEGIN IMMEDIATE` serialises all writes. Concurrent-write chain linearisation is specified and tested. No new concurrent race found.
- **Resource exhaustion** — Paginated `chain::verify` (batch 500), 10 MB query cap, `busy_timeout`, and `InvalidTimestamp` guard before mutex lock close the exhaustion paths. No new vector.
- **State machine violations** — Migration 0006 step order (ALTER TABLE → UPDATE → CREATE TRIGGER × 2) is correctly sequenced with the backfill before triggers to avoid the trigger blocking the backfill. No new violation.
- **Error handling gaps** — `BusyTimeout`, `ConnectionLost`, `ConnectionRecoveryFailed`, `InvalidTimestamp`, `SecretsRevealedNotYetSupported`, `TriggersMissing`, `UnknownActorKind`, `EmptyHash`, `ChainBroken` are all specified. No unhandled error path found.
- **Schema/type mismatches** — R11-F1101 (`AuditEvent FromStr`), R11-F1102 (`occurred_at` derivation in `canonical_json`), R11-F1103 (`actor_kind`/`actor_id` encoding), R11-F1104 (`RedactedDiff` encoding) are all closed. No new type mismatch found beyond the key-name finding above.
- **Rollbacks** — Audit writes are intentionally out-of-band from business transactions. No rollback semantics for audit rows by design.
- **Orphaned data** — Immutability triggers prevent cleanup by design. No orphan accumulation path.
- **Migration hazards** — sqlx wraps each migration file in a transaction by default; the four steps in `0006` are atomic. No partial-apply hazard.
- **Single points of failure** — Connection recovery (R5-F402, R6-F505) and `busy_timeout` address SPOF vectors. No new SPOF found.
- **Timeouts & retries** — `PRAGMA busy_timeout = 5000` + `BusyTimeout` error caps the wait; `busy_timeout` test (criterion h) specified. No retry loop introduced.
- **Eventual consistency** — Single-process SQLite; no multi-store consistency gap.
- **Observability gaps** — `tracing::error!` on chain break, `tracing::warn!` on synth correlation IDs, `tracing::info!` on sentinel-row count, and `tracing::error!` on `occurred_at` inconsistency are all specified. No new observability gap.

---

## Summary

**Critical:** 0  **High:** 1  **Medium:** 1  **Low:** 1

**Top concern:** The step 5 / step 6 incompatibility in `record` (High finding) is the most dangerous: step 5 fetches only `SELECT prev_hash … LIMIT 1` while step 6 implicitly requires `canonical_json(predecessor_full_row)`. An implementer who follows the steps literally computes `sha256(prev_hash_string)` rather than `sha256(canonical_json(full_row))`, producing a chain that `chain::verify` reports as broken from the second row. Because rows are immutable, this cannot be corrected in production.

**Recommended action before proceeding:** Address the High finding (step 5 query must fetch all columns, not just `prev_hash`) before implementation begins — this is a blocker because it directly determines whether the chain is constructable. The Medium finding (JSON key name for `redacted_diff_json`) should also be addressed as it produces different SHA-256 values across implementations. The Low finding is a documentation note rather than a blocker.
