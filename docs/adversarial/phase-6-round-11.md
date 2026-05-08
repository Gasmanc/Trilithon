# Adversarial Review — Phase 6 — Round 11

**Design summary:** Phase 6 adds a tamper-evident, append-only audit log to Trilithon: an `AuditWriter` with a dedicated serialised SQLite connection, a SHA-256 `prev_hash` chain, a `RedactedDiff` newtype for secrets-safe diffs, an `Actor` enum, startup integrity checks, and a query API with a 10 MB soft cap.

**Prior rounds:** Rounds 1–10 reviewed. All R10 blockers (F1001, F1002) are closed in the current design text: `canonical_json` now mandates `row.kind.to_string()` and `row.id.to_string()`, plus stability test criterion (h) asserts both. F1003–F1006 (MEDIUM/LOW) are also closed. No open items carried forward.

---

## Findings

### F1101 — HIGH — `AuditEvent` has no `FromStr` or `Deserialize` implementation specified; the startup paginator cannot reconstruct `AuditRow.kind` from the `kind TEXT` column, making `chain::verify` uncallable from the startup path

**Category:** Schema/type mismatch

**Trigger:** The startup guard calls `chain::verify(rows: impl Iterator<Item = &AuditRow>)` via a paginated `ORDER BY rowid ASC` loop. The paginator reads rows from SQLite where `kind` is stored as a TEXT column (e.g., `"mutation.applied"`). To construct an `AuditRow` (whose `kind` field is typed `AuditEvent`), the caller must convert that TEXT value back to an `AuditEvent` variant. `AuditEvent` has a `Display` impl that produces the canonical string but no `FromStr` or `Deserialize` impl is specified anywhere in the design. The vocabulary consolidation task says "derive `strum::EnumCount`" but does not specify `strum::EnumString` (which would provide `FromStr`) or `serde::Deserialize`. Without one of these, the startup paginator cannot construct a well-typed `AuditRow` at all.

Concrete failure: an implementer writes the startup paginator and discovers they cannot build `AuditRow { kind: AuditEvent, … }` from a raw sqlx `Row`. They choose one of three workarounds independently: (a) add `#[derive(strum::EnumString)]` to `AuditEvent` and use `.parse()`; (b) write a bespoke `from_str` match in the paginator; (c) change `AuditRow.kind` to `String` for the startup-path read model, using a separate DB-fetch struct. Options (a) and (b) produce different `FromStr` semantics if not cross-tested. Option (c) introduces a second `AuditRow`-like struct that diverges from the one passed to `chain::verify`. Because `chain::verify` takes `&AuditRow` with `kind: AuditEvent`, option (c) requires either two structs or a conversion — neither of which is specified.

**Consequence:** Every startup either fails to compile (can't construct `AuditRow`) or introduces an unspecified deserialization path that could silently mis-parse `kind` strings, producing `AuditRow` values with wrong `kind` fields. `canonical_json(row)` then encodes the wrong `kind` string, producing a different hash than `record` wrote — `chain::verify` reports a false `ChainBroken` for every row in the table. Because the rows are immutable, the permanently wrong startup hashes cannot be corrected without dropping the chain verification entirely.

**Design assumption violated:** The design assumes `AuditRow` is the canonical type for both write and read paths, without specifying how `AuditEvent` is reconstructed from its TEXT DB representation.

**Suggested mitigation:** Add to the vocabulary consolidation task: "`AuditEvent` MUST also derive `strum::EnumString` (with `#[strum(serialize = \"...\")]` per-variant matching the `Display` strings, or via `#[strum(serialize_all = \"kebab-case\")]` where the display strings are kebab-case). The `FromStr` implementation MUST round-trip with `Display`: `event.to_string().parse::<AuditEvent>() == Ok(event)` for every variant. The startup paginator uses `.parse()` to reconstruct `AuditEvent` from the `kind` TEXT column. Add an acceptance test: `for event in all_variants() { assert_eq!(event.to_string().parse::<AuditEvent>(), Ok(event)); }`." Without this, the startup `chain::verify` call has no specified path for reconstructing `AuditRow.kind`.

---

### F1102 — HIGH — `canonical_json(row: &AuditRow)` must include the `occurred_at` DB column but `AuditRow` has no `occurred_at` field; the function cannot include a value it cannot access from its argument

**Category:** Logic flaw / schema mismatch

**Trigger:** The `canonical_json` spec says it MUST include "every column of `audit_log`" in the JSON object. The `audit_log` table has two time columns: `occurred_at INTEGER NOT NULL` and `occurred_at_ms INTEGER NOT NULL`. `AuditRow` carries `occurred_at_ms: i64` but NOT `occurred_at` — `record` derives `occurred_at = occurred_at_ms / 1000` at bind time and never stores the quotient on the struct. `canonical_json` receives `&AuditRow`; it has access to `occurred_at_ms` but not to `occurred_at` as a stored field.

Concrete failure sequence: an implementer writes `canonical_json` to include every field from `AuditRow`. `AuditRow` has no `occurred_at` field, so they either (a) omit `occurred_at` from the JSON (but the spec says include every column → spec violation), or (b) compute `row.occurred_at_ms / 1000` inside `canonical_json` to derive `occurred_at`. If they choose (b) and separately another implementer writes the startup paginator, that paginator reads the `occurred_at` value directly from the DB row. If the DB row has `occurred_at = 1700000001` (written by `record`) and `canonical_json` computes `occurred_at_ms / 1000 = 1700000001`, they agree. But if `occurred_at_ms` is 1700000001500 (subsecond), `occurred_at_ms / 1000 = 1700000001` in Rust (integer truncation) — still correct. The real danger is a future implementer who reads the spec "include every column of `audit_log`" and re-fetches `occurred_at` from the DB at startup (since `AuditRow` doesn't carry it) while `record` uses `occurred_at_ms / 1000`. If subsecond rounding differs between the two codepaths, `occurred_at` in the DB and the computed value in `canonical_json` diverge → `chain::verify` always returns `ChainBroken` for every row.

More concretely: if the startup paginator reads `occurred_at` directly from the DB column and stores it on a `AuditRow`-equivalent struct, it must carry a different type than the `AuditRow` passed to `record`. One struct has `occurred_at: i64` (from DB), the other doesn't. Either `canonical_json` takes both structs (wrong — it takes `&AuditRow`) or the two codepaths diverge on `occurred_at` encoding.

**Consequence:** `canonical_json` either violates its own "every column" spec (by omitting `occurred_at`) or silently diverges between the write-time and read-time codepaths if `occurred_at` is computed differently. Any divergence produces permanent false `ChainBroken` errors on all rows because rows are immutable.

**Design assumption violated:** The design assumes `AuditRow` carries all DB columns needed by `canonical_json`, but `occurred_at` is a derived-at-write-time column not stored on `AuditRow`.

**Suggested mitigation:** Two options, either is acceptable: (a) Add `occurred_at: i64` to `AuditRow` as a computed field populated by `record` before calling `canonical_json` (i.e., `let occurred_at = row.occurred_at_ms / 1000; row.occurred_at = occurred_at;` or as a constructor parameter). Add an explicit note to `canonical_json`: "For `occurred_at`, use `row.occurred_at` (the quotient stored on the struct, NOT a re-computation from `occurred_at_ms` at call time)." Or (b) specify in `canonical_json`: "The `occurred_at` field in the canonical JSON MUST be computed as `row.occurred_at_ms / 1000` (integer truncation, matching `record`'s bind-time computation). The function does not read `occurred_at` from the DB." Option (b) is simpler and avoids adding a field to `AuditRow`, but requires the spec to make the computation explicit so both `record` and `chain::verify`'s internal `canonical_json` call agree.

---

### F1103 — MEDIUM — `canonical_json` spec says "actor_kind/actor_id are stored as plain strings in `AuditRow` and encoded as-is," but `AuditRow` definition specifies `actor: Actor` (an enum); these two specifications are mutually contradictory, leaving `canonical_json`'s access path for those fields unresolved

**Category:** Documentation trap / schema mismatch

**Trigger:** The `AuditRow` definition task specifies the field as `actor: Actor` (a typed Rust enum). The `canonical_json` task says "actor_kind/actor_id are stored as plain strings in AuditRow and encoded as-is." These two statements cannot both be true: if `AuditRow.actor` is `Actor` (an enum), then `actor_kind` and `actor_id` are NOT "plain strings in AuditRow" — they must be extracted by decomposing the enum. If they ARE plain strings on `AuditRow`, then the field definition contradicts the `actor: Actor` type in the `AuditRow` task.

Concrete consequence: an implementer writing `canonical_json` reads the spec and concludes they can call `row.actor_kind` and `row.actor_id` directly. But `AuditRow` has no such fields — `actor` is an enum. They must write a match arm to extract strings. A different implementer reads `actor: Actor` in the `AuditRow` definition and derives `Serialize` on `Actor` with a custom serializer that produces `{"actor_kind": "user", "actor_id": "alice"}` — then uses serde to serialize `AuditRow` in `canonical_json`. The two implementations produce different JSON structures for the same logical row (one has `actor_kind` and `actor_id` as top-level keys; the other has them nested under an `actor` object), producing different hashes.

**Consequence:** The canonical JSON format for `actor_kind`/`actor_id` is implementation-defined. Two implementations produce different hashes for the same logical row. On any deployment where `canonical_json` is rebuilt (e.g., after a refactor that restructures `AuditRow`), the chain hash is permanently invalidated.

**Design assumption violated:** The design assumes the two tasks (`AuditRow` definition and `canonical_json` spec) are consistent in how `actor_kind`/`actor_id` are stored on `AuditRow`.

**Suggested mitigation:** Resolve the contradiction explicitly in the `AuditRow` task: choose one of — (a) change the field to `actor_kind: String, actor_id: String` (two plain strings, decomposed from `Actor` at construction time in `record` and at read time in the startup paginator), consistent with the `canonical_json` spec; or (b) keep `actor: Actor` and update `canonical_json` to say "the `actor_kind` and `actor_id` fields in the canonical JSON MUST be derived from `row.actor` using the same mapping as the `Actor::to_kind_id()` helper (or equivalent) — NOT via serde." Whichever option is chosen, the stability test (criterion h) must include an assertion that `actor_kind` and `actor_id` appear as top-level string keys in the canonical JSON output.

---

### F1104 — MEDIUM — `canonical_json`'s handling of `Option<RedactedDiff>` is unspecified; `RedactedDiff` has no `Serialize` impl, only `as_str()`, and the function must choose between two incompatible serialization strategies that produce different byte outputs

**Category:** Documentation trap

**Trigger:** `canonical_json(row: &AuditRow) -> Vec<u8>` must serialize `row.redacted_diff: Option<RedactedDiff>` into the JSON object under the key `"redacted_diff_json"` (matching the DB column name). `RedactedDiff` has no public field and no specified `Serialize` impl. `as_str()` is the only specified public accessor. To produce the JSON, `canonical_json` must do one of:

(a) Call `row.redacted_diff.as_ref().map(RedactedDiff::as_str)` to get `Option<&str>`, then serialize to a JSON string or null. The output for a present value is `"redacted_diff_json": "{\"key\":\"value\"}"` — the diff content appears as a JSON-encoded string (the JSON object is string-escaped). This produces a different byte sequence than option (b).

(b) Call `row.redacted_diff.as_ref().map(|d| serde_json::from_str::<serde_json::Value>(d.as_str()))` to parse the string as a JSON value, then embed it as a nested JSON object. The output is `"redacted_diff_json": {"key":"value"}` — the diff appears as a nested object. This also produces a different byte sequence from (a).

The design prohibits `Json(…)` double-encoding in `record`'s SQL bind, but says nothing about `canonical_json`. An implementer who reasons "the DB stores a raw JSON string, so canonical_json should embed a JSON string value" chooses (a). An implementer who reasons "canonical_json should represent the actual structure, not an escaped string" chooses (b). These two produce different SHA-256 hashes.

**Consequence:** `record` and `chain::verify` implementations can diverge on the serialization of `redacted_diff_json`. Divergence produces false `ChainBroken` on every row that carries a `RedactedDiff`. Because rows are immutable, the divergence is permanent and uncorrectable once written.

**Design assumption violated:** The design assumes `canonical_json`'s serialization of `Option<RedactedDiff>` is obvious from context, but the `RedactedDiff` opacity invariant and the `as_str()` accessor create two equally defensible approaches.

**Suggested mitigation:** Add to the `canonical_json` spec: "The `redacted_diff_json` field in the canonical JSON MUST be encoded as a JSON string (or JSON null for `None`) — call `row.redacted_diff.as_ref().map(RedactedDiff::as_str)` to obtain `Option<&str>`, then encode as a JSON string value or null. Do NOT parse the inner string as a JSON value or embed it as a nested JSON object." This one-sentence rule matches the DB column's TEXT storage type (a string, not a nested object) and closes the ambiguity.

---

## No findings (categories with nothing concrete to raise)

- **Authentication & authorization** — `AuditWriter` is server-internal; no new auth surface in round 11.
- **Abuse cases** — 10 MB cap (with `notes` column now included in byte counting), per-row overhead constant, and max 1000 row limit close the main vectors. No new abuse path found.
- **Race conditions** — `Mutex<Option<SqliteConnection>>` + `BEGIN IMMEDIATE` serializes all writes. Startup paginator correctness with concurrent writes was addressed in R9-F903. No new concurrent race found.
- **Resource exhaustion** — `busy_timeout = 5000`, paginated verify (batch 500), and 10 MB cap close the main exhaustion paths.
- **State machine violations** — Migration 0006 step order is correct. No new state machine gap beyond the transaction atomicity note (which relies on sqlx migrator behavior that is standard).
- **Error handling gaps** — `BusyTimeout`, `ConnectionLost`, `ConnectionRecoveryFailed`, `InvalidTimestamp`, `SecretsRevealedNotYetSupported`, `TriggersMissing` all specified. No new unhandled error path found.
- **Rollbacks** — Audit writes are intentionally out-of-band from business transactions; no rollback semantics for audit rows by design.
- **Orphaned data** — Immutability triggers prevent cleanup by design. No new orphan path.
- **Migration hazards** — sqlx migrator wraps each migration file in a transaction by default; the four steps in `0006` are atomic under standard sqlx migration behavior.
- **Single points of failure** — Connection recovery (R5-F402, R6-F505) and `busy_timeout` (R8-F801) address SPOF vectors. No new SPOF found.
- **Timeouts & retries** — `PRAGMA busy_timeout = 5000` + `BusyTimeout` error cap the wait. No retry loop introduced.
- **Eventual consistency** — Single-process SQLite; no multi-store consistency gap.
- **`prev_hash` chain correctness** — `chain::verify` early return (R9-F903), sentinel hash tracking (R6-F502), and zero-sentinel OK return (R5-F401) are all specified. No new chain logic gap beyond the `AuditEvent` FromStr and `occurred_at` findings above.

---

## Summary

**Critical:** 0  **High:** 2  **Medium:** 2  **Low:** 0

**Top concern:** F1101 — if `AuditEvent` has no `FromStr`/`Deserialize`, the startup paginator cannot reconstruct `AuditRow.kind` from the DB, which means either `chain::verify` is never called in the startup path (the tamper-detection guarantee silently vanishes) or each implementer invents an independent deserialization path that diverges on corner cases and produces permanent false chain-break reports on immutable rows.

**Recommended action before proceeding:** Address criticals first — F1101 and F1102 are blockers because both create conditions where `canonical_json` produces a different hash than `record` wrote, reporting every row as tampered on startup. F1103 and F1104 are design clarifications that prevent implementer divergence on the serialization format.
