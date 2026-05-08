# Adversarial Review — Phase 6 — Round 12

**Design summary:** Phase 6 adds a tamper-evident, append-only audit log to Trilithon: an `AuditWriter` with a dedicated serialised SQLite connection, a SHA-256 `prev_hash` chain, a `RedactedDiff` newtype for secrets-safe diffs, an `Actor` enum, startup integrity checks, and a query API with a 10 MB soft cap.

**Prior rounds:** Rounds 1–11 reviewed. R11 findings F1103 and F1104 remain open in the current design text (the design decisions section records R11-F1101 and R11-F1102 as resolved but does not address F1103 or F1104). All other prior items are closed.

---

## Findings

### HIGH — `AuditRow.prev_hash` is a caller-supplied field but `record` computes a different value; INSERT step does not say to bind the computed value, not the struct field
**Category:** Logic flaw / documentation trap

**Trigger:** `AuditRow` carries `prev_hash: String` as a required, caller-visible field. The `record` step sequence computes the correct `prev_hash` from the DB (step 5: `SELECT prev_hash … ORDER BY rowid DESC LIMIT 1`; step 6: `new_prev_hash = sha256(canonical_json(predecessor))`). The INSERT step (7) lists the columns to bind but never says "use `new_prev_hash` for the `prev_hash` column, NOT `row.prev_hash`." An implementer writing the INSERT who binds all `AuditRow` fields uniformly — a natural pattern when mapping structs to INSERT statements — will bind `row.prev_hash` (the caller-supplied value, which could be `""`, `ZERO_SENTINEL`, or any stale string the caller happened to put there) instead of the DB-computed `new_prev_hash`. The INSERT succeeds (the trigger only blocks UPDATE and DELETE); the immutability triggers prevent correction after the fact.

**Consequence:** Every row written through this implementation has a `prev_hash` value that is not derived from the predecessor — the chain is immediately and permanently broken. `chain::verify` returns `Err(ChainBroken)` on the second row. The daemon starts anyway (chain errors are non-fatal at startup). The entire tamper-evidence guarantee silently fails from the first write.

**Design assumption violated:** The design assumes the implementer understands that `row.prev_hash` is ignored by `record` and replaced by the DB-computed value. This assumption is not stated anywhere — `prev_hash` is listed as a struct field like every other field, with no annotation that its value is discarded at write time.

**Suggested mitigation:** Either (a) remove `prev_hash` from `AuditRow` entirely — it is never populated by the caller and should not exist on the write-model struct; `record` computes and writes it internally, so the only place it belongs is in a DB-read struct used by the startup paginator; or (b) if `prev_hash` must remain on `AuditRow` for the startup paginator's read path, split the write and read types: `AuditRow` for writes (no `prev_hash` field), `StoredAuditRow` or equivalent for reads (includes `prev_hash`). If option (b) is impractical, at minimum add a note to the INSERT step (7): "bind `new_prev_hash` (computed in step 6) to the `prev_hash` column — do NOT bind `row.prev_hash`."

---

### HIGH — No `Actor` deserialization path specified for the startup paginator; paginator reads `actor_kind`/`actor_id` TEXT columns but `AuditRow.actor` is typed `Actor` (an enum), and no reverse mapping is specified
**Category:** Schema/type mismatch / documentation trap

**Trigger:** The startup guard calls `chain::verify` via a paginated loop that must construct `AuditRow` values from raw SQLite rows. The DB stores `actor_kind TEXT NOT NULL` and `actor_id TEXT NOT NULL`. `AuditRow.actor` is typed `Actor` (an enum with variants `User { id }`, `Token { id }`, `System { component }`, `Bootstrap`). To construct `AuditRow`, the paginator must convert the two DB strings back to an `Actor` variant. The design specifies `AuditEvent::FromStr` for kind reconstruction (addressing R11-F1101), but specifies no equivalent for `Actor`. The `Actor` task specifies only the forward direction: variant → `(actor_kind, actor_id)`. The reverse direction — `("user", "alice")` → `Actor::User { id: "alice".to_string() }` — has no specified method (`TryFrom<(&str, &str)>`, a `from_kind_id` helper, or similar).

Concrete failure sequence: an implementer writes the startup paginator, reads `actor_kind` and `actor_id` from the DB row, and cannot construct `AuditRow { actor: Actor, … }`. They choose one of three independent workarounds: (a) add an unspecified `Actor::from_kind_id(kind: &str, id: &str) -> Result<Actor>` method; (b) change the paginator to use a different read-model struct with `actor_kind: String, actor_id: String` fields — which then cannot be passed directly to `chain::verify(rows: impl Iterator<Item = &AuditRow>)` since `AuditRow` has `actor: Actor`; (c) derive `serde::Deserialize` on `Actor` and use sqlx's `FromRow` with a custom mapping — which may or may not produce the same `actor_kind`/`actor_id` values as `canonical_json` expects.

**Consequence:** Options (b) requires a type conversion or a second `AuditRow`-like struct. If that struct serialises `actor_kind`/`actor_id` differently than `Actor`'s specified mapping (e.g., because the implementer uses a different capitalisation), `canonical_json` in `chain::verify` produces different hash values for the same row than `record` wrote at insert time — `chain::verify` reports false `ChainBroken` on every row. Because rows are immutable, the misparse cannot be corrected.

**Design assumption violated:** The design assumes `AuditRow` is the canonical type for both the write path and the startup paginator read path, without specifying how `Actor` is reconstructed from its two-column DB representation.

**Suggested mitigation:** Add to the `Actor` enum task: "The `Actor` enum MUST also implement a reverse-mapping method: `fn from_kind_id(kind: &str, id: &str) -> Result<Actor, AuditError>` (or an equivalent `TryFrom<(&str, &str)>` impl). The mapping MUST be the inverse of the specified forward mapping: `('user', id)` → `Actor::User { id }`; `('token', id)` → `Actor::Token { id }`; `('system', 'bootstrap')` → `Actor::Bootstrap`; `('system', component)` → `Actor::System { component: … }` (with the caveat that `&'static str` cannot be produced from an owned DB string — `System { component }` should use `String` for the read path, or a separate variant). The round-trip test MUST include both directions: `actor.to_kind_id()` and `Actor::from_kind_id(kind, id)` produce the same value."

Note on `System { component: &'static str }`: this lifetime constraint means a `System` variant cannot be constructed from a runtime-owned `String` read from the DB. This is a structural incompatibility that requires either (a) changing `component` to `String` (accepting a small allocation), or (b) accepting that `Actor::System` variants cannot be read back from the DB and using a lossy enum. The design should resolve this before implementation.

---

### MEDIUM — `AuditEvent::FromStr` implementation mechanism is split across two tasks with no coordination; a bespoke `match` implementation can silently diverge from `Display` on any individual variant
**Category:** Documentation trap / test coverage gap

**Trigger:** The vocabulary consolidation task says "Derive `strum::EnumCount` on `AuditEvent`" and "Add `strum = { features = ['derive'] }` to `core/Cargo.toml`." It does not say to derive `strum::EnumString`. The `chain::verify` task separately says "`AuditEvent` MUST implement `std::str::FromStr` ... using the same `Display` strings as the `Display` impl. The `FromStr` implementation MUST be tested: for every variant, `variant.to_string().parse::<AuditEvent>()` round-trips." It then says "use `strum::EnumString` derive or a manual match — either is acceptable so long as it is tested."

The "either is acceptable" provision allows a bespoke `match` arm implementation. For 44 variants, a developer writing a `match` block is likely to introduce at least one silent typo — e.g., `"auth.bootstrap-credentials-created"` in `AUDIT_KIND_VOCAB` vs. `"auth.bootstrap-credential-created"` (missing 's') in the `FromStr` match. The round-trip test `variant.to_string().parse::<AuditEvent>()` would catch this — but only if the round-trip test is implemented against `all_variants()`. If `all_variants()` lags the enum (the already-addressed concern from R9-F906), a new variant not added to `all_variants()` has its round-trip never tested.

More concretely: the `chain::verify` task says the round-trip test covers "every variant" but specifies it using `variant.to_string().parse::<AuditEvent>()` for "every variant" — the "every variant" source is not named. If it uses `AuditEvent::all_variants()` (manually maintained), a variant missing from that list is not round-trip tested. If it iterates `AuditEvent::iter()` (from `strum::IntoEnumIterator`, not specified), it would be exhaustive. The design does not specify which iteration mechanism the round-trip test uses.

**Consequence:** A typo in a bespoke `FromStr` match for a low-frequency variant (e.g., `secrets.master-key-fallback-engaged`) goes undetected if that variant is not in `all_variants()` or the test's iteration source. At startup, `.parse::<AuditEvent>()` returns `Err` for a DB row with that `kind` value. The paginator either skips the row (chain gap) or propagates the error (startup fails). Neither outcome is specified.

**Design assumption violated:** The design assumes "tested" is sufficient to guarantee correctness of a bespoke `FromStr` match, but the test's coverage is only as exhaustive as the iteration source it uses.

**Suggested mitigation:** Close the implementation ambiguity: specify that `AuditEvent` MUST derive `strum::EnumString` (not a bespoke match), using per-variant `#[strum(serialize = "...")]` attributes matching the `Display` strings. Alternatively, if the existing `Display` implementation already uses strum's display derive with identical annotations, `EnumString` can inherit the same strings. Specify this in the vocabulary consolidation task: "Also derive `strum::EnumString` on `AuditEvent`, so `FromStr` and `Display` share the same string mapping at the derive level rather than in two separate code paths." Additionally, specify that the round-trip test iterates using `<AuditEvent as strum::IntoEnumIterator>::iter()` (which requires `derive(strum::EnumIter)`) — not `all_variants()` — so the test is exhaustive by construction.

---

## No findings (categories with nothing concrete to raise)

- **Authentication & authorization** — `AuditWriter` remains server-internal with no new auth surface in round 12.
- **Abuse cases** — 10 MB cap (with byte-accurate BLOB counting for both `redacted_diff_json` and `notes`), `busy_timeout = 5000`, and max 1000 row limit close the main vectors. No new abuse path found.
- **Race conditions** — `Mutex<Option<SqliteConnection>>` + `BEGIN IMMEDIATE` serialises all writes. All concurrent-write races were closed in prior rounds. No new concurrent race found.
- **Resource exhaustion** — Paginated `chain::verify` (batch 500), 10 MB query cap, and `busy_timeout` close the main exhaustion paths.
- **State machine violations** — Migration 0006 step order (ALTER, UPDATE, trigger CREATE) is correctly sequenced. No state machine gap found.
- **Error handling gaps** — `BusyTimeout`, `ConnectionLost`, `ConnectionRecoveryFailed`, `InvalidTimestamp`, `SecretsRevealedNotYetSupported`, `TriggersMissing` all specified. No new unhandled error path found.
- **Rollbacks** — Audit writes are intentionally out-of-band from business transactions; no rollback semantics for audit rows by design.
- **Orphaned data** — Immutability triggers prevent cleanup by design; no new orphan accumulation path.
- **Migration hazards** — sqlx wraps each migration file in a transaction by default; the four steps in `0006` are atomic. No partial-apply hazard.
- **Single points of failure** — Connection recovery (`R5-F402`, `R6-F505`) and `busy_timeout` (`R8-F801`) address SPOF vectors. No new SPOF found.
- **Timeouts & retries** — `PRAGMA busy_timeout = 5000` + `BusyTimeout` error caps the wait. No retry loop introduced.
- **Eventual consistency** — Single-process SQLite; no multi-store consistency gap.
- **Assumption violations** — All prior assumption violations are addressed. The three new findings above (HIGH, HIGH, MEDIUM) represent the remaining structural gaps.

---

## Open items from prior rounds not yet closed in the design

The following R11 findings are raised but not addressed in the current design text (no "Design decisions recorded here" entry, no task update, no Addresses reference):

- **R11-F1103** — `canonical_json` spec says "actor_kind/actor_id are stored as plain strings in `AuditRow` and encoded as-is" but `AuditRow.actor` is typed `Actor` (an enum). These two statements are mutually contradictory; the encoding path for `actor_kind`/`actor_id` in `canonical_json` is unresolved. **Not closed.**
- **R11-F1104** — `canonical_json`'s handling of `Option<RedactedDiff>` is unspecified: should the field be encoded as a JSON string (matching DB TEXT storage) or as a nested JSON object (by parsing the string)? The two options produce different SHA-256 hashes. **Not closed.**

---

## Summary

**Critical:** 0  **High:** 2  **Medium:** 1  **Low:** 0

**Top concern:** The `AuditRow.prev_hash` phantom-field binding ambiguity (High finding 1) is the most dangerous: an implementer who binds all struct fields uniformly in the INSERT will silently write the caller-supplied `prev_hash` instead of the DB-computed value, producing a permanently broken chain from the first write. The immutability triggers guarantee this cannot be corrected without dropping the chain entirely.

**Recommended action before proceeding:** Address the two open R11 items (F1103 and F1104) and the three new findings before implementation begins. The High findings (prev_hash bind ambiguity and Actor deserialization) directly threaten chain correctness; the Medium finding (FromStr implementation mechanism) threatens correctness of the startup paginator read path. All five create conditions where `canonical_json` produces different hashes at write time vs. verify time on an immutable log.
