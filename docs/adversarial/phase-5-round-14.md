# Adversarial Review — Phase 05 — Round 14

**Design summary:** A content-addressed, append-only SQLite snapshot store for a Caddy reverse-proxy configuration daemon, implemented in a three-layer Rust workspace. Writes use `BEGIN IMMEDIATE` OCC with SHA-256 content addressing, monotonic `config_version` per instance, and a multi-step deduplication protocol guarded by database-level immutability triggers.

**Prior rounds:** 13 prior rounds reviewed — all previously identified issues are marked as addressed. No prior findings are re-raised below.

---

## Findings

### [MEDIUM] `InRangeCursor` includes `created_at_ms` but `in_range`'s ORDER BY does not — cursor cannot be used as a keyset seek key for `in_range` in Phase 7

**Category:** Logic Flaws

**Trigger:** Phase 7 implements keyset pagination for `in_range`. The implementer reads the pagination caveat in `SnapshotFetcher`'s doc comment (slice 5.6), which states: "The cursor for both `children_of` and `in_range` is the four-column tuple `(created_at_ms, created_at_monotonic_nanos, daemon_run_id, config_version)` matching the ORDER BY used by both queries." The implementer constructs the Phase 7 keyset predicate as `WHERE (created_at_ms, created_at_monotonic_nanos, daemon_run_id, config_version) > (cursor.ms, cursor.nanos, cursor.run, cursor.version)`.

But `in_range`'s ORDER BY is `(created_at_monotonic_nanos ASC, daemon_run_id ASC, config_version ASC)` — three columns, no `created_at_ms`. Only `children_of`'s ORDER BY includes `created_at_ms` as the primary sort key. The doc comment's claim that the four-column tuple "matches the ORDER BY used by **both** queries" is false for `in_range`.

**Consequence:** Two concrete failure paths:

- **Path A (use the documented four-column predicate):** The Phase 7 keyset predicate includes `created_at_ms` in its tuple comparison, but `in_range`'s ORDER BY does not sort by `created_at_ms`. A row whose `created_at_ms` is in the range `[from_ms, to_ms]` but whose monotonic nanos fall before the cursor's nanos would be excluded if it sorts lexicographically after the cursor tuple due to `created_at_ms` — producing skipped rows. This is a silent correctness failure for history replay consumers.

- **Path B (discover the mismatch and fix the ORDER BY):** The implementer notices the inconsistency and adds `created_at_ms` to `in_range`'s ORDER BY to match the cursor, changing `in_range`'s sort semantics mid-design. This breaks the design's stated rationale that "`created_at_ms` MUST NOT be used as the primary sort key" because NTP corrections produce non-monotonic values.

The `InRangeCursor` struct is defined with `created_at_ms` as a field. Callers who receive it in Phase 5 and store it for Phase 7 use will hold a cursor that is either superfluous (if `created_at_ms` is dropped from the seek) or wrong (if used). Every Phase 5 consumer of `InRangeCursor` that persists cursor state will need to be audited when Phase 7 lands.

**Design assumption violated:** The design assumes the four-column cursor tuple `(created_at_ms, created_at_monotonic_nanos, daemon_run_id, config_version)` can serve as the Phase 7 keyset predicate for both `children_of` and `in_range` because both use "the same ORDER BY." They do not use the same ORDER BY: `children_of` sorts by four columns (leading with `created_at_ms`); `in_range` sorts by three (leading with `created_at_monotonic_nanos`). The cursor struct was designed for `children_of`'s four-column ORDER BY but extended to `in_range` without adjusting for the different sort key.

**Suggested mitigation:** Either (a) define two distinct cursor types — `InRangeCursor` with fields `(created_at_monotonic_nanos, daemon_run_id, config_version)` (three fields, matching the ORDER BY) and a separate `ChildrenCursor` with fields `(created_at_ms, created_at_monotonic_nanos, daemon_run_id, config_version)` (four fields); or (b) correct `in_range`'s ORDER BY to match its cursor by prepending `created_at_ms ASC` before `created_at_monotonic_nanos ASC`, and document why `created_at_ms` is the coarse-range filter key AND the leading sort key (accepting that NTP drift only affects ordering within the same range window, not across windows). Option (a) preserves the existing rationale for `in_range`'s three-column sort; option (b) aligns cursor and ORDER BY at the cost of re-examining the NTP-drift sort argument. Either way, correct the pagination caveat to stop claiming both queries share the same four-column ORDER BY.

---

### [MEDIUM] `regen-snapshot-hashes` conflates two distinct operational modes in one binary with an underspecified CLI interface — version-bump gate may block legitimate live-DB verification

**Category:** Logic Flaws

**Trigger:** `regen-snapshot-hashes` is described as performing two distinct operations:

1. **Live-DB verification:** reads rows from a `snapshots` table, re-hashes `desired_state_json`, and compares against stored `id` values.
2. **Fixture regeneration:** rewrites `desired_state_hashes::FIXTURES` (a Rust source file) with freshly computed hashes for known `DesiredState` test fixtures.

The design documents a version-bump enforcement check: "running without flags refuses to regenerate if `CANONICAL_JSON_VERSION` has not been incremented." The phrase "running without flags" is ambiguous — it could mean (a) only the fixture-regeneration sub-command is guarded, or (b) every invocation of the binary is guarded. If (b), then a production operator running `regen-snapshot-hashes --database /path/to/db` to verify a live database on a deployment where `CANONICAL_JSON_VERSION` was not recently bumped is refused or receives a confusing error. The `--skip-version-bump-check` flag only makes sense in context of fixture regeneration — it should not apply to live-DB mode at all — but the design does not specify a subcommand or mode flag that separates the two.

**Consequence:** An implementer building the CLI must invent the mode-separation mechanism themselves. Two plausible implementations:

- **Wrong implementation:** The binary checks the version-bump guard on startup regardless of mode. Operators running live-DB verification in a stable deployment (no recent version bump) receive a refusal or must pass `--skip-version-bump-check` on every run, making the guard permanently bypassed in operational scripts.

- **Correct implementation:** The binary has distinct subcommands (`regen verify --database ...` vs `regen regenerate-fixtures`), with the version-bump guard only on `regenerate-fixtures`. But neither subcommand names nor the single-binary vs subcommand architecture is specified — the implementer has no contract to work from.

**Design assumption violated:** The design assumes the CLI interface for `regen-snapshot-hashes` is obvious from context. It is not: the version-bump check, the `--skip-version-bump-check` bypass, the `--strict` flag, and the database path all need to be composed into a coherent CLI contract with clearly separated modes.

**Suggested mitigation:** Specify the CLI interface explicitly in slice 5.7: either (a) subcommands (`regen-snapshot-hashes verify --database <path> [--strict]` and `regen-snapshot-hashes regenerate-fixtures [--skip-version-bump-check]`) with the version-bump check bound exclusively to `regenerate-fixtures`; or (b) a single invocation where the database path flag (`--database`) activates live-DB mode and `--skip-version-bump-check` is documented as only relevant when generating fixtures. Either approach must be documented in slice 5.7's "Signatures and shapes" block, not left implicit.

---

### [LOW] `WriteError::Timeout` is semantically ambiguous — callers cannot determine whether the write committed, and no recovery protocol is documented

**Category:** Logic Flaws

**Trigger:** The `write_timeout` wraps steps 1–16 inclusive, including `tx.commit().await?` (step 15). If the Tokio timeout fires between the `commit()` future completing and control returning to the caller — a nanosecond-scale window theoretically reachable in a heavily loaded executor — the transaction has committed to the database but the caller receives `WriteError::Timeout(duration)`. If the timeout fires before step 15, the transaction is rolled back by Drop and `Timeout` correctly means "write not committed." In both cases the external signal is identical.

A caller that retries on `Timeout` with the original `expected_version = Some(v)` will either succeed (write didn't land) or receive `VersionConflict { expected: Some(v), current: v+1 }` (write landed). Callers following the `VersionConflict` semantics ("refetch and rebase") may unnecessarily rebase when their write actually committed, potentially applying the wrong snapshot to Caddy in Phase 7.

**Design assumption violated:** The design treats `Timeout` as unambiguous ("the write didn't land") but the timeout wraps the commit step, making this false in a strict sense.

**Suggested mitigation:** Add a doc comment to `WriteError::Timeout` and `SnapshotWriter::write` specifying the recovery protocol: "(1) `Timeout` does not guarantee the write did not commit; (2) on `Timeout`, callers SHOULD query `by_config_version(expected_version + 1)` and check if the returned snapshot's `id` matches the one they attempted to write — if it matches, the write succeeded; (3) do not rebase on `VersionConflict` received after a `Timeout` retry without first performing this identity check." Alternatively, move step 15 (`tx.commit()`) outside the `tokio::time::timeout` wrapper, eliminating the ambiguity entirely — only the pre-commit work is time-bounded.

---

## Summary

**Critical:** 0 &nbsp; **High:** 0 &nbsp; **Medium:** 2 &nbsp; **Low:** 1

**Top concern:** `InRangeCursor` includes `created_at_ms` but `in_range`'s three-column ORDER BY does not — the pagination caveat falsely claims both queries share a four-column ORDER BY. A Phase 7 implementer using the cursor as a keyset seek predicate for `in_range` will produce structurally wrong queries that silently skip or duplicate rows.

**Recommended action before proceeding:** Address the `InRangeCursor` / ORDER BY mismatch and the `regen-snapshot-hashes` CLI interface before implementation begins — both are design-time fixes. The `Timeout` ambiguity can be resolved with a doc comment addition inline during Slice 5.5 implementation.
