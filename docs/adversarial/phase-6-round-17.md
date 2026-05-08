# Adversarial Review — Phase 6 — Round 17

**Design summary:** Phase 6 adds a tamper-evident, append-only audit log to Trilithon: an `AuditWriter` with a dedicated serialised SQLite connection, a SHA-256 `prev_hash` chain, a `RedactedDiff` newtype for secrets-safe diffs, an `Actor` enum with `from_kind_id` reverse-mapping and an unknown-kind fallback, startup integrity checks, and a query API with a 10 MB soft cap.

**Prior rounds:** Rounds 1–16 reviewed. All R16 findings are closed in the current design text:
- R16-H1 (startup paginator `Actor::from_kind_id` fallback unspecified): CLOSED — design at lines 45 and 217 now mandates both call sites apply the identical `"__unknown:{kind}"` fallback sentinel.
- R16-M1 (`from_db_str` call-site grep recipe informational only): CLOSED — the companion `grep-from-db-str-callers` recipe is specified in the retire-`record_audit_event` task.
- R16-M2 (`SELECT *` vs named projection to prevent `occurred_at` field on `AuditRow`): CLOSED — design notes mandate named column list in startup paginator; `AuditRow` has no `occurred_at` field.
- R16-L1 (`"unknown"` fallback collides with legitimate `System { component: "unknown" }`): CLOSED — fallback changed to `"__unknown:{kind}"` sentinel encoding the original kind string.

No open items carried forward from prior rounds.

---

## Round 16 Closure Table

| ID | Severity | Title | Status |
|----|----------|-------|--------|
| R16-H1 | HIGH | Startup paginator `Actor::from_kind_id` fallback unspecified; hash diverges from write path | CLOSED — both `record` step 5 and startup paginator now specify identical `"__unknown:{kind}"` fallback |
| R16-M1 | MEDIUM | `RedactedDiff::from_db_str` call sites outside `adapters` undetected by CI | CLOSED — `grep-from-db-str-callers` informational recipe specified; `from_db_str` call sites are now auditable in code review |
| R16-M2 | MEDIUM | `SELECT *` in startup paginator may cause `occurred_at` field to be added to `AuditRow`, corrupting hashes | CLOSED — design mandates named column list (not `SELECT *`) in startup paginator; `AuditRow` must not carry `occurred_at` |
| R16-L1 | LOW | `"unknown"` fallback for unrecognized `actor_kind` collides with legitimate `System { component: "unknown" }` | CLOSED — fallback changed to `"__unknown:{kind}"` to be non-colliding and forensically identifiable |

---

## Findings

### HIGH — Startup paginator has no specified fallback for unrecognized `AuditEvent` kind strings; a future-phase row permanently breaks chain verification

**Category:** Logic flaw / documentation trap

**Trigger:** R16-H1 closed the `Actor::from_kind_id` unknown-kind fallback for the startup paginator by specifying that both `record` step 5 and the paginator apply `Actor::System { component: format!("__unknown:{}", kind) }` when `Actor::from_kind_id` returns `Err`. This ensures `canonical_json` produces the same hash for the same predecessor row on both the write path and the verify path, even for rows with unrecognized `actor_kind` values.

The same problem exists for the `AuditEvent` (the `kind` column) — and the design does not address it. The startup paginator reconstructs a typed `AuditRow { kind: AuditEvent, … }` from each DB row. Reconstruction requires parsing the stored `kind` TEXT string using `AuditEvent::from_str(kind_str)` (via `strum::EnumString`). `AuditEvent` is a closed enum: `from_str` returns `Err(strum::ParseError)` for any string not corresponding to one of the 44 enumerated variants.

Concrete failure sequence: Phase 7 (or any subsequent phase) adds a new audit event kind — e.g., `AuditEvent::SystemRestoreApplied` with display string `"system.restore-applied"`. Phase 7 writes audit rows with `kind = "system.restore-applied"` to `audit_log`. Phase 6's `AuditWriter::record` step 5 reads such a row as the predecessor and must reconstruct `AuditRow { kind: AuditEvent, … }`. `AuditEvent::from_str("system.restore-applied")` returns `Err`. The design specifies no handling for this case in `record` step 5. At startup, the paginator encounters the same row: `AuditEvent::from_str("system.restore-applied")` returns `Err`, with no specified fallback. The paginator must either: (a) propagate the error and abort chain verification entirely — `chain::verify` never returns; or (b) each implementer independently invents a fallback without specification.

If the paginator propagates the error (path a), the daemon starts (the startup guard does not block on chain errors) but chain verification is permanently non-functional for any database that has ever received a row with an unrecognized `kind` — which is every production database after Phase 7 or later deploys.

If the paginator invents its own fallback (path b), `canonical_json` uses `row.kind.to_string()` to encode the kind in the hash. Any fallback that does not preserve the original kind string exactly will produce a different hash than what `record` step 5 computed. For example: if step 5 also independently invents a fallback (`AuditEvent::MutationProposed` as a placeholder) and the paginator independently invents a different fallback (skipping the row), the hash for every row following the unrecognized kind diverges, and `chain::verify` returns `Err(ChainBroken)` for every subsequent row — a permanent false alarm on every daemon restart.

This is structurally identical to R16-H1 for `Actor`, but for `AuditEvent`. R16-H1 was a blocker. This is equally blocking.

**Consequence:** After any phase that adds new `AuditEvent` variants, `chain::verify` at startup either fails entirely or produces permanent false `ChainBroken` alarms. The log's tamper-evidence guarantee becomes useless as operators become desensitised to persistent startup alarms. If the paginator propagates the error and aborts, the daemon reports startup chain verification failure on every restart in perpetuity.

**Design assumption violated:** The design assumes the only `kind` values ever stored in `audit_log` are those corresponding to Phase 6's 44 `AuditEvent` variants. It does not account for forward-compatibility with rows written by future phases that extend the enum. The parallel fallback for `Actor::from_kind_id` (R16-H1) was closed but the symmetric gap for `AuditEvent::from_str` was not identified.

**Suggested mitigation:** Add to the `chain::verify` / startup paginator specification: "The startup paginator MUST apply a fallback when `AuditEvent::from_str(kind_str)` returns `Err`: emit `tracing::warn!(kind = %kind_str, row_id = %id, 'audit paginator: unrecognized kind string in row; using raw-string fallback for canonical_json')` and store the raw kind string directly in a fallback representation for `canonical_json`. Since `canonical_json` encodes `kind` as `row.kind.to_string()`, the fallback MUST produce a `to_string()` output equal to the original `kind_str`. The recommended implementation: add a variant `AuditEvent::Unknown(String)` with `#[strum(disabled)]` so it is excluded from `EnumIter` and `EnumCount`, and implement `Display` for it as `write!(f, '{}', self.0)`. On `from_str` failure, construct `AuditEvent::Unknown(kind_str.to_owned())`. This preserves the original string through `canonical_json` exactly, ensuring hash stability." Alternatively: define `canonical_json` to encode `kind` as a raw `&str` taken from a new method `AuditRow::kind_str() -> &str` that returns either `self.kind.to_string()` (for known kinds) or the raw DB string (for unknown kinds), stored as a separate field on `AuditRow` for the read path. Either way, add a test: inject a row with `kind = "synthetic.unknown-kind"` via raw SQL; write a subsequent row normally; assert that `chain::verify` over both rows returns `Ok(())`.

---

### MEDIUM — The ZERO_SENTINEL multi-row tamper-detection guarantee is overstated; only the last sentinel row's content is covered by the first chained row's `prev_hash`

**Category:** Logic flaw / documentation trap

**Trigger:** The `chain::verify` task specifies for ZERO_SENTINEL rows: "compute `sha256(canonical_json(row))` and record it as `last_computed_hash`." This sets `last_computed_hash` to the hash of the current sentinel row. After N sentinel rows, `last_computed_hash` equals `sha256(canonical_json(row_N))` — it is overwritten, not accumulated. No information about rows 1 through N-1 flows into `last_computed_hash`.

The design then asserts: "The first non-sentinel row's `prev_hash` MUST equal the `last_computed_hash` accumulated through the sentinel sequence — this ensures a tampered pre-migration row is detected by the first chained row."

This claim is false for any database with more than one sentinel row. Concrete scenario: a database has 100 pre-migration rows (all backfilled to `prev_hash = ZERO_SENTINEL` by migration 0006), then one Phase-6 row. `chain::verify` processes:
- Rows 1–99: each sets `last_computed_hash = sha256(canonical_json(row_k))` in turn. After row 99, `last_computed_hash = sha256(canonical_json(row_99))`.
- Row 100 (sentinel): `last_computed_hash = sha256(canonical_json(row_100))`.
- Row 101 (Phase-6 row): its `prev_hash` must equal `last_computed_hash = sha256(canonical_json(row_100))`. This is verified correctly.

An attacker who modifies rows 1 through 99 (any pre-migration row except the last) passes `chain::verify` without detection. Only row 100's content is protected. The other 99 rows can be arbitrarily altered.

The design's documentation ("accumulated through the sentinel sequence") implies a chained or accumulating hash, but the algorithm is just repeated overwrite. The word "accumulated" is incorrect — there is no accumulation.

**Consequence:** Operators and auditors who read the design documentation may believe that all pre-migration rows are protected by the hash chain. In reality, only the most recent sentinel row (the last pre-migration row) is covered. An attacker with write access to the SQLite file can alter any pre-migration audit rows except the last one without triggering `chain::verify`. This is a weaker security guarantee than stated, affecting every production deployment that had audit rows before migration 0006 was applied.

**Design assumption violated:** The design assumes that `chain::verify`'s processing of sentinel rows provides cumulative tamper-detection coverage over all sentinel rows. The algorithm provides coverage only for the final sentinel row.

**Suggested mitigation:** Two options: (a) Fix the documentation to accurately describe the actual security guarantee: "The first non-sentinel row's `prev_hash` equals the hash of the LAST sentinel row's canonical JSON, not the hash of a chain linking all sentinel rows. Pre-migration rows other than the immediately preceding row are not cryptographically protected by the chain. Operators who require tamper-detection for all pre-migration rows should re-hash the full table after migration 0006 completes." (b) Change the algorithm to actually accumulate: instead of `last_computed_hash = sha256(canonical_json(row))`, use `last_computed_hash = sha256(last_computed_hash || canonical_json(row))` (concatenated input), so each sentinel row's hash covers all prior sentinels. This makes the guarantee accurate but is a more complex algorithm. Option (a) is lower risk; option (b) is more secure. Either way, update the documentation to remove "accumulated through the sentinel sequence" and replace with language that accurately describes what is and is not protected.

---

### LOW — `PRAGMA foreign_keys = ON` is not specified for the dedicated `AuditWriter` connection; the `snapshot_id REFERENCES snapshots(id)` foreign key is unenforced at the DB layer for audit writes

**Category:** Schema/type mismatch / assumption violation

**Trigger:** The existing `SqliteStorage::open` method (in `sqlite_storage.rs`) builds connection options that explicitly set `.foreign_keys(true)` (which emits `PRAGMA foreign_keys = ON` on the connection). SQLite's default is `foreign_keys = OFF`; the PRAGMA must be set per-connection.

The `AuditWriter` task specifies that the dedicated connection "MUST be opened with `PRAGMA busy_timeout = 5000`." No other connection PRAGMAs are specified. An implementer who opens the audit connection without specifying `foreign_keys = ON` gets the SQLite default — foreign keys are disabled.

The `audit_log` schema declares `snapshot_id TEXT REFERENCES snapshots(id)`. Without `foreign_keys = ON`, an `AuditWriter::record` call can INSERT a row with `snapshot_id = "nonexistent-ulid"` (a stale reference, a typo, or a row ID from a different DB), and the INSERT succeeds silently. The `snapshot_id` column is optional (`NULL` is valid), so this only affects rows that provide a non-null `snapshot_id`. Concretely: if a code path constructs `AuditRow { snapshot_id: Some(id), … }` where `id` is computed from a stale or incorrect source, the INSERT succeeds and the audit row permanently references a non-existent snapshot. The `audit_log` is immutable, so the dangling reference cannot be corrected.

**Consequence:** The referential integrity guarantee of `snapshot_id REFERENCES snapshots(id)` is silently unenforced for the audit write path. Any call site that supplies a stale or incorrect `snapshot_id` produces a permanent dangling foreign key in the immutable audit log. Post-hoc analysis tools that JOIN `audit_log` with `snapshots` will silently miss these rows (inner join) or produce NULL columns (left join), misleading operators performing forensic analysis.

**Design assumption violated:** The design assumes the dedicated audit connection enforces the same referential integrity as the main pool connection. The task specifies only `busy_timeout`; all other connection options (including `foreign_keys`) default to SQLite's connection-level defaults, which include `foreign_keys = OFF`.

**Suggested mitigation:** Add to the `AuditWriter` task: "The dedicated `SqliteConnection` MUST also set `PRAGMA foreign_keys = ON` in addition to `PRAGMA busy_timeout = 5000`. When the connection is reopened during error recovery (`connect()` in step 9), the same pair of PRAGMAs MUST be re-applied to the new connection." This ensures the audit write path enforces the same referential integrity as the main pool.

---

## No findings (categories with nothing concrete to raise)

- **Authentication & authorization** — `AuditWriter` is server-internal. No new auth bypass vector beyond the `from_db_str` convention-only enforcement (raised in R16-M1, now grep-recipe mitigated).
- **Abuse cases** — 10 MB cap (byte-accurate BLOB counting for `redacted_diff_json` and `notes`), max 1000 row limit, `AUDIT_QUERY_ROW_FIXED_OVERHEAD_BYTES` constant, `busy_timeout = 5000`, and `occurred_at_ms > 0` guard before mutex lock close the main abuse vectors. No new path found.
- **Race conditions** — `Mutex<Option<SqliteConnection>>` + `BEGIN IMMEDIATE` serialises all writes. Concurrent-write chain linearisation is specified and tested. No new race found.
- **Resource exhaustion** — Paginated `chain::verify` (batch 500), 10 MB query cap, `busy_timeout`, and `InvalidTimestamp` guard close the exhaustion paths. No new vector.
- **State machine violations** — Migration 0006 step order (ALTER TABLE → UPDATE → CREATE TRIGGER × 2) is correctly sequenced with backfill before triggers. No new violation.
- **Single points of failure** — Connection recovery (R5-F402, R6-F505) and `busy_timeout` (R8-F801) address SPOF vectors. No new SPOF.
- **Timeouts & retries** — `PRAGMA busy_timeout = 5000` + `BusyTimeout` error caps the wait. No retry loop. No new hazard.
- **Rollbacks** — Audit writes are intentionally out-of-band from business transactions. No rollback semantics for audit rows by design.
- **Orphaned data** — Immutability triggers prevent cleanup by design. No orphan accumulation path.
- **Migration hazards** — sqlx wraps each migration file in a transaction; the four steps in `0006` are atomic. No partial-apply hazard.
- **Eventual consistency** — Single-process SQLite; no multi-store consistency gap.
- **`canonical_json` key ordering** — The specified sorted key list (`"actor_id"`, `"actor_kind"`, …, `"target_kind"`) is verified correct in lexicographic order. No misordering found.
- **`ZERO_SENTINEL` vs `""` handling** — The distinction between ZERO_SENTINEL (skip-assertion, hash-and-track) and `""` (immediate `ChainError::EmptyHash`) is correctly specified. The MEDIUM finding above concerns only the scope of pre-migration coverage, not the sentinel/empty distinction itself.
- **`Actor` unknown-kind fallback** — R16-H1 closed: both `record` step 5 and the startup paginator specify the identical `"__unknown:{kind}"` substitution. The symmetric gap for `AuditEvent` is raised as a new HIGH finding above.
- **`RedactedDiff` opacity** — `from_db_str` is `pub` with naming-convention enforcement; the `grep-from-db-str-callers` recipe makes call sites auditable. No new bypass path beyond the trade-off acknowledged in R15-H1 closure.
- **Schema/type mismatches** — `AuditOutcome` deserialization (`serde::Deserialize` with `rename_all = "lowercase"`), `AuditEvent::FromStr` via `strum::EnumString`, `Actor::from_kind_id` (with unknown-kind fallback), `canonical_json` key names (all 17 keys specified and verified), `redacted_diff_json` encoding, and `strum::Display` replacing bespoke match block are all fully specified. The `PRAGMA foreign_keys` omission is raised as a new LOW finding above.
- **`redaction_sites` type conversion** — `usize`-to-`i64` conversion is unspecified but practically harmless (no diff can have more than `i64::MAX` redaction sites; the corpus test verifies the stored count equals `RedactedDiff::new`'s returned count). Not raised as a finding — the practical overflow risk is negligible and the corpus test provides behavioral verification.

---

## Summary

**Critical:** 0  **High:** 1  **Medium:** 1  **Low:** 1

**Top concern:** R17-H1 — the startup paginator has no specified fallback for `AuditEvent::from_str` failures on unrecognized `kind` strings. This is structurally identical to the R16-H1 finding for `Actor::from_kind_id` (which was a blocker): after any future phase extends the `AuditEvent` enum and writes rows, the startup paginator will either abort chain verification entirely (propagating the error) or invent an unspecified fallback that diverges from the write path's `canonical_json`, producing permanent false `ChainBroken` alarms on every restart.

**Recommended action before proceeding:** Address R17-H1 (specify the `AuditEvent::from_str` fallback for the startup paginator, parallel to R16-H1's resolution for `Actor::from_kind_id`) before implementation begins — it is a blocker. R17-M1 (ZERO_SENTINEL documentation corrects a stated security guarantee that does not hold for tables with multiple pre-migration rows) should be addressed by updating the documentation to accurately describe coverage scope. R17-L1 (`PRAGMA foreign_keys = ON` for the dedicated audit connection) should be added to the task acceptance criteria as a connection option alongside `busy_timeout`.
