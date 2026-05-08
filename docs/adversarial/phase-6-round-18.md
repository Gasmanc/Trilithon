# Adversarial Review — Phase 6 — Round 18

**Design summary:** Phase 6 adds a tamper-evident, append-only audit log to Trilithon: an `AuditWriter` with a dedicated serialised SQLite connection, a SHA-256 `prev_hash` chain, a `RedactedDiff` newtype for secrets-safe diffs, an `Actor` enum with `from_kind_id` reverse-mapping and an unknown-kind fallback, startup integrity checks, and a query API with a 10 MB soft cap.

**Prior rounds:** Rounds 1–17 reviewed. All R17 findings are closed in the current design text:
- R17-H1 (`AuditEvent::from_str` fallback unspecified in startup paginator): CLOSED — design now mandates `AuditEvent::Unknown(String)` with `#[strum(disabled)]` as the fallback catch-all variant; `unwrap_or_else(|_| AuditEvent::Unknown(kind_str.to_owned()))` specified for both `record` step 5 and the startup paginator; `canonical_json` calls `row.kind.to_string()` so the original string must pass through unchanged.
- R17-M1 (ZERO_SENTINEL documentation overstated cumulative coverage): CLOSED — design now explicitly documents that only the last sentinel row's content is cryptographically protected; `core/README.md` documentation requirement added.
- R17-L1 (`PRAGMA foreign_keys = ON` omitted from dedicated audit connection): CLOSED — design now mandates `PRAGMA foreign_keys = ON` on both initial open and recovery reopens of the dedicated `AuditWriter` connection.

No open items carried forward from prior rounds.

---

## Round 17 Closure Table

| ID | Severity | Title | Status |
|----|----------|-------|--------|
| R17-H1 | HIGH | Startup paginator has no specified fallback for unrecognized `AuditEvent` kind strings | CLOSED — `AuditEvent::Unknown(String)` with `#[strum(disabled)]` mandated; `unwrap_or_else` specified for both call sites |
| R17-M1 | MEDIUM | ZERO_SENTINEL multi-row tamper-detection guarantee overstated | CLOSED — documentation updated to accurately reflect last-row-only coverage; `core/README.md` requirement added |
| R17-L1 | LOW | `PRAGMA foreign_keys = ON` omitted from dedicated audit connection | CLOSED — mandated on initial open and recovery reopens |

---

## Findings

### HIGH — `#[strum(to_string = "{0}")]` on `AuditEvent::Unknown` produces the literal string `"{0}"` not the inner string; every unrecognized kind hashes as `"{0}"`, collapsing the fallback

**Category:** Logic flaw / documentation trap

**Trigger:** R17-H1 closed by adding `AuditEvent::Unknown(String)` with `#[strum(disabled)]` and specifying that `Unknown("some.future.kind").to_string()` MUST return `"some.future.kind"` (the inner string unchanged). The design states this is achieved via `#[strum(to_string = "{0}")]` on the variant: "which strum's Display derive supports via `#[strum(to_string = "{0}")]` on the variant."

This claim is incorrect. Strum's `EnumDisplay` derive processes `#[strum(to_string = "...")]` as a static string literal, not as a Rust format template. The `{0}` in `#[strum(to_string = "{0}")]` is stored and emitted verbatim — it is not interpreted as a format specifier referencing the tuple field. The output of `AuditEvent::Unknown("system.restore-applied".to_owned()).to_string()` with this attribute is the literal string `"{0}"`, not `"system.restore-applied"`.

Concrete failure sequence: a future phase writes an audit row with `kind = "system.restore-applied"`. Phase 6's `record` step 5 reads this as the predecessor, parses `"system.restore-applied"` via `kind_str.parse::<AuditEvent>().unwrap_or_else(|_| AuditEvent::Unknown(kind_str.to_owned()))`, obtaining `AuditEvent::Unknown("system.restore-applied".to_owned())`. `canonical_json` calls `row.kind.to_string()`. With `#[strum(to_string = "{0}")]`, this returns `"{0}"`. The `canonical_json` output includes `"kind": "{0}"`. The `new_prev_hash` is computed from this JSON. The next row stores `prev_hash = sha256(... "kind": "{0}" ...)`.

At startup, the paginator reconstructs the same predecessor row, again obtaining `AuditEvent::Unknown("system.restore-applied")` with `to_string()` returning `"{0}"`. The hash recomputed by `chain::verify` includes `"kind": "{0}"` — identical to the write-path computation. So `chain::verify` passes for this row.

However, ALL unrecognized kind strings collapse to the single string `"{0}"` in `canonical_json`. A row with `kind = "system.restore-applied"` and a row with `kind = "system.restore-cross-machine"` both produce `"kind": "{0}"` in their canonical JSON. If both appear as predecessor rows in the chain, their canonical JSON is identical for the `kind` field. If an attacker who has write access to the SQLite file swaps the content of one row for the other (changing the kind and other fields while preserving the `id` and `prev_hash`), the swap may produce the same `canonical_json` for the kind field — partially defeating the tamper-detection guarantee for rows with unrecognized kinds. More concretely: forensic tooling that reads `audit_log` and recomputes canonical JSON using the stored `kind` TEXT value directly (not via the `AuditEvent` enum) will compute `"kind": "system.restore-applied"`, producing a different hash than the stored `prev_hash` value — a false positive chain break in any external verifier.

Additionally: if a future Trilithon version recognizes the previously-unknown kind (adds the variant to `AuditEvent`), then `canonical_json` for that variant now produces `"kind": "system.restore-applied"` via strum's Display. The stored `prev_hash` of the following row was computed with `"kind": "{0}"`. Chain verification now fails permanently for the transition row — a permanent false `ChainBroken` after upgrading Trilithon.

**Consequence:** Every audit row whose `kind` was unrecognized at write time uses `"kind": "{0}"` in its canonical hash. (1) External verifiers disagree with the stored chain. (2) A Trilithon upgrade that recognizes a previously-unknown kind permanently breaks chain verification for rows written during the transition window. (3) Multiple rows with different unrecognized kinds are indistinguishable in the `canonical_json` kind field.

**Design assumption violated:** The design assumes that strum's `EnumDisplay` derive processes `{0}` as a format specifier referencing the tuple struct's first field. Strum's `to_string` attribute expects a literal string; format-specifier interpolation is not part of strum's Display derive API. The correct implementation requires a manual `impl fmt::Display for AuditEvent` that matches the `Unknown(s)` arm and writes `s` directly.

**Suggested mitigation:** Remove `#[strum(to_string = "{0}")]` from the `Unknown` variant specification. Instead, specify that because `strum::Display` cannot produce dynamic content for `Unknown`, the `AuditEvent` Display implementation MUST NOT be a pure `#[derive(strum::Display)]` — it must be a hybrid: `#[derive(strum::Display)]` drives all named variants (so they share their string mappings with `strum::EnumString`), and a manual `impl fmt::Display for AuditEvent` override handles the `Unknown` arm only: `AuditEvent::Unknown(s) => write!(f, "{}", s)`. Alternatively, add a wrapper method `fn kind_str(&self) -> &str` that returns the stored string for `Unknown` and `self.to_string().as_str()` for named variants, then use `kind_str()` in `canonical_json` instead of `to_string()`. Add a targeted test: `assert_eq!(AuditEvent::Unknown("sys.future".to_owned()).to_string(), "sys.future")` — this test fails with the `#[strum(to_string = "{0}")]` approach and passes with the manual impl.

---

### MEDIUM — R15-M1 design decision note specifies `"unknown"` as the `Actor::from_kind_id` fallback sentinel; R16-H1/R16-L1 note and implementation tasks specify `"__unknown:{kind}"`; the contradiction will cause one or both call sites to implement the wrong sentinel

**Category:** Documentation trap / composition failure

**Trigger:** The "Design decisions recorded here" section contains two notes that give contradictory fallback values for the `Actor::from_kind_id` unknown-kind case:

- The R15-M1 note (current design text line 213): "`Actor::from_kind_id` unknown-kind fallback is `System { component: 'unknown' }` + warn; error is NOT propagated." The component string is `"unknown"`.
- The R16-H1/R16-L1 note (current design text line 217): "Startup paginator applies same `Actor::from_kind_id` fallback as `record` step 5; sentinel is `'__unknown:{kind}'`." The component string is `"__unknown:{kind}"` (encoding the original kind).

The implementation tasks (the `AuditWriter::record` step 5 specification and the `chain::verify` task for the startup paginator) correctly say `"__unknown:{kind}"`. However, an implementer who reads the Design Decisions section to understand the design rationale (a natural reading order before diving into task acceptance criteria) encounters R15-M1 first and sees `"unknown"` as the stated fallback value. When they reach R16-H1/R16-L1, they see `"__unknown:{kind}"` with a reference to the startup paginator only. The R15-M1 entry is not marked superseded or amended.

Concrete consequence: an implementer implements `record` step 5 using `"unknown"` (from R15-M1) and the startup paginator using `"__unknown:{kind}"` (from R16-H1). Both call sites apply a different component string for the same unknown actor kind. `canonical_json` for the predecessor row computed in `record` step 5 produces `"actor_id": "unknown"`. `canonical_json` recomputed by `chain::verify` produces `"actor_id": "__unknown:api-key"`. The hashes diverge. Every row following a row with an unrecognized `actor_kind` fails chain verification — a permanent false `ChainBroken` on every daemon restart.

**Consequence:** The R15-M1 design note is stale: it was written when `"unknown"` was the specified sentinel, before R16-L1 changed it to `"__unknown:{kind}"`. An implementer who treats the Design Decisions section as authoritative (it is labeled as "Design decisions recorded here" and is placed prominently) will implement `record` step 5 with `"unknown"` while the startup paginator uses `"__unknown:{kind}"`. The resulting hash divergence produces permanent false chain breaks after any future phase uses a new actor kind.

**Design assumption violated:** The design assumes that all specification artifacts (task acceptance criteria and Design Decisions notes) are consistent. The R15-M1 note was not updated when R16-L1 changed the fallback sentinel. The resulting contradiction is an ambiguity at a hash-stability-critical boundary.

**Suggested mitigation:** Amend the R15-M1 design decision note to read: "`Actor::from_kind_id` unknown-kind fallback is `System { component: format!('__unknown:{}', kind) }` + warn; error is NOT propagated. (The sentinel was initially `'unknown'`; updated in R16-L1 to encode the original kind string for forensic identification and to avoid collision with legitimate `System { component: 'unknown' }` rows.)" Mark the R16-H1/R16-L1 note as the authoritative specification for both call sites. Add to the sign-off checklist: "Both `AuditWriter::record` step 5 and the startup paginator apply identical `'__unknown:{kind}'` substitution for unrecognized `actor_kind` values — grep confirms no `'unknown'` literal is used as the fallback component string."

---

### LOW — `AuditEvent::Unknown(String)` excluded from the exhaustive round-trip test by `#[strum(disabled)]`; the `to_string()` correctness of the fallback variant is not covered by any specified test

**Category:** Test coverage gap

**Trigger:** The vocabulary consolidation task mandates an exhaustive round-trip test: `for variant in AuditEvent::iter() { assert_eq!(variant.to_string().parse::<AuditEvent>().unwrap(), variant); }`. `AuditEvent::iter()` is provided by `strum::EnumIter`. Because `Unknown(String)` is annotated with `#[strum(disabled)]`, it is excluded from `EnumIter` and is therefore excluded from the round-trip test. This exclusion is intentional (parsing `Unknown`'s display string back to `Unknown` via `from_str` would fail because `Unknown` is disabled for `EnumString`). The exclusion is correct from a round-trip perspective but it means no specified test exercises `AuditEvent::Unknown("any.string").to_string()`.

If the `Unknown` variant's `Display` implementation is incorrect — producing `"{0}"` instead of the inner string (as raised in R18-H1 above), or producing `""`, or panicking — no specified test catches it. The only code path that exercises `Unknown.to_string()` is the `canonical_json` function itself, but the `canonical_json` stability test (criterion h) uses a fixed `AuditRow` with known variants (named `AuditEvent` variants, not `Unknown`). The stability test never constructs an `AuditRow { kind: AuditEvent::Unknown("any".to_owned()), … }`.

This is distinct from R18-H1 (which identifies the root cause — strum's `to_string` attribute not supporting `{0}` interpolation). Even if the implementation is correct (using a manual `Display` impl for the `Unknown` arm), the absence of a specified test means a future refactor that breaks `Unknown.to_string()` is undetected until production.

**Consequence:** A regression in `AuditEvent::Unknown`'s `to_string()` behavior (e.g., from a strum version bump that changes the `to_string` attribute semantics, or from a manual impl that is accidentally removed during a merge) causes `canonical_json` to encode a wrong `"kind"` value for all rows with unrecognized kinds. Chain verification fails permanently for rows written after the regression. No specified test catches this before the daemon ships.

**Design assumption violated:** The design assumes that the `AuditEvent::iter()`-driven round-trip test provides full coverage of `to_string()` correctness. Because `Unknown` is excluded from `iter()` by design, the fallback variant's `to_string()` is a testing blind spot.

**Suggested mitigation:** Add to the vocabulary consolidation task's "Done when" criteria: "A targeted test MUST assert `AuditEvent::Unknown('synthetic.future.kind'.to_owned()).to_string() == 'synthetic.future.kind'` — verifying that the `Unknown` variant's `Display` implementation returns the inner string unchanged, not a literal `'{0}'` or empty string. This test is separate from the round-trip test (which excludes `Unknown` by design) and exercises the fallback Display path directly."

---

## No findings (categories with nothing concrete to raise)

- **Authentication & authorization** — `AuditWriter` is server-internal. No new auth bypass vector found.
- **Abuse cases** — 10 MB cap (byte-accurate BLOB counting for `redacted_diff_json` and `notes`), max 1000 row limit, `AUDIT_QUERY_ROW_FIXED_OVERHEAD_BYTES` constant, `busy_timeout = 5000`, and `occurred_at_ms > 0` guard before mutex lock close the main abuse vectors. No new path found.
- **Race conditions** — `Mutex<Option<SqliteConnection>>` + `BEGIN IMMEDIATE` serialises all writes. Concurrent-write chain linearisation is specified and tested. No new race found.
- **Resource exhaustion** — Paginated `chain::verify` (batch 500), 10 MB query cap, `busy_timeout`, and `InvalidTimestamp` guard close the exhaustion paths. No new vector.
- **State machine violations** — Migration 0006 step order (ALTER TABLE → UPDATE → CREATE TRIGGER × 2) correctly sequenced with backfill before triggers. `SecretsRevealed` guard in `record` is specified before any mutex lock. No new violation.
- **Single points of failure** — Connection recovery (R5-F402, R6-F505), `busy_timeout` (R8-F801), and `PRAGMA foreign_keys = ON` on both initial and recovery opens (R17-L1) address SPOF vectors. No new SPOF.
- **Timeouts & retries** — `PRAGMA busy_timeout = 5000` + `BusyTimeout` error caps the wait. No retry loop. No new hazard.
- **Rollbacks** — Audit writes are intentionally out-of-band from business transactions. No rollback semantics for audit rows by design.
- **Orphaned data** — Immutability triggers prevent cleanup by design. No orphan accumulation path.
- **Migration hazards** — sqlx wraps each migration file in a transaction; the four steps in `0006` are atomic. No partial-apply hazard.
- **Eventual consistency** — Single-process SQLite; no multi-store consistency gap.
- **Schema/type mismatches** — `AuditOutcome` deserialization (R14-M1), `Actor::from_kind_id` (R12-H2), `canonical_json` key names (all 17 keys specified), `RedactedDiff` encoding (R11-F1104), and `strum::Display` replacing bespoke match (R15-H2) are all specified. The `Unknown` variant Display correctness is raised as R18-H1.
- **`chain::verify` completeness** — Early-return semantics (R9-F903), ZERO_SENTINEL tracking (R6-F502), `EmptyHash` guard, `Ok(())` on all-sentinel table, tamper-sentinel test, and 3-row break test with row 3 not reported are all specified. No new chain logic gap.
- **`actor_kind` CHECK constraint** — The design specifies that `AuditWriter::record` step 7 rejects rows where `row.actor.to_kind_id().0 NOT IN ("user", "token", "system")` at INSERT time, documented as an application-layer constraint because SQLite ALTER TABLE cannot add CHECK constraints. No new gap.
- **`AUDIT_KIND_VOCAB` compile-time assertion** — The UFCS assertion `<AuditEvent as strum::EnumCount>::COUNT == AUDIT_KIND_VOCAB.len()` counts 44 named variants (excluding `Unknown` via `#[strum(disabled)]`). The vocab list has 44 entries after adding the three new Phase-6 variants. The assertion correctly validates that every named variant has a corresponding vocabulary entry. No gap — the `Unknown` variant is correctly excluded from both sides of this assertion.
- **`cancel safety` documentation** — `AuditWriter` doc comment correctly distinguishes happy path (cancel-safe, slot remains `Some`) from error-recovery path (not cancel-safe, slot may be `None` after drop during `connect()`). No new gap.

---

## Summary

**Critical:** 0  **High:** 1  **Medium:** 1  **Low:** 1

**Top concern:** R18-H1 — the design specifies `#[strum(to_string = "{0}")]` on `AuditEvent::Unknown(String)` to make `to_string()` return the inner string, but strum's `to_string` attribute treats its value as a static literal, not a format template. Every unrecognized `kind` value hashes as the literal string `"{0}"` in `canonical_json` rather than the actual kind string. This collapses all unrecognized kinds to the same canonical form, causing (a) external verifiers to compute different hashes than stored `prev_hash` values, and (b) permanent `ChainBroken` errors after a Trilithon upgrade that adds recognition for a previously-unknown kind.

**Recommended action before proceeding:** Address R18-H1 (replace `#[strum(to_string = "{0}")]` with a manual `Display` implementation for the `Unknown` arm alongside the derived `strum::Display` for named variants; add a targeted test asserting `Unknown("x").to_string() == "x"`) before implementation begins — it is a blocker. Address R18-M1 (amend the stale R15-M1 design decision note to match the `"__unknown:{kind}"` sentinel now specified in R16-H1/R16-L1 and the task acceptance criteria) to prevent one of the two critical hash-stability call sites from using the wrong fallback string. R18-L1 (targeted test for `Unknown.to_string()`) closes naturally alongside R18-H1's mitigation.
