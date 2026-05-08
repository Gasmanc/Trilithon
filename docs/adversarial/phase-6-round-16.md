# Adversarial Review — Phase 6 — Round 16

**Design summary:** Phase 6 adds a tamper-evident, append-only audit log to Trilithon: an `AuditWriter` with a dedicated serialised SQLite connection, a SHA-256 `prev_hash` chain, a `RedactedDiff` newtype for secrets-safe diffs, an `Actor` enum with `from_kind_id` reverse-mapping and an unknown-kind fallback, startup integrity checks, and a query API with a 10 MB soft cap.

**Prior rounds:** Rounds 1–15 reviewed. All R15 findings are closed in the current design text:
- R15-H1 (`pub(crate)` inaccessible cross-crate): CLOSED — design now mandates `pub fn from_db_str` with the bypass invariant enforced by naming convention and doc comment.
- R15-H2 (bespoke `Display` match alongside `strum::EnumString`): CLOSED — design mandates `#[derive(strum::Display)]` replacing the bespoke match block.
- R15-M1 (`Actor::from_kind_id` error propagated on unknown kind in step 5): CLOSED — design specifies `System { component: "unknown" }` fallback + warn, error not propagated.
- R15-M2 (deduplication set contradicts early-return semantics): CLOSED — design removes the deduplication set; at most one break is reported per startup call by construction.

No open items carried forward from prior rounds.

---

## Round 15 Closure Table

| ID | Severity | Title | Status |
|----|----------|-------|--------|
| R15-H1 | HIGH | `pub(crate) RedactedDiff::from_db_str` inaccessible from `crates/adapters` | CLOSED — redesigned as `pub fn from_db_str`; cross-crate access enabled |
| R15-H2 | HIGH | Bespoke `Display` match + `strum::EnumString` attributes are independent string tables | CLOSED — design mandates `strum::Display` derive replacing the bespoke match |
| R15-M1 | MEDIUM | `Actor::from_kind_id` unknown-kind error propagated in `record` step 5, blocking all writes | CLOSED — fallback to `System { component: "unknown" }` + warn specified |
| R15-M2 | MEDIUM | Startup deduplication set is vacuous given `chain::verify` early-return | CLOSED — deduplication set removed from specification |

---

## Findings

### HIGH — Startup paginator's `Actor::from_kind_id` fallback is unspecified; hash computed by `record` step 5 and hash recomputed by `chain::verify` will diverge for any row with an unrecognized `actor_kind`

**Category:** Logic flaw / documentation trap

**Trigger:** R15-M1 closed by specifying that `AuditWriter::record` step 5, when `Actor::from_kind_id` returns `Err(UnknownActorKind)`, substitutes `Actor::System { component: "unknown".to_owned() }` and logs a warning instead of propagating the error. Step 6 then computes `new_prev_hash = sha256(canonical_json(predecessor_row))` where `predecessor_row.actor = Actor::System { component: "unknown" }`.

This `new_prev_hash` is stored as the `prev_hash` column of the newly inserted row. Its correctness depends on the verification path computing the same `canonical_json` for the same predecessor row.

The verification path is the startup paginator, which reads all rows `ORDER BY rowid ASC` in batches of 500 and passes them to `chain::verify`. The startup paginator must also call `Actor::from_kind_id(actor_kind, actor_id)` to construct each typed `AuditRow { actor: Actor }` for the rows it reads. The design specifies this fallback only inside `AuditWriter::record` step 5 (in `adapters`). The startup paginator's error-handling for `Actor::from_kind_id` failures is not specified anywhere in the design.

Concrete failure sequence: a future phase writes an audit row with `actor_kind = "api-key"`. `record` step 5 reads this as the predecessor, calls `Actor::from_kind_id("api-key", "key-abc")`, gets `Err(UnknownActorKind)`, substitutes `Actor::System { component: "unknown" }`, and computes `new_prev_hash = sha256(canonical_json({ actor_kind: "system", actor_id: "unknown", … }))`. This hash is stored in the next row. At startup, the paginator reads the same predecessor row, calls `Actor::from_kind_id("api-key", "key-abc")`, and — because the design does not specify the fallback for the paginator — either: (a) propagates the error, causing the paginator to fail and preventing `chain::verify` from completing; or (b) applies a different fallback (or no fallback, passing a raw string), computing `canonical_json({ actor_kind: "api-key", actor_id: "key-abc", … })`. The hash diverges from `new_prev_hash` stored in the following row, causing `chain::verify` to report `Err(ChainBroken { … })` for every row following the unrecognized-actor-kind row — a permanent false positive that fires on every daemon restart until the actor kind is recognised.

**Consequence:** Any `audit_log` row with an unrecognized `actor_kind` produces a permanent false `ChainBroken` error on startup. The daemon starts (the design says chain breaks are logged but do not block startup) but emits a chain-integrity alarm on every restart. Operators become desensitised to the alarm, defeating its purpose. If the paginator propagates the error rather than applying a fallback, startup chain verification fails entirely and returns no result — a worse outcome.

**Design assumption violated:** The design assumes the unknown-kind fallback in `record` step 5 and the startup paginator apply identical logic when reconstructing predecessor rows. The fallback is specified only for the `record` step 5 path. The paginator's reconstruction path is not specified, producing implementation-defined behavior at a critical hash-stability boundary.

**Suggested mitigation:** Add to the `SecretFieldRegistry` / `RedactedDiff` task or a new note: "The startup paginator MUST apply the identical `Actor::from_kind_id` fallback as `record` step 5 when reconstructing typed `AuditRow` values from DB rows: if `Actor::from_kind_id` returns `Err(UnknownActorKind)`, substitute `Actor::System { component: 'unknown'.to_owned() }` and log `tracing::warn!(actor_kind = %kind, 'audit paginator: unknown actor_kind in row; using same fallback as record step 5 to preserve hash stability')`. Do NOT propagate the error. This ensures `canonical_json` produces identical output for the same row on both the write path and the verify path." Add a test: a row with a custom `actor_kind` injected directly via raw SQL; assert that `chain::verify` run after a subsequent write returns `Ok(())` — confirming hash stability through the fallback.

---

### MEDIUM — `RedactedDiff::from_db_str` is now `pub`; write-path call sites in current or future crates can construct unredacted `RedactedDiff` values that bypass the redactor without a compile error

**Category:** Abuse case / assumption violation

**Trigger:** R15-H1 was closed by promoting `from_db_str` from `pub(crate)` to `pub` because `pub(crate)` cannot cross crate boundaries. The design acknowledges: "The bypass invariant is enforced by naming convention + doc comment, not by visibility restriction." This is a deliberate trade-off.

However, the `RedactedDiff` task says: "No `From<String>`, no public field. `RedactedDiff` MUST expose: (1) `pub fn as_str(&self) -> &str` — the only public read accessor; (2) `pub fn from_db_str(s: String) -> RedactedDiff` — a public constructor for use ONLY by `record` step 5 and the startup paginator." The entire audit of who calls `from_db_str` must now be done manually at every code review, because the compiler allows any caller in any crate to use it.

Concrete failure sequence: a developer in `crates/cli` (or a future `crates/api-gateway` crate) constructs an `AuditRow` for a non-read-path operation and writes: `redacted_diff: Some(RedactedDiff::from_db_str(raw_diff_json.to_string()))` — perhaps copying what they found in the adapters crate without understanding the "DB read path only" invariant. This call site compiles without error. The raw, unredacted diff JSON is stored in `audit_log.redacted_diff_json`. The redactor is bypassed. The immutable log now permanently contains plaintext secret values. No test catches this because the compile-fail test only asserts `From<String>` is absent — `from_db_str` is a distinct, public, compilable call.

**Consequence:** A plaintext secret written to the immutable `audit_log` cannot be deleted or corrected. The only remediation is the incident-response procedure documented in ADR-0009 ("re-keying and recording the incident in a new audit row, not by rewriting history"). This is the exact hazard H10 scenario the `RedactedDiff` newtype was designed to prevent structurally, but which is now only convention-prevented.

**Design assumption violated:** The design assumes that a `pub` function with a "do not call here" doc comment is sufficient to prevent write-path misuse. The original `RedactedDiff` specification assumed the compiler enforces the boundary; the R15 fix trades compiler enforcement for cross-crate access.

**Suggested mitigation:** One of two approaches: (a) Add a dedicated compile-fail test in `crates/cli` (and any future user-facing crate) that asserts `RedactedDiff::from_db_str` is not called from outside `crates/adapters` — a `compile_error!` proc-macro or a linter rule. This does not add compiler enforcement but adds a CI-visible regression test for misuse. Or (b) Move `from_db_str` into a separate sealed trait `AuditDbReconstructExt` with a blanket impl visible only to `crates/adapters` via a `#[doc(hidden)] pub mod __private` pattern — giving the effect of `pub(adapters)` without the language feature. Document whichever approach is chosen as the replacement for compiler-level enforcement. Either way, the sign-off checklist should add: "No call to `RedactedDiff::from_db_str` exists outside `crates/adapters`" with a `grep` recipe in the justfile parallel to `grep-no-record-audit-event`.

---

### MEDIUM — `canonical_json` key `"occurred_at"` is computed as `occurred_at_ms / 1000` at hash-write time, but the DB `occurred_at` column may differ; startup paginator reads DB `occurred_at` which could diverge if passed to `canonical_json`

**Category:** Logic flaw / schema-type mismatch

**Trigger:** The `AuditRow` struct has `occurred_at_ms: i64` but no `occurred_at` field. `canonical_json` computes `occurred_at = occurred_at_ms / 1000` internally and includes it in the JSON object with key `"occurred_at"`.

At write time: `record` step 7 binds `occurred_at = occurred_at_ms / 1000` to the DB column. The DB `occurred_at` column value equals `occurred_at_ms / 1000`. `canonical_json` for the next row's step 5 reads the predecessor's `occurred_at_ms` from the DB (from `SELECT *`) and derives `occurred_at = occurred_at_ms / 1000`. This is consistent.

However, the startup consistency check (task "Startup guard") does: `SELECT COUNT(*) FROM audit_log WHERE occurred_at != occurred_at_ms / 1000 LIMIT 1` and logs an error if rows are found — but the daemon starts. This means the DB may legitimately contain rows where `occurred_at != occurred_at_ms / 1000` (written by the old `record_audit_event` path before Phase 6). For such rows, the startup paginator reads `occurred_at_ms` from the DB and derives `occurred_at_ms / 1000` — this may not equal the stored `occurred_at`. But since `canonical_json` ignores the stored `occurred_at` and always derives from `occurred_at_ms`, this is consistent between step 5 and `chain::verify`. The design is correct here for the hash computation.

The vulnerability is different: the startup paginator constructs `AuditRow` from DB columns. The design says `AuditRow` has no `occurred_at` field. But the DB `SELECT *` returns an `occurred_at` column. An implementer writing the paginator with `sqlx::query_as!` or a manual `FromRow` derive must either (a) skip the `occurred_at` column (correct), or (b) inadvertently add `occurred_at: i64` to `AuditRow` to make `query_as!` compile. If (b), then a future refactor of `canonical_json` might accidentally use `row.occurred_at` (from the DB) instead of `row.occurred_at_ms / 1000`, producing a different `"occurred_at"` value for rows with inconsistent DB timestamps. This silently corrupts the chain for any row that was written before Phase 6.

**Consequence:** If `AuditRow` acquires an `occurred_at` field (to satisfy `query_as!` macro requirements), the design's invariant that "`canonical_json` derives `occurred_at` from `occurred_at_ms` internally" becomes brittle. A future maintainer who reads `row.occurred_at` instead of `row.occurred_at_ms / 1000` in `canonical_json` produces incorrect hashes for inconsistent pre-Phase-6 rows. Since those rows are immutable and already in the log, the chain breaks permanently. The startup consistency check logs but does not block — so the presence of inconsistent rows is not a startup blocker, but it creates a latent hash-stability hazard.

**Design assumption violated:** The design assumes `AuditRow` will never carry an `occurred_at` field, relying on the implementer to correctly skip this column when using `sqlx`'s `query_as!` macro or equivalent. This assumption is not enforced by the type system and is not explicitly stated as a constraint in the `AuditRow` task.

**Suggested mitigation:** Add to the `AuditRow` task: "The `AuditRow` struct MUST NOT include an `occurred_at: i64` field, even though the `audit_log` DB schema carries an `occurred_at` column. When constructing `AuditRow` values from `SELECT *` results (in `record` step 5 and the startup paginator), the `occurred_at` column MUST be explicitly ignored — do not map it to any `AuditRow` field. If `sqlx::query_as!` requires all returned columns to be mapped, use a projection `SELECT id, caddy_instance_id, correlation_id, occurred_at_ms, actor_kind, actor_id, kind, target_kind, target_id, snapshot_id, redacted_diff_json, redaction_sites, outcome, error_kind, notes, prev_hash` (all columns except `occurred_at`) instead of `SELECT *`. This ensures `canonical_json` always derives `occurred_at` from `occurred_at_ms`, never from a separately tracked field." Add a test: `AuditRow { occurred_at: i64, … }` does not compile (field must not exist).

---

### LOW — `Actor::from_kind_id` unknown-kind fallback produces `actor_id = "unknown"` which collides with a legitimate `System { component: "unknown" }` variant; `chain::verify` cannot distinguish the two

**Category:** Logic flaw

**Trigger:** R15-M1 specifies the fallback: `Actor::System { component: "unknown".to_owned() }`. `to_kind_id()` for `System { component }` returns `("system", component.as_str())`. So the fallback produces `actor_kind = "system"`, `actor_id = "unknown"` in `canonical_json`.

If an application code path legitimately writes an audit row with `Actor::System { component: "unknown".to_owned() }` — for example, a daemon component named `"unknown"` (unlikely but legal per the `Actor::System` definition, which only says `component: String` with no constraint on value), or if the application in a future phase adopts a convention of using `"unknown"` as a sentinel component name — then: (a) the legitimate `System { component: "unknown" }` row and (b) a row written by an unrecognized-actor-kind predecessor produce identical `canonical_json` output. If the DB later contains both kinds of rows, `chain::verify` cannot distinguish a "real unknown" from a "fallback unknown" when replaying the chain. A forensic investigation of a chain break must determine whether a suspicious `actor_id = "unknown"` row is genuine or a fallback artifact — without any indicator in the log.

**Consequence:** Low probability of collision, but the fallback value `"unknown"` is not a sentinel value guaranteed to be non-colliding. A log forensics tool comparing `actor_id = "unknown"` rows cannot determine provenance without external context. If a future phase intentionally uses `component = "unknown"` as a system actor component, the two cases become indistinguishable in the immutable log.

**Design assumption violated:** The design assumes `"unknown"` is a safe non-colliding fallback value for the `System` component. It does not reserve this value or constrain the `System` variant to prohibit `component = "unknown"` at write time.

**Suggested mitigation:** Change the fallback value to a string that is guaranteed non-colliding: use `Actor::System { component: format!("__unknown_actor_kind:{kind}") }` so the fallback encodes the actual unrecognized kind string. This makes fallback rows identifiable in forensics (the `actor_id` column in the DB stores `"__unknown_actor_kind:api-key"` for example), and the fallback remains stable (same substitution for the same unknown kind). The double-underscore prefix marks the value as a sentinel. Update R15-M1's design note accordingly and add an assertion to the unknown-kind fallback test: `actor_id = format!("__unknown_actor_kind:{kind}")`.

---

## No findings (categories with nothing concrete to raise)

- **Authentication & authorization** — `AuditWriter` is server-internal. `Actor` enum type-level constraint on actor identity is fully specified. `from_db_str` bypass risk is raised under Abuse Cases above. No new auth bypass vector.
- **Race conditions** — `Mutex<Option<SqliteConnection>>` + `BEGIN IMMEDIATE` serialises all writes. Concurrent-write chain linearisation is tested. No new race found.
- **Resource exhaustion** — Paginated `chain::verify` (batch 500), 10 MB query cap (byte-accurate BLOB counting for both `redacted_diff_json` and `notes`), `busy_timeout`, and `occurred_at_ms > 0` guard before mutex lock close exhaustion paths. No new vector.
- **State machine violations** — Migration 0006 step order (ALTER TABLE → UPDATE → CREATE TRIGGER × 2) correctly sequenced with backfill before triggers. No new violation.
- **Single points of failure** — Connection recovery (R5-F402, R6-F505) and `busy_timeout` (R8-F801) address SPOF vectors. No new SPOF.
- **Timeouts & retries** — `PRAGMA busy_timeout = 5000` + `BusyTimeout` error caps wait. No retry loop. No new hazard.
- **Rollbacks** — Audit writes are intentionally out-of-band from business transactions. No rollback semantics for audit rows by design.
- **Orphaned data** — Immutability triggers prevent cleanup by design. No orphan accumulation path.
- **Migration hazards** — sqlx wraps each migration file in a transaction by default; the four steps in `0006` are atomic. No partial-apply hazard.
- **Eventual consistency** — Single-process SQLite; no multi-store consistency gap.
- **`prev_hash` chain correctness (excluding paginator fallback above)** — All `canonical_json` fields, encodings, and key names are fully specified through R15. `chain::verify` semantics (early return, sentinel hash tracking) are complete.
- **Data exposure** — `RedactedDiff` opacity is maintained at naming-convention level (compiler enforcement traded for cross-crate access in R15). The `from_db_str` public-visibility risk is raised under Abuse Cases above. `synth:` fallback correlation IDs are specified. No new exposure vector beyond those raised.
- **Schema/type mismatches** — `AuditOutcome` deserialization (R14-M1), `AuditEvent::FromStr` (R11-F1101), `Actor::from_kind_id` (R12-H2), `canonical_json` key names (R13-M1, R13-H1), `RedactedDiff` encoding (R11-F1104), and `strum::Display` replacing bespoke match (R15-H2) are all specified. The `occurred_at` field absence risk is raised under Logic Flaws above.

---

## Summary

**Critical:** 0  **High:** 1  **Medium:** 2  **Low:** 1

**Top concern:** R16-H1 — the `Actor::from_kind_id` unknown-kind fallback is specified only for `AuditWriter::record` step 5 and is not specified for the startup paginator. Because `record` step 5 computes `new_prev_hash` using the substituted `canonical_json`, and `chain::verify` recomputes the same hash using paginator-reconstructed rows, both paths MUST apply identical fallback logic. Without specifying the paginator's fallback, the hash computed at write time and the hash recomputed at verify time will diverge for any row with an unrecognized `actor_kind`, producing permanent false `ChainBroken` alarms on every daemon restart.

**Recommended action before proceeding:** Address R16-H1 (specify the identical `Actor::from_kind_id` fallback in the startup paginator's reconstruction path) before implementation begins — it is a blocker. R16-M1 (grep recipe for `from_db_str` call sites outside `adapters`) and R16-M2 (`SELECT *` vs explicit projection to prevent `occurred_at` field on `AuditRow`) are implementation guidance that should be added to the relevant tasks. R16-L1 (fallback collision with `"unknown"`) is worth addressing by using a prefixed sentinel like `"__unknown_actor_kind:{kind}"` to make forensics unambiguous.
