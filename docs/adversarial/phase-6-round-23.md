# Adversarial Review — Phase 6 — Round 23

**Design summary:** Phase 6 adds a tamper-evident, append-only audit log to Trilithon: an `AuditWriter` with a dedicated serialised `tokio::sync::Mutex<Option<SqliteConnection>>`, a SHA-256 `prev_hash` chain verified via a stateful batch API (`ChainVerifyState` / `verify_batch` / `verify_finish`), a `RedactedDiff` newtype for secrets-safe diffs, an `Actor` enum with `from_kind_id` reverse-mapping and unknown-kind fallback, startup integrity checks, and a query API with a 10 MB soft cap.

**Prior rounds:** Rounds 1–22 reviewed. All R22 findings are assessed below.

- R22-M1 (`sentinel_count` increment unverifiable — no test asserted `state.sentinel_count == N`): CLOSED — test criterion (c) now reads "this test MUST also assert `state.sentinel_count == N` where N is the exact count of ZERO_SENTINEL rows in the input slice; this assertion ensures the struct field is actually incremented and not a local variable."
- R22-L1 (test (j) "does not modify state" unverifiable as written): CLOSED — test (j) is now operationally specified: "call `verify_batch` on a 3-row chain (setting `state.last_computed_hash` to a known value), then call `verify_batch` with an empty slice, then assert that `state.last_computed_hash` still equals the value set after the 3-row call."
- R22-L2 (`RedactedDiff` sqlx reconstruction mechanism unspecified — implementers face unguided compile error): CLOSED — both `record` step 5 and the startup paginator now mandate: "Use an intermediate `struct PredecessorRow { redacted_diff_json: Option<String>, /* … all other columns as primitive types … */ }` when running the sqlx query; then convert to `AuditRow` by calling `RedactedDiff::from_db_str` on the non-null value. Do NOT add `impl sqlx::Type<Sqlite> for RedactedDiff` or `impl From<String> for RedactedDiff`."

---

## Findings

### HIGH — `#[strum(disabled)]` does NOT exclude a variant from `EnumCount`; the compile-time assertion `COUNT == AUDIT_KIND_VOCAB.len()` will fail to compile with 44 named variants + 1 `Unknown` = 45 vs 44

**Category:** Logic flaw

**Trigger:** The vocabulary task (line 14) and chain-verify task (line 45) both state that `AuditEvent::Unknown(String)` is annotated with `#[strum(disabled)]` and "does NOT appear in `EnumCount`, `AUDIT_KIND_VOCAB`, or `AuditEvent::iter()`." The compile-time assertion is:

```rust
const _: () = assert!(<AuditEvent as strum::EnumCount>::COUNT == AUDIT_KIND_VOCAB.len());
```

The design assumes `Unknown` is excluded from `EnumCount` because it has `#[strum(disabled)]`. This assumption is incorrect. In strum, `#[strum(disabled)]` controls two things only: (1) the variant is excluded from `EnumIter` — it will not appear in `AuditEvent::iter()`; (2) the variant cannot be parsed by `EnumString` — `"some.str".parse::<AuditEvent>()` will not produce `Unknown(...)` via EnumString (which is correct — the design uses `.unwrap_or_else(|_| AuditEvent::Unknown(s))` instead). `#[strum(disabled)]` does NOT affect `EnumCount`. `EnumCount::COUNT` is derived by counting all variants in the enum declaration, including disabled ones. If `AuditEvent` has 44 named variants plus `Unknown`, `COUNT` will be 45.

Concrete failure sequence: implementer adds 44 named variants to `AuditEvent` and adds `#[strum(disabled)] Unknown(String)`. `AUDIT_KIND_VOCAB` has 44 entries. The compile-time assertion evaluates `45 == 44` and fails with a compile error. The project does not compile. The implementer must either (a) remove `Unknown` from the enum (which breaks the fallback parse path), (b) remove the compile-time assertion (which was added to catch vocabulary desync), or (c) add a 45th entry to `AUDIT_KIND_VOCAB` (which is semantically wrong — `Unknown` is not a fixed vocabulary entry). All three workarounds violate the design's intent.

Alternatively, if the implementer encounters the compile failure and adds a 45th `AUDIT_KIND_VOCAB` entry as a placeholder, the assertion passes but now `AUDIT_KIND_VOCAB` contains a non-meaningful entry. Any runtime code that iterates `AUDIT_KIND_VOCAB` (e.g., future `record` vocabulary validation) would include the fake entry.

**Consequence:** The compile-time assertion — the design's central compile-time safety net for vocabulary synchrony — will fail to compile as specified. The project cannot be built without working around the assertion, and every workaround either removes safety or corrupts `AUDIT_KIND_VOCAB`. The design's claim that "deleting any `AUDIT_KIND_VOCAB` entry is a compile error" holds only for the 44 named variants; the assertion itself breaks at compile time before any deletion.

**Design assumption violated:** The design assumes `#[strum(disabled)]` excludes a variant from `EnumCount`. This is not how strum works: `EnumCount` counts all syntactic variants, disabled or not.

**Suggested mitigation:** One of two fixes: (A) Move `Unknown` out of the enum and represent unknown kinds as `String` stored alongside the enum (e.g., `AuditEvent::Parsed(KnownEvent)` + a separate fallback field on `AuditRow`) — structurally cleaner but requires re-specifying the kind-storage type. (B) Keep `Unknown` in the enum but change the compile-time assertion to exclude it explicitly: instead of comparing `COUNT` to `AUDIT_KIND_VOCAB.len()`, derive a separate constant `KNOWN_AUDIT_EVENT_COUNT = COUNT - 1` and assert that equals `AUDIT_KIND_VOCAB.len()`. Specifically: `const _: () = assert!(<AuditEvent as strum::EnumCount>::COUNT - 1 == AUDIT_KIND_VOCAB.len());`. Option B requires documenting that the `-1` accounts for `Unknown`. Either fix must be made explicit in the design before implementation begins.

---

### MEDIUM — `PredecessorRow` field set is underspecified — "all other columns as primitive types" is ambiguous for typed `AuditRow` fields; implementers will produce different (and potentially incompatible) struct shapes

**Category:** Logic flaw

**Trigger:** Both `record` step 5 (line 64) and the startup paginator task (line 82) now mandate using an intermediate `struct PredecessorRow { redacted_diff_json: Option<String>, /* … all other columns as primitive types … */ }`. The comment `/* … all other columns as primitive types … */` is a placeholder, not a field list.

An implementer building `PredecessorRow` will look at `AuditRow` and attempt to convert typed fields to "primitive DB types." But `AuditRow` contains:

- `id: Ulid` — must become `id: String` in `PredecessorRow` (26-char TEXT)
- `actor: Actor` — the `Actor` enum maps to two columns: `actor_kind: String` + `actor_id: String`; there is no single `actor` column in `audit_log`; `PredecessorRow` must have two separate fields
- `kind: AuditEvent` — must become `kind: String` (TEXT column)
- `outcome: AuditOutcome` — must become `outcome: String` (TEXT column)
- `occurred_at_ms: i64` — stays as `i64` (INTEGER column), but `occurred_at: i64` must also be excluded (as discussed in R16-M2 / R19-M1); an implementer who includes `occurred_at: i64` in `PredecessorRow` will break the sqlx named-projection exclusion requirement
- `redaction_sites: i64` — stays as `i64`
- `snapshot_id: Option<String>` — stays as `Option<String>`
- etc.

The `actor` split is the most dangerous gap: `AuditRow` has `actor: Actor` (one field mapping to two columns). An implementer who naively maps `AuditRow` minus `redacted_diff` to `PredecessorRow` would write `actor: Actor` in `PredecessorRow`. `Actor` does not implement `sqlx::Type<Sqlite>` (it maps to two columns, not one). The sqlx `FromRow` derive will fail to compile or produce a runtime decode error.

Concrete failure sequences:
1. Implementer writes `struct PredecessorRow { id: Ulid, actor: Actor, kind: AuditEvent, outcome: AuditOutcome, … }`. Compile error from sqlx: `Actor` does not implement `sqlx::Type<Sqlite>`. Implementer who doesn't understand the two-column mapping attempts to add `impl sqlx::Type<Sqlite> for Actor` — violating the spirit of the type system, since `Actor` is a composite.
2. Implementer correctly splits `actor` into `actor_kind: String` + `actor_id: String` but writes `id: Ulid` (copying from `AuditRow`). `Ulid` does not implement `sqlx::Type<Sqlite>` by default. Compile error or runtime decode error.
3. Two implementers produce `PredecessorRow` with different field types for `kind` (`String` vs. the `AuditEvent` enum) and different conversion logic. Both pass compilation, but the one using `AuditEvent` must have added `sqlx::Type<Sqlite>` for it — which the design prohibits for `RedactedDiff` but does not address for `AuditEvent`, `Actor`, or `AuditOutcome`.

Since `PredecessorRow` is used in two places (step 5 and the startup paginator) and the struct is local to the adapters crate, divergent field definitions in two separate file locations (e.g., one inline in `sqlite_storage.rs::record` and another in `sqlite_storage.rs::startup_verify`) compile independently but convert to `AuditRow` via different paths, producing structurally different intermediate states for `canonical_json`. If the field types differ, the conversion to `AuditRow` may produce different `Actor`/`kind`/`outcome` values, causing the startup paginator to compute a different `canonical_json` than `record` computed at write time — a permanent hash divergence in the immutable log.

**Consequence:** Implementers without explicit field-type guidance will make different type choices for the four non-trivially-mapped fields (`id`, `actor`, `kind`, `outcome`). Any implementation that reconstructs `Actor` differently in step 5 vs. the startup paginator produces a different `canonical_json` output for the same row, making `verify_batch` report a spurious `ChainBroken` on startup — a false tamper alarm that blocks operator trust in the audit log.

**Design assumption violated:** The design assumes "all other columns as primitive types" is self-evident to implementers reading `AuditRow`. It is not: four fields in `AuditRow` have non-trivially mapped DB representations (`id: Ulid` → `String`, `actor: Actor` → two `String` fields, `kind: AuditEvent` → `String`, `outcome: AuditOutcome` → `String`), and `occurred_at` must be actively excluded despite being a real DB column.

**Suggested mitigation:** Replace the `/* … all other columns as primitive types … */` placeholder with the explicit field list in both `record` step 5 and the startup paginator task:

```rust
struct PredecessorRow {
    id: String,
    caddy_instance_id: String,
    correlation_id: String,
    occurred_at_ms: i64,
    actor_kind: String,
    actor_id: String,
    kind: String,
    target_kind: Option<String>,
    target_id: Option<String>,
    snapshot_id: Option<String>,
    redacted_diff_json: Option<String>,
    redaction_sites: i64,
    outcome: String,
    error_kind: Option<String>,
    notes: Option<String>,
    prev_hash: String,
    // occurred_at is intentionally absent — derived from occurred_at_ms / 1000
}
```

This eliminates all ambiguity about field types and the `occurred_at` exclusion, and ensures both call sites produce identical `PredecessorRow` shapes.

---

## No findings (categories with nothing concrete to raise)

- **Authentication & authorization** — `AuditWriter` is server-internal; all public surface is `record`. No new bypass vector.
- **Abuse cases** — 10 MB query cap (BLOB-accurate, COALESCE-wrapped, both columns), max 1000 rows, `AUDIT_QUERY_ROW_FIXED_OVERHEAD_BYTES` constant, `busy_timeout = 5000`, `occurred_at_ms > 0` guard before mutex lock. No new abuse vector.
- **Data exposure** — `RedactedDiff` newtype with controlled constructors, `from_db_str` is `pub` with doc comment and companion grep recipe. No new exposure vector.
- **Race conditions** — `tokio::sync::Mutex` + `BEGIN IMMEDIATE` serialises all writes. Concurrent chain test specified. No new race vector.
- **State manipulation** — ZERO_SENTINEL / `""` / computed-hash three-way per-row dispatch fully specified. `SecretsRevealed` and `InvalidTimestamp` guards are pre-mutex. No new vector.
- **Resource exhaustion** — 500-row batch API bounds memory; no full-log preload; busy_timeout; 10 MB query cap. No new exhaustion vector.
- **Single points of failure** — Connection recovery (close + reopen), `ConnectionRecoveryFailed` surfacing both errors, `PRAGMA foreign_keys = ON` on recovery opens. No new SPOF.
- **Timeouts & retries** — `busy_timeout = 5000` + `BusyTimeout` return; test (h) verifies ~6 s bound. No retry amplification.
- **Eventual consistency** — Single-process SQLite; no multi-store gap.
- **Rollbacks** — Audit writes are out-of-band from business transactions by design; immutability by DB trigger. No rollback semantics for audit rows.
- **Rate limits** — `busy_timeout` + bounded query page sizes cover the query path. No new gap.
- **Orphaned data** — Immutability triggers prevent cleanup by design. No accumulation path during normal operation.
- **R22 closure verification** — R22-M1, R22-L1, and R22-L2 are all genuinely closed in the updated design text. Test (c) now asserts `state.sentinel_count == N`; test (j) is operationally specified with an explicit `state.last_computed_hash` assertion; step 5 and the startup paginator now mandate `PredecessorRow` with prohibitions on `sqlx::Type<Sqlite>` and `From<String>` impls.
- **`verify_batch`/`verify_finish` coverage for empty log** — Tests (k) and (l) are genuine non-vacuous checks. Both require `Ok(())`. Any implementation that adds an error condition to `verify_finish` fails both. Closed.
- **Test (j) empty-slice state preservation** — The test as amended (call `verify_batch` on 3 rows, then call with empty slice, assert `state.last_computed_hash` unchanged) directly detects the buggy `state.last_computed_hash = None` reset implementation. Closed.
- **`verify_batch` called with empty slice test (l) vs. test (j) distinction** — Test (l) covers zero rows total then `verify_finish`; test (j) covers non-zero prior state then empty-slice call. The two tests are complementary and together close the empty-slice surface.

---

## Summary

**Critical:** 0  **High:** 1  **Medium:** 1  **Low:** 0

**Top concern:** R23-H1 — The compile-time assertion `<AuditEvent as strum::EnumCount>::COUNT == AUDIT_KIND_VOCAB.len()` will fail to compile as written: `#[strum(disabled)]` does not exclude `Unknown` from `EnumCount`, so COUNT = 45 while AUDIT_KIND_VOCAB.len() = 44. The assertion fires at compile time, not at runtime, so the entire project is unbuildable until the assertion is corrected. Every workaround either removes the compile-time safety net or corrupts AUDIT_KIND_VOCAB.

**Recommended action before proceeding:** Not yet ready — 2 blocker(s) remain (R23-H1, R23-M1).
