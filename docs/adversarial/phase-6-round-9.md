# Adversarial Review — Phase 6 — Round 9

## Summary

0 critical · 2 high · 2 medium · 2 low

## Round 8 Closure

| ID | Status | Notes |
|----|--------|-------|
| F801 | Closed | `PRAGMA busy_timeout = 5000` mandated; `AuditError::BusyTimeout` returned when `BEGIN IMMEDIATE` exceeds timeout; test (h) added |
| F802 | Closed | `RedactedDiff::as_str()` public read accessor specified; `record` binds via `as_str()` as raw TEXT, not via `Json(…)` |
| F803 | Closed | Error log for empty `correlation_id` updated to include `correlation_id = %synth_id` structured field |
| F804 | Closed | Test (f) respecified to use file-based SQLite and file deletion to trigger the error-recovery branch |

---

## New Findings

### F901 — HIGH — `canonical_json(row)` for `prev_hash` is never defined; two implementations will diverge, producing a chain that verifies locally but breaks on any other build

**Category:** Documentation trap / schema mismatch

**Attack:** The `prev_hash` chain design requires `sha256(canonical_json(row))` at two call sites: (1) `AuditWriter::record` computes `sha256(canonical_json(predecessor_row))` and writes it as the new row's `prev_hash`; (2) `chain::verify` recomputes `sha256(canonical_json(row))` for each row to cross-check. The design names `canonical_json` but never specifies what it produces. Key questions with multiple defensible answers:

- Does `canonical_json` include the `prev_hash` field of the row being hashed, or exclude it? If included, the predecessor row's `prev_hash` is part of the hash input — which is correct and produces a proper chain. If excluded, an attacker who knows the hash function can swap entire rows while preserving the prev_hash link.
- Does `canonical_json` include all columns, or only a subset? Including `id`, `occurred_at_ms`, `actor_kind`, `actor_id`, `kind`, `outcome`, `redacted_diff_json`, `redaction_sites`, `correlation_id`, `caddy_instance_id`, `target_kind`, `target_id`, `snapshot_id`, `error_kind`, `notes` is the tamper-resistant choice. Excluding any mutable-looking column (e.g., `notes`) weakens the guarantee.
- Does `canonical_json` sort object keys? The ADR says "sort object keys lexicographically" but this is stated for snapshot identifiers — not explicitly for audit row hashing.
- How are NULLs represented? JSON null vs. key omission changes the byte representation.

An implementer of `record` and a separate implementer of `chain::verify` will each make independent choices about field inclusion and NULL representation. The chain will verify in unit tests (same implementation) but will silently produce divergent hashes the moment anyone adds a second codepath that recomputes the hash.

**Why the design doesn't prevent it:** The `chain::verify` task and the `AuditWriter::record` task both reference `canonical_json(row)` but neither defines it. The design records the canonical JSON format for `DesiredState` snapshots in ADR-0009 but does not carry that definition forward to the audit row hash.

**Mitigation required:** Add to the `chain::verify` task: "The `canonical_json(row: &AuditRow)` function MUST be defined in `crates/core/src/audit.rs` as a named, tested function — not inlined at each call site. It MUST: (a) include every column of `audit_log` in the JSON object, including `prev_hash`; (b) represent NULL values as JSON null (not omit them); (c) sort object keys lexicographically; (d) produce no whitespace between tokens. The acceptance criteria MUST include a test asserting that `canonical_json` produces a stable byte-for-byte output for a fixed `AuditRow` value." This ensures the hash is deterministic across both call sites and resistant to field-omission attacks.

---

### F902 — HIGH — `AuditRow.occurred_at_ms` is an `i64`; `record` derives `occurred_at` as `occurred_at_ms / 1000` at bind time; truncation toward zero produces a value in the future for negative timestamps

**Category:** Logic flaw / schema mismatch

**Attack:** The design specifies `occurred_at_ms: i64` on `AuditRow` and instructs `record` to bind `occurred_at = occurred_at_ms / 1000` at bind time. For any timestamp before the Unix epoch (negative `occurred_at_ms` values), integer truncation toward zero in Rust produces a value that is 1 second _larger_ (less negative, i.e., closer to zero, i.e., after the actual time). Concretely: `occurred_at_ms = -500` (half a second before epoch) → `occurred_at = 0` (exactly at epoch). This is a 500 ms error. For `occurred_at_ms = -1500` → `occurred_at = -1` (correct), but `occurred_at_ms = -999` → `occurred_at = 0` (1 second wrong).

This matters because the startup consistency check queries `SELECT COUNT(*) FROM audit_log WHERE occurred_at != occurred_at_ms / 1000 LIMIT 1`. SQLite integer division also truncates toward zero. If both Rust and SQLite apply the same truncation, the check passes even for negative timestamps — silently accepting the inconsistency as consistent. However, a future cross-check tool (e.g., written in Python, where `-999 // 1000 = -1`) will produce a different value and flag these rows as inconsistent.

For practical Trilithon deployments (self-hosted, post-2001), negative timestamps should never occur. But the startup check will silently pass on systems with incorrect system clocks set before 1970, and any audit row written with a negative `occurred_at_ms` will have an `occurred_at` value that is wrong by up to 1 second.

**Why the design doesn't prevent it:** The design specifies `i64` for `occurred_at_ms` without noting that negative values are invalid, and does not add a write-time guard that rejects negative timestamps. The startup consistency check uses the same truncation semantics as the write, masking the error.

**Mitigation required:** Add to the `AuditRow` task: "`occurred_at_ms` MUST be a positive `i64`. `AuditWriter::record` MUST validate `row.occurred_at_ms > 0` before binding and return `Err(AuditError::InvalidTimestamp { occurred_at_ms: row.occurred_at_ms })` if the value is zero or negative. This prevents clock-at-epoch anomalies and ensures the `occurred_at` derived value is always correct under both Rust and SQLite integer division." This is a one-line guard that closes the silent-corruption path without changing the storage format.

---

### F903 — MEDIUM — The `chain::verify` startup call is paginated in batches of 500 `ORDER BY rowid ASC`, but `record` inserts new rows concurrently with the startup scan; the chain seen by `verify` may not match the chain `record` is building

**Category:** Race condition

**Attack:** The startup sequence is: apply migrations → call `chain::verify` → accept writes. The design says the chain check "logs the result" and "daemon starts" even if broken. The design does not say `chain::verify` runs before `AuditWriter` is made available to the rest of the daemon.

If the startup sequence is concurrent (e.g., the daemon begins serving requests before startup verification finishes, or `chain::verify` runs as a background task), the following race is possible:

1. `chain::verify` reads batch 1 (rows 1–500), accumulates `last_computed_hash = H500`.
2. A write path begins, calls `AuditWriter::record`, reads the last row's hash (also `H500`), inserts row 501 with `prev_hash = H500`.
3. `chain::verify` reads batch 2 (rows 501–1000), row 501's `prev_hash = H500` matches accumulated hash — OK.

This specific case is safe. But consider:

1. `chain::verify` reads batch 1 (rows 1–500), pauses between batches (Tokio yield point).
2. A write inserts row 501 with `prev_hash = H500`.
3. Another write inserts row 502 with `prev_hash = H501` (where `H501 = sha256(canonical_json(row501))`).
4. `chain::verify` continues reading batch 2, sees rows 501 and 502 — both correct.

Still safe. Now the chain-breaking case:

1. `chain::verify` begins, logs a broken link at row 200 (`Err(ChainBroken)`), logs error, daemon starts.
2. A new write via `AuditWriter::record` continues inserting from the current chain tip — building a valid chain from row N onward, even though rows 1–199 are broken.
3. On next startup, `chain::verify` still reports the same break at row 200 — permanently — and the daemon continues starting.

The race here is that the design specifies: broken chain → log error → daemon starts — but never specifies that a broken chain is investigated or quarantined before new rows are written. The new rows that are written will themselves be verifiable, but the broken section is permanent and immutable. This is not a new finding in itself, but the design does not specify what `chain::verify` does with the `last_computed_hash` after the break: does it resume tracking from the broken row (building a new sub-chain) or does it stop? If it resumes, verification reports one break but continues — the second half of the chain verifies correctly, masking whether the break was a single tampered row or a complete log replacement.

**Why the design doesn't prevent it:** The `chain::verify` task specifies the two error variants but does not specify what happens to `last_computed_hash` when `ChainBroken` is returned — whether the function stops immediately on the first break or continues scanning. If the function continues after the first break and resets `last_computed_hash` to the current (tampered) row, it will report `Ok(())` for the remainder of the chain even though the chain's integrity from the break point forward is unverifiable relative to the original pre-break rows.

**Mitigation required:** Clarify in the `chain::verify` task: "`chain::verify` MUST return `Err(ChainBroken { … })` on the first broken link and MUST NOT continue scanning past the break. Continuing past a break and resetting `last_computed_hash` to the tampered row's hash would allow a complete log replacement (keeping internal consistency) to pass as 'one break then OK.' Early return on first error is the correct behavior." This is a one-sentence clarification that makes the semantics unambiguous.

---

### F904 — MEDIUM — The non-empty assertion `const _: () = assert!(!PHASE6_REGISTRY.0.is_empty())` is in `core`, but the compile-time assertion for `AuditEvent::COUNT == AUDIT_KIND_VOCAB.len()` is also in `core` with UFCS — the design does not specify where `PHASE6_REGISTRY` lives relative to `AUDIT_KIND_VOCAB`; an implementer may place it in `adapters`, making the compile-time assertion unreachable from production code

**Category:** Documentation trap

**Attack:** The design specifies two compile-time assertions:
1. `const _: () = assert!(<AuditEvent as strum::EnumCount>::COUNT == AUDIT_KIND_VOCAB.len())` — in `crates/core/src/audit.rs`, production code.
2. `const _: () = assert!(!PHASE6_REGISTRY.0.is_empty())` — location unspecified.

The `SecretFieldRegistry` and `PHASE6_REGISTRY` task says "`SecretFieldRegistry` MUST be `pub struct SecretFieldRegistry(&'static [&'static str])`" and "The registry contains JSON Pointer paths that exist in actual `DesiredState` diffs." `DesiredState` is a `core` type. If `PHASE6_REGISTRY` is defined in `adapters` (which might seem natural since `AuditWriter` is in `adapters`), the compile-time assertion fires in `adapters`, not `core`. That is acceptable — `adapters` is also production code.

But if an implementer reads "the registry contains paths in actual `DesiredState` diffs" and places `PHASE6_REGISTRY` in `core/src/audit.rs`, while placing `AuditWriter` in `adapters`, the assertion fires in `core`. The `RedactedDiff::new` function signature is `(raw: &serde_json::Value, registry: &SecretFieldRegistry) -> (RedactedDiff, usize)` — also in `core`. All of this is self-consistent.

The actual trap is: the design says `AuditWriter::record` returns `Err(AuditError::SecretsRevealedNotYetSupported)` when `row.kind == AuditEvent::SecretsRevealed`. This guard lives in `adapters`. If an implementer adds a second check in `core` (e.g., in `RedactedDiff::new`), the guard is duplicated. If the guard is only in `adapters`, an implementer who calls `RedactedDiff::new` directly from a test with a `SecretsRevealed` row will not hit the guard. The compile-time assertion `!PHASE6_REGISTRY.0.is_empty()` asserts the registry has a placeholder entry, but does not prevent `RedactedDiff::new` from being called with real secret data in a test context. The test would exercise the real redaction path against the placeholder registry, silently missing any actual secret paths.

This is a minor design ambiguity, not a production failure — but it creates a false sense of security in tests.

**Why the design doesn't prevent it:** The design places the `SecretsRevealedNotYetSupported` guard only in `AuditWriter::record`, not at the `RedactedDiff::new` level. A test that constructs a `SecretsRevealed` row and calls `RedactedDiff::new` directly bypasses the guard.

**Mitigation required:** Add a clarification to the `SecretFieldRegistry` task: "`PHASE6_REGISTRY` MUST be defined in `crates/core/src/audit.rs` alongside `SecretFieldRegistry`. The `SecretsRevealedNotYetSupported` guard is in `AuditWriter::record` only (not in `RedactedDiff::new`); test code that calls `RedactedDiff::new` directly is not subject to the guard and MUST use non-`SecretsRevealed` event rows or the placeholder path." This prevents the false-security interpretation without changing the design.

---

### F905 — LOW — `synth:<ulid>` fallback correlation IDs are indistinguishable from each other in the `correlation_id` index; queries for `synth:` rows require a LIKE scan, not the index

**Category:** Observability gap

**Attack:** The design specifies `synth:<ulid>` as the fallback `correlation_id` format when no active span exists or when the `correlation_id` is empty. `audit_log` has `CREATE INDEX audit_log_correlation_id ON audit_log(correlation_id)`. A query `WHERE correlation_id = 'synth:01J...'` uses the index efficiently. However, to find all "synthetic" correlation IDs (e.g., in a diagnostic report or anomaly scan), the query is `WHERE correlation_id LIKE 'synth:%'` — a prefix scan that cannot use a B-tree index efficiently in SQLite (SQLite does support index-assisted LIKE prefix scans when `PRAGMA case_sensitive_like = ON` or when the column has `TEXT` affinity and the pattern has no leading wildcard, but this is fragile and implementation-dependent).

This is not an outage risk — `synth:` IDs are fallback paths that should occur rarely. But an operator or diagnostic query that counts or lists all synthetic IDs will perform a full-table scan on a potentially large audit log. On a system that has been running for years with frequent no-span paths (e.g., bootstrap events on every startup), this scan could be slow.

**Why the design doesn't prevent it:** The design specifies the `synth:` prefix format without noting that aggregate queries on synthetic IDs will not use the correlation_id index.

**Mitigation required:** Add a note to the documentation task: "The `synth:` prefix enables prefix-scan identification of fallback correlation IDs via `WHERE correlation_id LIKE 'synth:%'`. SQLite supports index-assisted prefix scans for LIKE when the pattern has no leading wildcard, but operators should be aware that counting synthetic IDs on large tables may be slow. If the frequency of synthetic IDs is a concern, a separate `is_synthetic_correlation` boolean column can be added in a future migration." No code change required in Phase 6 — this is a documentation note.

---

### F906 — LOW — The `all_variants()` helper in `audit.rs` must be updated to 44 entries as part of Phase 6; the design acceptance criteria say "Update `all_variants()` to 44 entries" but no test asserts that `all_variants()` is exhaustive

**Category:** Test coverage gap

**Attack:** The existing `variant_count_matches_expected` test asserts `all_variants().len() == AUDIT_EVENT_VARIANT_COUNT`. After Phase 6 adds three variants and derives `strum::EnumCount`, the test changes to `<AuditEvent as strum::EnumCount>::COUNT == AUDIT_KIND_VOCAB.len()`. But the `all_variants()` helper is used in two tests: `display_strings_match_six_six_vocab` and `no_two_variants_share_a_kind`. If an implementer adds the three new variants to the `AuditEvent` enum and to `AUDIT_KIND_VOCAB` but forgets to add them to `all_variants()`, both tests pass — `all_variants()` returns 41 items (or 44 minus the forgotten ones), `len()` is used only in `variant_count_matches_expected` which is now checking against `strum::EnumCount::COUNT`. The `display_strings_match_six_six_vocab` test checks that every variant in `all_variants()` maps to a known string — it does not check the converse (that every `AUDIT_KIND_VOCAB` entry has a corresponding variant). A variant added to the enum but not to `all_variants()` would have its display string never checked.

After the transition to `strum::EnumCount`, the compile-time assertion `<AuditEvent as strum::EnumCount>::COUNT == AUDIT_KIND_VOCAB.len()` ensures the vocab list stays in sync with the enum. But the `all_variants()` helper remains a manually maintained list that can silently lag behind.

**Why the design doesn't prevent it:** The acceptance criterion says "Update `all_variants()` to 44 entries" as an imperative but does not add a test that asserts `all_variants().len() == <AuditEvent as strum::EnumCount>::COUNT`, which would catch the case where someone adds a variant to the enum but not to the helper.

**Mitigation required:** Add to the vocabulary consolidation task's "Done when" criteria: "The `variant_count_matches_expected` test MUST be updated to assert `all_variants().len() == <AuditEvent as strum::EnumCount>::COUNT` (replacing the hardcoded `AUDIT_EVENT_VARIANT_COUNT` constant which is deleted). This ensures `all_variants()` stays exhaustive even as the enum grows." This is a one-line test change that closes the silent-lagging-helper path.

---

## No findings (categories with nothing concrete to raise)

- **Authentication & authorization** — `AuditWriter` is server-internal; no new auth surface introduced.
- **Abuse cases** — Rate limiting and 10 MB cap are already specified (F406/F504 closed). No new abuse vector found.
- **Race conditions** — F903 (chain::verify early-return on break) noted above. The `Mutex<Option<SqliteConnection>>` + `BEGIN IMMEDIATE` design from R3-F201/R5-F402 closes the concurrent-write race; no new concurrent race found beyond the verify-semantics clarification.
- **Resource exhaustion** — 10 MB byte-accurate cap (F504 closed), `busy_timeout` (F801 closed), paginated chain verify (R2-F108 closed) address the main vectors.
- **State machine violations** — Migration order (ALTER then UPDATE then triggers) is correctly sequenced.
- **Migration hazards** — SQLite DDL is transactional; partial migration rolls back cleanly. The migration steps are well-ordered.
- **Rollbacks** — Audit writes are intentionally out-of-band; no rollback semantics for audit rows by design.
- **Orphaned data** — Immutability triggers prevent cleanup by design; no new orphan path found.
- **Eventual consistency** — Single-process SQLite; no multi-store consistency gap.
- **Single points of failure** — Connection recovery (R5-F402, R6-F505) and busy timeout (F801) address the main SPOF vectors.
