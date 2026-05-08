# Adversarial Review — Phase 6 — Round 14

**Design summary:** Phase 6 adds a tamper-evident, append-only audit log to Trilithon: an `AuditWriter` with a dedicated serialised SQLite connection, a SHA-256 `prev_hash` chain, a `RedactedDiff` newtype for secrets-safe diffs, an `Actor` enum with `from_kind_id` reverse-mapping, startup integrity checks, and a query API with a 10 MB soft cap.

**Prior rounds:** Rounds 1–13 reviewed. All R13 findings are closed in the current design text:
- R13-H1 (step 5 query incompatible with `canonical_json`): CLOSED — Design now mandates `SELECT * FROM audit_log ORDER BY rowid DESC LIMIT 1` and a "Design decisions recorded here" entry confirms step 5 fetches all predecessor columns.
- R13-M1 (key name ambiguity for `redacted_diff_json`): CLOSED — `canonical_json` key list now explicitly states `"redacted_diff_json"` (NOT `"redacted_diff"`), and a Design Decisions entry confirms the key matches the DB column name.
- R13-L1 (sentinel-row tampering before migration 0006): Design-documented scope boundary; no further action required.

---

## Round 13 Closure Table

| ID | Severity | Title | Status |
|----|----------|-------|--------|
| R13-H1 | HIGH | `record` step 5 fetches only `prev_hash`; step 6 requires full row for `canonical_json` | CLOSED — step 5 now mandates `SELECT *`; Design Decisions §R13-H1 explicit |
| R13-M1 | MEDIUM | `canonical_json` JSON key name for `redacted_diff` ambiguous | CLOSED — key list mandates `"redacted_diff_json"`; Design Decisions §R13-M1 explicit |
| R13-L1 | LOW | ZERO_SENTINEL sentinel rows: attacker window before migration 0006 | CLOSED (documentation note) |

---

## Findings

### HIGH — `RedactedDiff` has no construction path from a DB-retrieved string; `record` step 5 and the startup paginator cannot reconstruct `AuditRow { redacted_diff: Option<RedactedDiff> }` for rows with non-null `redacted_diff_json`

**Category:** Logic flaw / schema-type mismatch

**Trigger:** The R13-H1 fix requires `record` step 5 to fetch the full predecessor row (`SELECT *`) and reconstruct a complete `AuditRow` to pass to `canonical_json(predecessor_row)`. Separately, the startup paginator must also construct typed `AuditRow` values from DB rows to feed into `chain::verify(rows: impl Iterator<Item = &AuditRow>)`.

Both paths need to construct `AuditRow { redacted_diff: Option<RedactedDiff>, … }` from a raw SQLite row whose `redacted_diff_json` column contains an already-redacted JSON string (e.g., `{"field":"secret:v1:abc...def"}`).

The `RedactedDiff` task explicitly mandates: "No `From<String>`, no public field. `RedactedDiff` MUST expose exactly one public read accessor: `pub fn as_str(&self) -> &str { &self.0 }`." The only specified constructor is `RedactedDiff::new(raw: &serde_json::Value, registry: &SecretFieldRegistry) -> (RedactedDiff, usize)`, which takes a raw unredacted JSON value and a registry, applies redaction, and returns the result. There is no constructor that accepts an already-redacted string from the DB.

Concrete failure sequence: `record` executes step 5 and reads the predecessor row. The predecessor has `redacted_diff_json = '{"password_hash":"secret:v1:a1b2..."}' `. `record` must reconstruct `AuditRow { redacted_diff: Some(?), … }` from this string but has no way to do so — `RedactedDiff::new` requires an unredacted `serde_json::Value` and a registry, not the already-redacted string. The same failure occurs in the startup paginator for every row with a non-null `redacted_diff_json`.

An implementer forced to resolve this gap independently will choose one of three paths: (a) change `AuditRow.redacted_diff` from `Option<RedactedDiff>` to `Option<String>` on the read path — which contradicts the task spec and breaks the type-level redactor guarantee; (b) add a crate-internal constructor `RedactedDiff::from_db(s: String) -> RedactedDiff` gated behind a module-visibility boundary — which the task spec does not specify and may violate the intended opacity invariant; (c) define a separate "read model" struct with `redacted_diff_json: Option<String>` and a separate `canonical_json_read_model` function — which introduces the possibility of divergence between write-time and verify-time `canonical_json` implementations. Any of these paths taken independently by different implementers produces different `canonical_json` output for the same DB row, and because the rows are immutable, any divergence permanently corrupts the chain.

**Consequence:** Without a specified DB reconstruction path for `RedactedDiff`, `record` step 5 and the startup paginator cannot be implemented in a way that is consistent with the `AuditRow` type signature. Implementers will independently invent incompatible solutions, producing `canonical_json` divergence between the write path and the verify path. On an immutable log, any such divergence is permanent and uncorrectable.

**Design assumption violated:** The R13-H1 fix assumed that fetching `SELECT *` and reconstructing a `AuditRow` was straightforward, but the `RedactedDiff` opacity invariant makes the reconstruction of `Option<RedactedDiff>` from a raw DB string impossible with the currently specified API surface.

**Suggested mitigation:** Add to the `RedactedDiff` task a module-internal (not public) constructor: "A `pub(crate)` constructor `RedactedDiff::from_db_str(s: String) -> RedactedDiff` MUST be defined in `crates/core/src/audit.rs` for use exclusively by the startup paginator and by `record` step 5 when reconstructing a predecessor `AuditRow`. This constructor does NOT validate or re-redact; it wraps the already-stored string. Its visibility MUST be `pub(crate)` — not `pub` — to prevent callers outside `core` from constructing unredacted `RedactedDiff` values. It MUST NOT appear in the public doc. The existing type-level redactor guarantee is preserved because `AuditWriter::record` (in `adapters`) still only accepts `AuditRow { redacted_diff: Option<RedactedDiff> }` values produced by `RedactedDiff::new` at the call site — `from_db_str` is only used on the predecessor read path, never on the row being written." Alternatively, split `AuditRow` into write and read types: `AuditRowWrite { redacted_diff: Option<RedactedDiff>, … }` (no `prev_hash`, constructor-enforced) and `AuditRowStored { redacted_diff_json: Option<String>, prev_hash: String, … }` (DB read model, passed to `canonical_json` via a companion function). Document which type each code path uses.

---

### MEDIUM — `AuditOutcome` deserialization from DB TEXT is unspecified; `record` step 5 and the startup paginator cannot reconstruct `AuditRow { outcome: AuditOutcome }` from the raw `"ok"` / `"error"` / `"denied"` DB values

**Category:** Documentation trap / schema-type mismatch

**Trigger:** The design mandates `#[serde(rename_all = "lowercase")]` on `AuditOutcome` to ensure serialization to `"ok"`, `"error"`, `"denied"`. It does not specify a `FromStr`, `TryFrom<&str>`, or `serde::Deserialize` implementation for the reverse direction. After the R13-H1 fix, `record` step 5 fetches the predecessor row's `outcome TEXT` column (value: `"ok"`, `"error"`, or `"denied"`) and must reconstruct `AuditOutcome` from it to build the typed `AuditRow { outcome: AuditOutcome }` for `canonical_json`. The startup paginator has the same requirement.

The design has explicitly specified deserialization for every other typed field that requires DB reconstruction: `AuditEvent` via `strum::EnumString` (R11-F1101), `Actor` via `from_kind_id` (R12-H2). `AuditOutcome` is the remaining typed enum without a specified deserialization path. An implementer can reasonably:

- (a) Derive `strum::EnumString` with `#[strum(serialize_all = "lowercase")]` — correct output, not specified.
- (b) Write a manual `match "ok" => Ok(AuditOutcome::Ok)` arm — works but is a separate code path from the serde serialization.
- (c) Derive `serde::Deserialize` with the same `rename_all = "lowercase"` attribute — correct, and natural alongside the existing Serialize derive, but not specified.

If two implementers choose different approaches and make independent errors (e.g., one uses `"OK"` instead of `"ok"` in a manual match), `canonical_json(predecessor)` encodes the wrong outcome string. The chain hash computed by `record` and the hash computed by `chain::verify` diverge for any predecessor row with `outcome = "error"` or `outcome = "denied"`, producing permanent false `ChainBroken` errors.

**Consequence:** Implementation-defined deserialization of a 3-variant enum is unlikely to diverge catastrophically, but the inconsistency with the design's explicit treatment of `AuditEvent` and `Actor` deserialization leaves a documentation gap. A typo in a manual match (e.g., `"Error"` vs `"error"`) produces the same class of permanent chain corruption as R11-F1101 did for `AuditEvent`.

**Design assumption violated:** The design assumes `AuditOutcome` reconstruction from DB TEXT is obvious, but the pattern of explicit deserialization specs for `AuditEvent` and `Actor` establishes a precedent that all typed enum fields require specification. `AuditOutcome` is the outlier.

**Suggested mitigation:** Add to the `AuditRow` task (or to the `AuditWriter` task alongside the existing `#[serde(rename_all = "lowercase")]` requirement): "`AuditOutcome` MUST also derive `serde::Deserialize` with `#[serde(rename_all = \"lowercase\")]`, or derive `strum::EnumString` with `#[strum(serialize_all = \"lowercase\")]`, so that DB TEXT values `\"ok\"`, `\"error\"`, `\"denied\"` deserialize back to the correct variant. The round-trip test MUST assert: `for outcome in [AuditOutcome::Ok, AuditOutcome::Error, AuditOutcome::Denied] { let s = serde_json::to_string(&outcome).unwrap(); assert_eq!(serde_json::from_str::<AuditOutcome>(&s).unwrap(), outcome); }`." Two lines of addition close the gap.

---

### LOW — `canonical_json` stability test (criterion h) does not assert the `outcome` field encoding; a serde misconfiguration on `AuditOutcome` that produces `"Ok"` instead of `"ok"` passes the stability test silently

**Category:** Test coverage gap

**Trigger:** The stability test (criterion h) for `canonical_json` mandates assertions for: `kind` field equals `row.kind.to_string()`, `id` has length 26, `occurred_at == occurred_at_ms / 1000`, `actor_kind`/`actor_id` match `to_kind_id()`, `redacted_diff_json` key is present, `redacted_diff_json` value is a JSON string. `outcome` is not in this list.

An implementer who accidentally derives `#[derive(Serialize)]` on `AuditOutcome` without `#[serde(rename_all = "lowercase")]` will produce `"Ok"`, `"Error"`, `"Denied"` (PascalCase) in `canonical_json`. The stability test calls `canonical_json` twice on the same `AuditRow` and asserts byte-for-byte equality — both calls produce the same (wrong) `"Ok"` string. The stability test passes. The `AuditOutcome` CHECK constraint violation is caught at INSERT time (the CHECK only applies to the DB column, not to `canonical_json`'s output), but `canonical_json` in the chain hash uses `"Ok"` — which is different from the DB column value `"ok"`. A future tool that recomputes `canonical_json` from DB rows (using `"ok"` from the TEXT column) would disagree with the stored hashes.

This is not a production-blocking finding — the CHECK constraint ensures the DB column value is always `"ok"`, `"error"`, or `"denied"`, and as long as both `record` and `chain::verify` use the same (potentially wrong) `AuditOutcome` serialization, the chain verifies internally. The concern is an external verifier or a future cross-build comparison.

**Design assumption violated:** The design assumes the stability test (criterion h) is a comprehensive guard against `canonical_json` serialization errors, but it omits the `outcome` field, which has a known class of potential misconfiguration (the serde rename issue already addressed for the DB INSERT in R7-F601).

**Suggested mitigation:** Add to the stability test acceptance criterion (h): "MUST also assert that the `outcome` field in the canonical JSON output equals `row.outcome.to_string()` (or equivalent lowercase string — one of `\"ok\"`, `\"error\"`, `\"denied\"`)." This one assertion catches the serde-without-rename misconfiguration on `AuditOutcome` at test time rather than in production.

---

## No findings (categories with nothing concrete to raise)

- **Authentication & authorization** — `AuditWriter` is server-internal with no new auth surface in round 14.
- **Abuse cases** — 10 MB cap (byte-accurate for `redacted_diff_json` and `notes`), `busy_timeout = 5000`, and max 1000 row limit close the main abuse vectors. No new vector found.
- **Race conditions** — `Mutex<Option<SqliteConnection>>` + `BEGIN IMMEDIATE` serialises all writes. Concurrent-write chain linearisation is specified and tested. No new race found.
- **Resource exhaustion** — Paginated `chain::verify` (batch 500), 10 MB query cap, `busy_timeout`, and `InvalidTimestamp` guard before mutex lock close the exhaustion paths. No new vector.
- **State machine violations** — Migration 0006 step order (ALTER TABLE → UPDATE → CREATE TRIGGER × 2) is correctly sequenced. The R13-H1 fix (full predecessor fetch) does not introduce new state machine gaps.
- **Rollbacks** — Audit writes are intentionally out-of-band from business transactions. No rollback semantics for audit rows by design.
- **Orphaned data** — Immutability triggers prevent cleanup by design. No orphan accumulation path.
- **Migration hazards** — sqlx wraps each migration file in a transaction by default; the four steps in `0006` are atomic. No partial-apply hazard.
- **Single points of failure** — Connection recovery (R5-F402, R6-F505) and `busy_timeout` address SPOF vectors. The R13 fix does not add new SPOF.
- **Timeouts & retries** — `PRAGMA busy_timeout = 5000` + `BusyTimeout` error caps the wait. No retry loop introduced.
- **Eventual consistency** — Single-process SQLite; no multi-store consistency gap.
- **`prev_hash` chain correctness** — R13-H1 and R13-M1 are closed; `canonical_json` now has a complete and unambiguous specification for all fields except `outcome` (Low finding above) and the `RedactedDiff` reconstruction path (High finding above). The chain logic itself is structurally sound.
- **Observability gaps** — `tracing::error!` on chain break, `tracing::warn!` on synth correlation IDs, `tracing::info!` on sentinel-row count, and `tracing::error!` on `occurred_at` inconsistency are all specified. No new observability gap.

---

## Summary

**Critical:** 0  **High:** 1  **Medium:** 1  **Low:** 1

**Top concern:** The `RedactedDiff` DB reconstruction path (High finding) is the most structurally dangerous finding in Round 14: the R13-H1 fix (fetching `SELECT *` for the predecessor row) created a requirement for `record` step 5 and the startup paginator to construct `AuditRow { redacted_diff: Option<RedactedDiff> }` from raw DB strings, but `RedactedDiff` has no constructor from an already-redacted DB string. An implementer forced to resolve this gap independently will produce a solution that either breaks the type-level redactor guarantee, introduces a second struct that may diverge from the spec, or silently changes the type of `redacted_diff` on the read path — any of which can cause `canonical_json` to produce different bytes at write time vs. verify time on an immutable log.

**Recommended action before proceeding:** Address the High finding (specify a `pub(crate) RedactedDiff::from_db_str` constructor or a separate read-model struct) before implementation begins — this is a blocker because the lack of a specified reconstruction path will force every implementer to invent an incompatible solution. The Medium finding (`AuditOutcome` deserialization) should also be resolved as it is a two-line addition that closes the last unspecified enum deserialization gap. The Low finding (stability test coverage for `outcome`) is a one-line test addition and is not a blocker.
