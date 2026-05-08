# Adversarial Review — Phase 6 — Round 15

**Design summary:** Phase 6 adds a tamper-evident, append-only audit log to Trilithon: an `AuditWriter` with a dedicated serialised SQLite connection, a SHA-256 `prev_hash` chain, a `RedactedDiff` newtype for secrets-safe diffs, an `Actor` enum with `from_kind_id` reverse-mapping, startup integrity checks, and a query API with a 10 MB soft cap.

**Prior rounds:** Rounds 1–14 reviewed. All R14 findings are closed in the current design text:
- R14-H1 (`RedactedDiff::from_db_str` constructor): CLOSED — `pub(crate) fn from_db_str(s: String) -> RedactedDiff` specified in the `RedactedDiff` task.
- R14-M1 (`AuditOutcome` deserialization): CLOSED — `serde::Deserialize` with `rename_all = "lowercase"` mandated on `AuditOutcome`.
- R14-L1 (stability test coverage for `outcome`): CLOSED — stability test criterion (h) now asserts `outcome` value is one of `"ok"`, `"error"`, `"denied"`.

No open items carried forward from prior rounds.

---

## Round 14 Closure Table

| ID | Severity | Title | Status |
|----|----------|-------|--------|
| R14-H1 | HIGH | `RedactedDiff` has no DB reconstruction constructor | CLOSED — `pub(crate) from_db_str` specified in `RedactedDiff` task |
| R14-M1 | MEDIUM | `AuditOutcome` deserialization unspecified | CLOSED — `serde::Deserialize` with `rename_all = "lowercase"` mandated |
| R14-L1 | LOW | Stability test omits `outcome` field assertion | CLOSED — criterion (h) now asserts `outcome` is lowercase |

---

## Findings

### HIGH — `pub(crate) RedactedDiff::from_db_str` is inaccessible from `crates/adapters`; `record` step 5 cannot call it across crate boundaries

**Category:** Composition failure / logic flaw

**Trigger:** The `RedactedDiff` task specifies: "`pub(crate) fn from_db_str(s: String) -> RedactedDiff`" in `crates/core/src/audit.rs`. `AuditWriter::record` step 5 is in `crates/adapters`. In Rust, `pub(crate)` visibility is scoped to the crate in which the item is defined — it is visible only within `crates/core`, not from `crates/adapters`. `adapters` is a separate crate that depends on `core`; it can only access items that are `pub` or `pub(super)` in the relevant module of `core`. `pub(crate)` is explicitly less permissive than `pub` and does not cross the crate boundary.

Concrete failure sequence: an implementer writes `AuditWriter::record` in `adapters`, reaches step 5, fetches the predecessor row with `SELECT *`, and needs to construct `AuditRow { redacted_diff: Some(RedactedDiff::from_db_str(s)), … }`. The compiler produces `error[E0603]: function 'from_db_str' is private` (or more precisely, "function 'from_db_str' is not accessible" because of `pub(crate)` on a cross-crate call). The design also says "a `pub(crate)` constructor for use ONLY by `record` step 5 and the startup paginator" — but the startup paginator is also in `adapters` (it calls `chain::verify` which lives in `core`, but the paginator loop that reads DB rows and constructs `AuditRow` values lives in `adapters`). Both stated use sites are in a different crate than the visibility allows.

**Consequence:** The design is structurally inconsistent: it mandates `pub(crate)` (intra-crate only) for a function that must be called from a different crate. An implementer cannot follow the design as written. They will independently choose one of: (a) promote the visibility to `pub`, destroying the opacity guarantee — any external caller can now construct an unredacted `RedactedDiff` from a raw string; (b) create a wrapper function in `core` that bridges the call (e.g., `pub fn reconstruct_redacted_diff_from_db(s: String) -> RedactedDiff`) — semantically equivalent to making it public under a different name; (c) restructure `adapters` to not need `from_db_str` at all, changing the `AuditRow` type for the read path — which is the split-struct approach the R14 finding explicitly avoided. Any independent choice produces a different visibility contract than the design specifies, with option (a) directly violating the redactor-bypass prevention guarantee.

**Design assumption violated:** The design assumes `pub(crate)` is sufficient for cross-crate callers in `adapters`, conflating "restrict to known callers" with "intra-crate visibility." The correct visibility to share across crates while restricting to the project is `pub` with module-path documentation, or a re-export at the `core` crate's public surface.

**Suggested mitigation:** Choose one of two approaches: (a) make `from_db_str` fully `pub` but rename it to signal intent — `pub fn wrap_already_redacted_db_string(s: String) -> RedactedDiff` — and document it with a warning: "This constructor bypasses redaction. Call only on the DB read path, never at the point of writing a new row." The opacity guarantee is now enforced by convention, not the compiler, but this is the correct trade-off for cross-crate access. Or (b) keep `pub(crate)` and move the predecessor-row reconstruction logic into `core` (a `reconstruct_predecessor` function in `core/src/audit.rs` that takes raw column values and returns `AuditRow`), then call this `pub` function from `adapters`. Either way, the design must acknowledge that `pub(crate)` does not cross crate boundaries and update the specification accordingly.

---

### HIGH — `strum::EnumString` derive and bespoke `Display` match block are independent string tables; a per-variant discrepancy silently corrupts `FromStr` without breaking `Display`

**Category:** Logic flaw / test coverage gap

**Trigger:** The vocabulary consolidation task mandates: "`AuditEvent` MUST also derive `strum::EnumString` (NOT a bespoke `match` block), with per-variant `#[strum(serialize = '...')]` attributes matching the `Display` strings." The current `audit.rs` implements `Display` as a bespoke `match` block (not via `strum::Display`). The design does not say to replace the existing `Display` impl with `strum::Display` — it says to ADD `strum::EnumString` derive alongside the existing `Display`. This creates two independent string-to-variant mappings: one in the `match` arms of `Display::fmt`, and one in the `#[strum(serialize = "...")]` attributes for `EnumString`. These are separate annotation sites that a developer maintains independently.

Concrete failure scenario: an implementer adds the three new variants (`AuthBootstrapCredentialsCreated`, `CaddyOwnershipSentinelTakeover`, `SecretsMasterKeyFallbackEngaged`) to the enum and to the `Display` match block, then adds `#[strum(serialize = "auth.bootstrap-credentials-created")]` to `AuthBootstrapCredentialsCreated` — but accidentally writes `"caddy.ownership-sentinel-takeover"` in the Display match but `"caddy.ownership.sentinel.takeover"` (dots instead of hyphens) in the strum attribute. Or, more plausibly: for `MutationRejectedMissingExpectedVersion`, the Display string is `"mutation.rejected.missing-expected-version"` — a developer adding strum attributes might write `"mutation.rejected.missing-expected-version"` correctly in some variants but make a typo in the long string.

The round-trip test `for variant in AuditEvent::iter() { assert_eq!(variant.to_string().parse::<AuditEvent>().unwrap(), variant); }` DOES catch this — `to_string()` uses the Display match, `.parse()` uses EnumString. If they disagree for a variant, the round-trip test fails. This is the designed detection mechanism.

However, the design also says the round-trip test uses `AuditEvent::iter()` from `strum::EnumIter`. If `EnumIter` is added but not `EnumString` (for example if an implementer derives `EnumCount` and `EnumIter` correctly but writes a bespoke `FromStr` match instead of `EnumString`), the round-trip test iterates via `iter()` and calls `.parse()` which dispatches to the bespoke `FromStr`. A typo in the bespoke `FromStr` match for a low-traffic variant (e.g., `SecretsMasterKeyFallbackEngaged`) — one that only appears in operational edge cases — would be caught by the test only if the test actually runs that variant's round-trip. With `EnumIter` driving the test, all 44 variants are covered. But the design says "with per-variant `#[strum(serialize = '...')]` attributes" AND separately allows "a bespoke `FromStr` match" in the R12-M1 wording — though R12-M1 was closed by mandating `strum::EnumString`. The closure is in the design text but there is tension because the existing bespoke `Display` match remains.

The residual risk: if the implementer keeps the bespoke `Display` and adds `strum::EnumString` with strum attributes, they maintain two tables. The round-trip test catches divergence. But the compile-time assertion `<AuditEvent as strum::EnumCount>::COUNT == AUDIT_KIND_VOCAB.len()` does NOT catch a discrepancy between Display strings and EnumString strings — it only counts variants. A variant where Display produces `"X"` and EnumString parses `"Y"` passes the count assertion and even passes a one-direction vocab test (Display→vocab), but fails only the round-trip. If the round-trip test is skipped or excluded from CI, the discrepancy goes undetected in production.

**Consequence:** A divergence between Display and EnumString for any variant means: (a) `record` stores `kind = "X"` (from Display) in the DB; (b) the startup paginator parses `"X"` using EnumString but gets an error (EnumString only knows `"Y"` for that variant) — the paginator's `Actor::from_kind_id` analogue for `AuditEvent` produces `Err`, and `record` step 5's predecessor reconstruction fails, causing every new audit write to fail for any table that has the discrepant variant as the predecessor row's `kind`. The chain is permanently unwritable for that audit table until the discrepancy is fixed and the daemon is redeployed.

**Design assumption violated:** The design assumes the round-trip test is sufficient to catch Display/EnumString divergence, but does not address that two independent annotation sites (match arms and strum attributes) must be kept in sync manually.

**Suggested mitigation:** Eliminate the dual-annotation risk: specify that the existing bespoke `Display` match block MUST be replaced with a `strum::Display` derive. When both `Display` and `EnumString` are driven by the same strum attributes (`#[strum(serialize = "...")]`), divergence is structurally impossible — the same attribute drives both traits. Add to the vocabulary consolidation task: "The existing bespoke `impl fmt::Display for AuditEvent { fn fmt(...) match ... }` MUST be removed and replaced with `#[derive(strum::Display)]` so that Display and EnumString share the same per-variant `#[strum(serialize = '...')]` annotation at the derive level. Maintaining two separate string tables (a match block and strum attributes) is not acceptable." If removing the bespoke Display creates issues (e.g., strum formatting differences), document the exact strum feature flag needed.

---

### MEDIUM — `Actor::from_kind_id` error path in `record` step 5 is unspecified; an unrecognized `actor_kind` in the predecessor row causes every subsequent audit write to fail permanently

**Category:** Error handling gap / state machine violation

**Trigger:** `AuditWriter::record` step 5 fetches `SELECT * FROM audit_log ORDER BY rowid DESC LIMIT 1` and reconstructs a typed `AuditRow`. This reconstruction requires calling `Actor::from_kind_id(actor_kind_text, actor_id_text)`. The `Actor` task specifies: "Unrecognised `kind` strings → `Err(AuditError::UnknownActorKind { kind: kind.to_owned() })`."`

The step sequence in the `AuditWriter` task is silent on what `record` should do when step 5's `Actor::from_kind_id` returns `Err`. The design mentions the error variant exists but never places it in the step-5 error-handling flow.

Concrete failure scenarios:
- A future phase adds a new `Actor` variant with a new `actor_kind` string (e.g., `"api-key"` for API tokens). Any `audit_log` row written by that future phase has `actor_kind = "api-key"`. When Phase 6's `AuditWriter::record` reads the predecessor row and calls `Actor::from_kind_id("api-key", id)`, it returns `Err(UnknownActorKind { kind: "api-key" })`. If `record` propagates this error (the natural `?` operator behavior), every subsequent audit write returns `Err(UnknownActorKind)` permanently — because the most-recent row in the table always has `actor_kind = "api-key"` and `record` always reads the most-recent row. The audit trail is silently dead until the future phase's `Actor` variant is backported to Phase 6's `from_kind_id`, requiring a code change and redeployment.
- A DB admin inserts a test row directly with `actor_kind = "admin"`. Same consequence.
- Note: the `audit_log` schema has NO `CHECK` constraint on `actor_kind` (unlike `snapshots` which has `CHECK (actor_kind IN ('user', 'token', 'system'))`), so there is no DB-level guard preventing an unrecognized `actor_kind` from being inserted.

**Consequence:** Any audit row with an unrecognized `actor_kind` permanently blocks `AuditWriter::record` via the predecessor reconstruction in step 5. The daemon continues running and serving requests, but no audit events are written. The audit trail goes silent without any alarm specific to this cause (the returned error is `UnknownActorKind`, not `ConnectionLost` or `BusyTimeout` — callers who inspect the error type will see an unexpected variant). Because `audit_log` has no `CHECK (actor_kind IN (...))` constraint, no write-time guard prevents the blocking row from entering the table.

**Design assumption violated:** The design assumes the only `actor_kind` values that will ever appear in `audit_log` are those producible by Phase 6's `Actor` enum. It does not account for direct DB writes, future phases adding new actor kinds before Phase 6's `from_kind_id` is updated, or production data imported from a different schema version.

**Suggested mitigation:** Add two mitigations:
1. Add to `AuditWriter::record` step 5: "If `Actor::from_kind_id` returns `Err(UnknownActorKind)`, treat the predecessor row as a ZERO_SENTINEL row for the purpose of `canonical_json` — substitute `Actor::System { component: 'unknown-actor-kind' }` and log `tracing::warn!(actor_kind = %kind, row_id = %id, 'audit: unknown actor_kind in predecessor row; chain hash may differ from a future re-verify with the correct Actor variant')`. Do NOT propagate the error — the audit write must succeed regardless of predecessor reconstruction issues." OR alternatively: add a `CHECK (actor_kind IN ('user', 'token', 'system'))` constraint to `audit_log.actor_kind` in migration 0006 (matching the `snapshots` table constraint), so that unrecognized `actor_kind` values are rejected at INSERT time by the DB rather than discovered at predecessor reconstruction time.
2. Whichever approach is chosen, document it in the `AuditWriter` task step sequence.

---

### MEDIUM — Startup deduplication set conflicts with `chain::verify`'s early-return contract; the in-memory set can only ever hold one entry per startup run, making the deduplication logic vacuous

**Category:** Logic flaw / documentation trap

**Trigger:** The startup guard task specifies: "to prevent operators from becoming desensitised to a historical break, log the chain break **once per distinct `row_id`** by recording the last-seen broken `row_id` in an in-memory set at startup (not persisted — the goal is to not repeat the same error multiple lines in a single startup, not to suppress it across restarts)."

`chain::verify` is specified (R9-F903, now in the task) to: "return `Err(ChainBroken { … })` on the first broken link and MUST NOT continue scanning past the break." `chain::verify` returns a single `Err` value and then stops — it does not return a list of broken rows, and it does not call a callback multiple times. The caller receives one `Err` with one `row_id`.

The startup sequence therefore calls `chain::verify` once and gets either `Ok(())` or `Err(ChainBroken { row_id })`. There is only ever one `row_id` to add to the in-memory set per `chain::verify` invocation. If the startup sequence calls `chain::verify` only once per startup (which is the specified behavior — "startup calls `chain::verify` via paginated `ORDER BY rowid ASC` (batch 500)"), the set can contain at most one element throughout the entire startup pass. A set that can hold one element provides zero deduplication value and adds specification complexity without benefit.

The original intent (from R10-F1004) was to prevent the same break from being logged multiple times in a single startup run — but `chain::verify` can only produce one break report per call by design. The deduplication requirement creates a logical contradiction: the code must track "seen row_ids" for a source that can only produce one distinct row_id per invocation.

**Consequence:** An implementer who follows the design literally will write a `HashSet<String>` that is populated with zero or one entry, guarded by `if !seen_breaks.contains(&row_id)` before logging. This code path is dead logic — the condition is always true (the set is always empty when the first and only break is received). The implementation will be either: (a) correct but wasteful (an always-empty set guarded by an always-true condition), or (b) an implementer who misreads the spec will build a loop that calls `chain::verify` in a different way (perhaps catching partial results via a callback) — fundamentally misunderstanding the iterator API in an attempt to make the deduplication logic meaningful, producing a different `chain::verify` invocation pattern than specified.

**Design assumption violated:** The design assumes `chain::verify` can produce multiple distinct `row_id` errors in a single startup pass, making a deduplication set meaningful. This contradicts the early-return-on-first-break specification.

**Suggested mitigation:** Remove the in-memory deduplication set from the startup guard task. Replace with: "If `chain::verify` returns `Err(ChainBroken { row_id, … })`, log `tracing::error!(chain_break_row_id = %row_id, 'audit: chain integrity violation detected at startup — investigate before trusting audit log')` exactly once (the early-return ensures at most one break is reported per startup call) and the daemon continues to start. The repeated-startup-alarm issue noted in R10-F1004 is a documentation concern, not a runtime deduplication concern — address it in `core/README.md` rather than in the startup code." This removes dead code from the specified implementation without changing behavior.

---

## No findings (categories with nothing concrete to raise)

- **Authentication & authorization** — `AuditWriter` is server-internal with no new auth surface in Round 15. The `Actor` enum's type-level constraint on actor identity is fully specified. No auth gap found.
- **Abuse cases** — 10 MB cap (byte-accurate BLOB counting for both `redacted_diff_json` and `notes`), `busy_timeout = 5000`, max 1000 row limit, and `AUDIT_QUERY_ROW_FIXED_OVERHEAD_BYTES` constant close the main abuse vectors. No new path found.
- **Race conditions** — `Mutex<Option<SqliteConnection>>` + `BEGIN IMMEDIATE` serialises all writes. Concurrent-write chain linearisation is specified and tested. No new concurrent race found.
- **Resource exhaustion** — Paginated `chain::verify` (batch 500), 10 MB query cap, `busy_timeout`, and `InvalidTimestamp` guard before mutex lock close the exhaustion paths. No new vector.
- **State machine violations** — Migration 0006 step order (ALTER TABLE → UPDATE → CREATE TRIGGER × 2) is correctly sequenced with the backfill before triggers. No new violation.
- **Rollbacks** — Audit writes are intentionally out-of-band from business transactions. No rollback semantics for audit rows by design.
- **Orphaned data** — Immutability triggers prevent cleanup by design. No orphan accumulation path.
- **Migration hazards** — sqlx wraps each migration file in a transaction by default; the four steps in `0006` are atomic under standard sqlx migration behavior. No partial-apply hazard.
- **Single points of failure** — Connection recovery (R5-F402, R6-F505) and `busy_timeout` (R8-F801) address SPOF vectors. No new SPOF found.
- **Timeouts & retries** — `PRAGMA busy_timeout = 5000` + `BusyTimeout` error caps the wait. No retry loop introduced.
- **Eventual consistency** — Single-process SQLite; no multi-store consistency gap.
- **`prev_hash` chain correctness** — All canonical_json fields, encodings, and key names are fully specified through R14. `chain::verify` semantics (early return, sentinel hash tracking, zero-sentinel OK return) are complete. No new chain logic gap beyond the two High findings above.
- **Data exposure** — `RedactedDiff` opacity invariant is maintained by the type system. `synth:` prefix on fallback correlation IDs is specified. No new exposure vector.
- **Schema/type mismatches** — `AuditOutcome` deserialization (R14-M1), `AuditEvent::FromStr` (R11-F1101), `Actor::from_kind_id` (R12-H2), `canonical_json` key names (R13-M1, R13-H1), and `RedactedDiff` encoding (R11-F1104) are all specified. No new mismatch found beyond those raised above.

---

## Summary

**Critical:** 0  **High:** 2  **Medium:** 2  **Low:** 0

**Top concern:** R15-H1 — `pub(crate)` on `RedactedDiff::from_db_str` is structurally inaccessible from `crates/adapters` (the crate containing `AuditWriter::record` and the startup paginator). An implementer who follows the design literally will get a compile error; every workaround they independently invent either destroys the opacity guarantee or introduces a type divergence that corrupts `canonical_json`. This is the most dangerous finding because it blocks implementation entirely and the workaround space is small but consequential.

**Recommended action before proceeding:** Address R15-H1 (correct the `pub(crate)` visibility specification — it must be `pub` or moved into a bridging function in `core` with `pub` visibility) and R15-H2 (specify that the bespoke Display match block is replaced by `strum::Display`, not coexists with `strum::EnumString`) before implementation begins. Both are blockers. The two Medium findings (F-actor-error-path and F-dedup-vacuous) should also be resolved: the former prevents a realistic failure mode in multi-phase deployments, the latter removes dead code from the specification.
