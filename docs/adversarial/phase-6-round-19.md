# Adversarial Review — Phase 6 — Round 19

**Design summary:** Phase 6 adds a tamper-evident, append-only audit log to Trilithon: an `AuditWriter` with a dedicated serialised SQLite connection, a SHA-256 `prev_hash` chain, a `RedactedDiff` newtype for secrets-safe diffs, an `Actor` enum with `from_kind_id` reverse-mapping and an unknown-kind fallback, startup integrity checks, and a query API with a 10 MB soft cap.

**Prior rounds:** Rounds 1–18 reviewed. All R18 findings are closed in the current design text:
- R18-H1 (`#[strum(to_string = "{0}")]` produces literal `"{0}"` not the inner string): CLOSED — design at line 223 now explicitly states `#[strum(to_string = "{0}")]` emits the literal `"{0}"` and mandates a manual `Display` impl arm for `Unknown`; vocabulary task at line 15 says `#[strum(to_string = "{0}")]` NOT permitted. However, the resolution introduces a new compile-time contradiction (see R19-H1 below).
- R18-M1 (stale R15-M1 note used `"unknown"` instead of `"__unknown:{kind}"`): CLOSED — line 213 says "R15-M1, corrected by R16-L1" and states the sentinel is `"__unknown:{kind}"`. Implementers reading line 213 see the correction.
- R18-L1 (`Unknown.to_string()` not covered by specified test): CLOSED — vocabulary task at line 15 now mandates: "A separate test asserts `AuditEvent::Unknown('tool.future-op'.to_owned()).to_string() == 'tool.future-op'`".

No open items carried forward from prior rounds.

---

## Round 18 Closure Table

| ID | Severity | Title | Status |
|----|----------|-------|--------|
| R18-H1 | HIGH | `#[strum(to_string = "{0}")]` on `AuditEvent::Unknown` produces the literal string `"{0}"`, collapsing all unrecognized kinds to the same canonical form | CLOSED — design mandates manual `Display` impl arm; targeted test specified |
| R18-M1 | MEDIUM | Stale R15-M1 note states `"unknown"` fallback; R16-H1/R16-L1 note states `"__unknown:{kind}"`; contradiction at hash-stability boundary | CLOSED — line 213 notes the correction explicitly |
| R18-L1 | LOW | `AuditEvent::Unknown.to_string()` not covered by any specified test | CLOSED — vocabulary task now requires a targeted test for this path |

---

## Findings

### HIGH — `#[derive(strum::Display)]` and a manual `impl fmt::Display for AuditEvent` cannot coexist in Rust; the vocabulary consolidation task mandates both, producing a compile error

**Category:** Documentation trap / logic flaw

**Trigger:** The vocabulary consolidation task on line 14 contains a hard acceptance requirement: "**Replace the existing bespoke `impl fmt::Display for AuditEvent { fn fmt(...) { match ... } }` with `#[derive(strum::Display)]`**". An implementer following this acceptance criterion uses `#[derive(strum::Display)]`, which causes the Rust compiler to emit `impl fmt::Display for AuditEvent` as a generated item.

The "Done when" criteria on line 15 then require: "The `Unknown(String)` variant MUST have a manual `Display` impl arm (`AuditEvent::Unknown(s) => write!(f, "{}", s)`)". A "manual `Display` impl arm" for a specific variant of `AuditEvent` can only be expressed inside a `impl fmt::Display for AuditEvent { fn fmt(&self, f: ...) { match self { ... } } }` block.

In Rust, a type can have at most one implementation of any given trait in its defining crate. Having both `#[derive(strum::Display)]` (which generates `impl fmt::Display for AuditEvent`) and a hand-written `impl fmt::Display for AuditEvent` in the same compilation unit is a compile error: `error[E0119]: conflicting implementations of trait 'std::fmt::Display' for type 'AuditEvent'`. The compiler rejects the file before any test can run.

Concrete failure sequence: implementer reads line 14, adds `#[derive(strum::EnumString, strum::EnumIter, strum::EnumCount, strum::Display)]` to `AuditEvent`, deletes the existing manual `impl fmt::Display`. They then read line 15 and see the `Unknown` variant requires a manual arm. They write `impl fmt::Display for AuditEvent { fn fmt(...) { match self { AuditEvent::Unknown(s) => write!(f, "{}", s), _ => <strum_generated_display>(self, f) } } }`. This does not compile — strum has already generated the full `impl fmt::Display` for the type. No workaround within the stated task constraints compiles.

The `or a manual Display impl` parenthetical at the end of line 15's last sentence ("Add `strum::EnumString`, `strum::EnumIter`, `strum::EnumCount`, and `strum::Display` (**or a manual `Display` impl**) to the `strum` feature list") acknowledges the alternative, but line 14 already explicitly mandated replacing the bespoke impl with `#[derive(strum::Display)]`. These two instructions are in direct conflict, and no guidance specifies which takes precedence when they conflict.

The R18-H1 mitigation on line 223 describes "a hybrid: `#[derive(strum::Display)]` drives all named variants [...] and a manual `impl fmt::Display for AuditEvent` override handles the `Unknown` arm only." This hybrid is impossible to compile in Rust — the derive generates the full trait impl, and no partial override mechanism exists.

**Consequence:** Any implementer who follows line 14's acceptance criterion literally (`#[derive(strum::Display)]`) and then follows line 15's "Done when" requirement (manual `Display` impl arm) produces a file that does not compile. The only escape is to ignore line 14's explicit replacement mandate and write a fully manual `impl fmt::Display` covering all 44 named variants — but this reintroduces the dual-string-table problem that line 14's mandate was designed to eliminate (the bespoke match block can diverge from the `strum::EnumString` attributes). The design provides no path that simultaneously (a) uses `#[derive(strum::Display)]` for named variants, (b) provides correct `to_string()` for `Unknown`, and (c) compiles.

**Design assumption violated:** The design assumes that strum's `#[derive(Display)]` can be combined with a hand-written `impl fmt::Display` on the same type, allowing the derive to handle named variants and the manual impl to handle `Unknown`. Rust does not permit two implementations of the same trait for the same type in the same crate. This is a fundamental language constraint, not a strum-specific limitation.

**Suggested mitigation:** Amend the vocabulary consolidation task acceptance criterion on line 14 to replace the current instruction with one of two mutually exclusive approaches, specified unambiguously:

Option A (preferred — eliminates dual-string-table risk for named variants): "Do NOT use `#[derive(strum::Display)]`. Instead, write a **single manual `impl fmt::Display for AuditEvent`** covering all variants: named variants use the same string as their `#[strum(serialize = '...')]` attribute (a comment in the match arm must cite the attribute value to keep them in sync); `Unknown(s) => write!(f, '{}', s)`. The round-trip test validates that `to_string().parse::<AuditEvent>()` round-trips correctly for all named variants via `AuditEvent::iter()`, which verifies that the manual `Display` strings and the `strum::EnumString` parse attributes are consistent."

Option B (alternative if strum::Display is strongly preferred): "Use `#[derive(strum::Display)]` for all named variants. Handle `Unknown` by NOT deriving `strum::Display` directly on `AuditEvent`; instead, annotate with `#[strum(display)]` on each named variant only (strum 0.26+ supports `#[strum(to_string = '...')]` per-variant within a derive). Write a wrapper function `fn kind_str(&self) -> Cow<'_, str>` that returns `Cow::Borrowed(self.to_string())` for named variants and `Cow::Borrowed(s.as_str())` for `Unknown(s)`. Replace all `row.kind.to_string()` calls in `canonical_json` with `row.kind.kind_str()`. Do not derive or implement `fmt::Display` for `AuditEvent` at all." Note that Option B changes the public API of `canonical_json` and `AuditEvent`.

Remove the R18-H1 design decision note description of the impossible "hybrid" approach from line 223 and replace it with whichever option is chosen above.

---

### MEDIUM — `SELECT *` in `AuditWriter::record` step 5 faces the same `occurred_at` field-mapping problem as the startup paginator, but only the startup paginator task mentions the named-column-list fix

**Category:** Schema/type mismatch / documentation trap

**Trigger:** The design specifies (line 82) that the startup paginator must avoid mapping the `occurred_at` column from the DB to a field on `AuditRow` (which has no such field), and directs: "Use a named column list in the startup paginator query if the sqlx `FromRow` derive would otherwise map unknown columns to a struct error."

The same structural problem exists for `AuditWriter::record` step 5: the design mandates `SELECT * FROM audit_log ORDER BY rowid DESC LIMIT 1` (line 64). The `audit_log` table has an `occurred_at` column (present since migration `0001_init.sql`, line 73 of that file). `AuditRow` has no `occurred_at` field — only `occurred_at_ms`. When sqlx maps the `SELECT *` result to an `AuditRow` via `FromRow`, it encounters the `occurred_at` column with no matching field. In sqlx 0.7+, the default `FromRow` derive fails with an error if a column in the query result has no corresponding field in the struct, unless the struct explicitly uses `#[sqlx(flatten)]` or the query uses a named projection.

The fix (named column list instead of `SELECT *`) is specified only for the startup paginator. Step 5 is instructed to use `SELECT *` explicitly, with the comment that the startup paginator should use a named list "if the sqlx FromRow derive would otherwise map unknown columns to a struct error." But step 5 faces the identical sqlx constraint; the design does not say step 5 should also use a named column list.

Concrete failure sequence: implementer writes step 5 using `sqlx::query_as::<_, AuditRow>("SELECT * FROM audit_log ORDER BY rowid DESC LIMIT 1")`. The `AuditRow` struct derives `#[derive(sqlx::FromRow)]`. At runtime (or compile time with sqlx's offline mode), sqlx reports that the `occurred_at` column has no matching field. The query fails. Either the query is never reached (a compile error in offline mode with a saved query) or it panics at runtime on the first call to `record` after the table has any rows. Because this is the predecessor-fetch step inside a `BEGIN IMMEDIATE` transaction, the failure causes step 9 (the error recovery path) to fire — connection is closed and reopened. Subsequent calls also fail the same way, leaving the audit log unwritable.

**Consequence:** `AuditWriter::record` step 5 fails on every call where the `audit_log` table is non-empty. The predecessor is never fetched. `canonical_json` is never computed for the predecessor. The error-recovery path fires repeatedly, cycling through connection close and reopen. The audit log is permanently unwritable after the first row is inserted. The chain is broken after exactly one row.

**Design assumption violated:** The design assumes that the sqlx `occurred_at` field-mapping problem is unique to the startup paginator and that step 5's `SELECT *` is unaffected. In fact, sqlx's `FromRow` derive applies the same strict column-to-field mapping in both contexts.

**Suggested mitigation:** Apply the same fix to step 5 that the design already specifies for the startup paginator: replace `SELECT * FROM audit_log ORDER BY rowid DESC LIMIT 1` with a named column list that enumerates all columns except `occurred_at`. The column list is exactly the set of fields in `AuditRow`: `id, caddy_instance_id, correlation_id, occurred_at_ms, actor_kind, actor_id, kind, target_kind, target_id, snapshot_id, redacted_diff_json, redaction_sites, outcome, error_kind, notes, prev_hash`. Add this named list to the step 5 specification in the `AuditWriter` task. Add the same note that is given for the startup paginator: "The `occurred_at` column value is silently excluded from the projection; `AuditRow` has no such field; it is computed by `canonical_json` from `occurred_at_ms / 1000`." Add to the "Done when" criteria: "step 5 uses a named column projection, not `SELECT *`, to avoid a sqlx FromRow error on the `occurred_at` column."

---

## No findings (categories with nothing concrete to raise)

- **Authentication & authorization** — `AuditWriter` is server-internal. No public-facing bypass vector.
- **Abuse cases** — 10 MB cap (BLOB-accurate), max 1000 rows, `busy_timeout = 5000`, `occurred_at_ms > 0` guard before mutex lock, and named constant `AUDIT_QUERY_ROW_FIXED_OVERHEAD_BYTES` are fully specified. No new path.
- **Race conditions** — `Mutex<Option<SqliteConnection>>` + `BEGIN IMMEDIATE` serialises all writes. Concurrent-write chain linearisation is specified and tested (10 concurrent writes × 10 repetitions). No new gap.
- **Resource exhaustion** — Paginated `chain::verify` (batch 500), 10 MB cap, `busy_timeout`. No new vector.
- **State machine violations** — Migration 0006 step order (ALTER TABLE → backfill UPDATE → CREATE TRIGGER × 2) is correctly sequenced: backfill before triggers so the UPDATE is not blocked by the immutability trigger. `SecretsRevealed` guard is step 1 (before mutex). `InvalidTimestamp` guard is step 1b (before mutex). Migration file is atomic (sqlx wraps each migration in a transaction). No new violation.
- **Single points of failure** — Connection recovery (R5-F402, R6-F505), `busy_timeout`, and `PRAGMA foreign_keys = ON` on initial and recovery opens address SPOF vectors. The `ConnectionRecoveryFailed` error correctly surfaces both the original write error and the recovery error. No new SPOF.
- **Timeouts & retries** — `busy_timeout` at 5000ms + `BusyTimeout` error return; test (h) verifies the ~6s bound. No retry loop. No new hazard.
- **Rollbacks** — Audit writes are out-of-band from business transactions by design. No rollback semantics for audit rows.
- **Orphaned data** — Immutability triggers prevent cleanup. No orphan accumulation path.
- **`chain::verify` completeness** — Early-return semantics (R9-F903), ZERO_SENTINEL tracking (R6-F502), `EmptyHash` guard, `Ok(())` on all-sentinel table, tamper-sentinel test, and 3-row break test are all specified. The `Unknown` actor-kind fallback and `Unknown` kind-string fallback are both specified for both call sites (step 5 and startup paginator). No new chain logic gap.
- **`canonical_json` stability** — All 17 key names documented (including `"redacted_diff_json"` not `"redacted_diff"`), `occurred_at` computed inside from `occurred_at_ms / 1000`, JSON null for None, sorted keys, no whitespace. Stability test (criterion h) specified with specific assertions. No new gap.
- **`AuditEvent::Unknown` Display correctness** — R18-H1 and R18-L1 both closed: design mandates manual `Display` arm and targeted test. R19-H1 identifies that the vocabulary task acceptance criterion (line 14) still contradicts this mandate, but the `Unknown` Display behavior itself is now correctly specified.
- **`actor_kind` constraint** — `Actor` is a closed Rust enum; `to_kind_id()` returns only `"user"`, `"token"`, or `"system"` by construction. The application-layer step 7 check is redundant-but-harmless defense-in-depth. No new gap.
- **`AuditOutcome` serde encoding** — `#[serde(rename_all = "lowercase")]` on both `Serialize` and `Deserialize` ensures lowercase `"ok"`, `"error"`, `"denied"` in canonical_json and in DB reconstruction. Stability test asserts this. No new gap.
- **`synth:` correlation ID fallback** — Both empty and absent span paths produce `synth:<ulid>` with `tracing::warn!`. Stored in DB. Aggregate queries using `LIKE 'synth:%'` documented. No new gap.
- **Migration hazards** — 0006 is atomic; backfill happens before trigger creation. No partial-apply hazard. No new gap.
- **Eventual consistency** — Single-process SQLite; no multi-store gap.

---

## Summary

**Critical:** 0  **High:** 1  **Medium:** 1  **Low:** 0

**Top concern:** R19-H1 — The vocabulary consolidation task simultaneously mandates `#[derive(strum::Display)]` (line 14) and a manual `impl fmt::Display for AuditEvent` arm for `Unknown` (line 15). These cannot coexist in Rust; the derive generates the full trait implementation, making a subsequent manual impl a compile error `E0119`. No implementer can follow both requirements. The design must specify exactly one approach: either a fully manual `impl fmt::Display` (safe, recommended) or a non-Display strum derive with a wrapper method for `canonical_json`.

**Recommended action before proceeding:** Address R19-H1 (resolve the compile-time contradiction in the vocabulary consolidation task by choosing one Display strategy and stating it unambiguously; remove the description of the impossible "hybrid" approach from the R18-H1 design decision note). Address R19-M1 (extend the step 5 `SELECT *` specification to use a named column projection, matching the fix already specified for the startup paginator). Both are straightforward textual fixes to the design document.
