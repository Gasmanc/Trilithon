# Design Decision — Phase 05

**Date:** 2026-05-05
**Rounds:** 14
**Final approach:** A content-addressed, append-only SQLite snapshot store backed by `BEGIN IMMEDIATE` OCC, SHA-256 content addressing, per-instance monotonic `config_version`, and database-level immutability triggers. Writes flow through a 16-step algorithm in `SnapshotWriter::write`; reads are served by `SnapshotFetcher` with instance-scoped queries throughout. The `regen-snapshot-hashes` binary is verification-only.

---

## Rejected Approaches

| Approach | Rejected because |
|----------|-----------------|
| `SELECT MAX(config_version)` without caller-supplied `expected_version` | R1: silently accepts stale-state mutations — two actors writing against the same base state both succeed. ADR-0012 requires the caller's observed version as the OCC guard. |
| `BEGIN` (deferred) instead of `BEGIN IMMEDIATE` for the write transaction | R1/R2: TOCTOU between MAX-read and INSERT causes opaque `UNIQUE constraint failed` errors with no typed recovery path. `BEGIN IMMEDIATE` holds the write lock across both operations. |
| `INSERT OR IGNORE` in step 13 | R9: applies `IGNORE` conflict resolution to ALL constraints including future `CHECK` constraints and `BEFORE INSERT` triggers, silently discarding rows and misclassifying failures as `VersionRace`. Plain `INSERT` required. |
| `length(NEW.intent)` in the immutability trigger | R2: SQLite `length()` on TEXT returns codepoint count, not bytes. `length(CAST(NEW.intent AS BLOB))` is required to match Rust's `.len()` semantics. |
| `regen-snapshot-hashes` issuing `UPDATE` or `DELETE` to rehash rows | R10: migration 0004's `BEFORE UPDATE` / `BEFORE DELETE` triggers (RAISE ABORT) unconditionally block every such attempt. Rehash via mutation is architecturally impossible; the binary is scoped to read-only verification. |
| Cross-instance `id` lookup in step 7 and step 14 collision dispatch | R9/R10: without `AND caddy_instance_id = self.instance_id`, a second instance with identical content receives `Deduplicated` while holding zero rows — data loss with a success signal. All queries are instance-scoped. |
| `changes()` as the insert-success signal in step 14 | R8/R5: SQLite's `changes()` is reset by any row-modifying trigger that fires after the INSERT. A future audit trigger would make it unreliable. Replaced by a direct `SELECT id FROM snapshots WHERE id = ? AND config_version = ?` existence query. |
| `tx.commit()` on the `Deduplicated` path inside `BEGIN IMMEDIATE` | R8: the INSERT was rejected — nothing was written. Committing permanently consumes the `config_version` slot and creates a gap that downstream consumers cannot distinguish from data loss. `tx.rollback()` is required. |
| `created_at_ms` as primary sort key for `in_range` | R7/R8: NTP corrections produce non-monotonic wall-clock values, yielding wrong history-replay order. Three-key compound `(created_at_monotonic_nanos, daemon_run_id, config_version)` used instead. |
| SHA-256 hex `id` as a tiebreaker in `children_of` ORDER BY | R6: SHA-256 hashes of distinct content are uniformly distributed — `id ASC` produces content-ordered, not arrival-ordered, results within the same millisecond. `config_version` is the tiebreaker. |
| `HashCollision` returned from inside the open transaction without rollback | R7: the connection is permanently wedged if the transaction is abandoned without rollback. Explicit `tx.rollback().await?` required before returning `HashCollision`. |
| `with_limits` returning `Self` without `#[must_use]` | R7: silently discarding the return value leaves the writer with its original limits unchanged — a class of bug that is invisible in production until a timeout or size check fails. `#[must_use]` enforced. |
| Pre-transaction `expected_version = None` checking current_max == -1 sentinel | R5: two concurrent root-creation callers with different content both pass the OCC check, producing two parentless root nodes. The `None` predicate must verify that zero rows exist for the instance, not compare against a sentinel. |
| Single-mode `regen-snapshot-hashes` with no explicit `BEGIN DEFERRED` read transaction | R11: in WAL mode, implicit per-statement read transactions see different checkpoints across batched SELECTs. A concurrent writer commits between two SELECT batches, producing false-positive integrity anomalies. |
| `regen` treating `version > CURRENT` rows as "legacy" with exit 0 | R12: a binary that cannot verify future-version rows MUST NOT exit zero — doing so gives operators false confidence. `version > CURRENT` always exits non-zero. |
| Step-14 "not found" arm returning `VersionRace` for an impossible state | R12: a UNIQUE violation fired on `id`, so some row with that `id` must exist. "Not found" is a storage invariant violation, not a retryable race. Returns `WriteError::InvariantViolated` instead. |
| Construction-time `write_timeout` formula check emitting `WARN` | R13: the formula checks `max_desired_state_bytes` (the ceiling), not the actual payload. Deployments with generous ceilings but small typical writes see a permanent spurious WARN on every startup. Downgraded to `tracing::debug!`. Per-call step-0 check against `bytes.len()` is the actionable signal. |

---

## Key Constraints Surfaced

The adversarial process revealed these constraints that any implementation must respect:

1. **`BEGIN IMMEDIATE` is load-bearing.** The MAX-read (step 10) and INSERT (step 13) must be atomic. Any implementation that separates them with a deferred transaction reintroduces the TOCTOU race from R1.

2. **All SQL queries must include `AND caddy_instance_id = self.instance_id`.** This applies to: step 7 (both the length check and the full-body fetch), step 9 (parent existence), step 10 (MAX version), step 14 (collision dispatch — both the instance-scoped length check and the full-body fetch), `by_id`, `children_of`, and `by_config_version`. The only exception is `by_id_global`, which is `pub(crate)` and guarded by a call-site grep test.

3. **Step-14 existence query MUST bind both `id` AND `config_version`.** This is the correctness pivot for the concurrent-identical-payload race: writer 2's query `WHERE id = writer_2_id AND config_version = writer_2_new_version` finds no row (because writer 1 committed at a different version), correctly routing to the instance-scoped fallback dedup path. Checking only `id` misclassifies this case as a landed insert.

4. **Plain `INSERT` in step 13, never `INSERT OR IGNORE`.** Constraint classification logic depends on receiving the actual error; `INSERT OR IGNORE` swallows it.

5. **Two-stage monotonic clamp is required.** `u128 → u64` (first), then `u64 → i64` (second). SQLite `INTEGER` is signed 64-bit; a `u64` value above `i64::MAX` bound to the column stores as a negative number, silently breaking `ORDER BY created_at_monotonic_nanos ASC`.

6. **`Deduplicated` from the step-14 collision path MUST call `tx.rollback()`.** Nothing was written; committing creates a permanent `config_version` gap.

7. **`daemon_clock::override_run_id_for_current_thread` requires `flavor = "current_thread"`.** Under `multi_thread`, Tokio task migration at `.await` points silently loses the thread-local, causing the test to write rows with the wrong `daemon_run_id`.

8. **`regen-snapshot-hashes` is verification-only.** Immutability triggers make in-place rehash impossible. Hash migration after a `CANONICAL_JSON_VERSION` bump requires a separate ADR and is out of scope for Phase 5.

9. **`regen` MUST wrap the entire scan in a single `BEGIN DEFERRED` read transaction.** Without it, concurrent writers cause false-positive integrity anomalies across batched SELECTs in WAL mode.

10. **`InRangeCursor` is detection-only in Phase 5 — not a seek key.** The `in_range` ORDER BY is three columns `(created_at_monotonic_nanos, daemon_run_id, config_version)` and does not include `created_at_ms`. Phase 7 must use a three-column keyset predicate for `in_range` seeks, not the four-column cursor tuple. The cursor's `created_at_ms` field is present for caller diagnostics only. The pagination caveat doc comment must be corrected before implementation: it falsely claims both `children_of` and `in_range` share a four-column ORDER BY.

11. **`regen-snapshot-hashes` CLI must separate live-DB verification from fixture regeneration.** The version-bump enforcement gate (`--skip-version-bump-check`) applies only to fixture regeneration, not to live-DB verification. The CLI contract must be explicit — either as distinct subcommands or as clearly scoped flags — so production operators can run verification without triggering the gate.

12. **`VersionOverflow` guard at step 12.** If `current_max == i64::MAX`, return `WriteError::VersionOverflow` immediately. Without the guard, step 1 rejects all future writes (negative `expected_version`), permanently wedging the instance.

13. **OCC `None` predicate checks row count, not sentinel equality.** `expected_version = None` is valid only when zero rows exist for the instance. The check `expected_version.is_none() || expected_version == Some(current_max)` is wrong — the `is_none()` branch must query the database, not compare to a sentinel.

---

## Unaddressed Findings

Findings raised in the final round and accepted as known risk or deferred:

| ID | Severity | Finding | Accepted because |
|----|----------|---------|-----------------|
| R14-F1 | MEDIUM | `InRangeCursor` includes `created_at_ms` but `in_range`'s ORDER BY does not — Phase 7 keyset predicate built from this cursor will be structurally wrong | Recorded as Constraint 10 above. Phase 7 implementers must use a three-column predicate for `in_range`. The cursor field is retained for caller diagnostics. The pagination caveat must be corrected during implementation of Slice 5.6. |
| R14-F2 | MEDIUM | `regen-snapshot-hashes` CLI does not specify how to separate live-DB verification from fixture regeneration — version-bump gate may block production operators | Recorded as Constraint 11 above. Slice 5.7 implementer must define the subcommand or flag interface explicitly before writing the CLI code. |
| R14-F3 | LOW | `WriteError::Timeout` is semantically ambiguous — callers cannot determine if the write committed before the timeout fired | Accepted as documented. Mitigation: add a doc comment to `WriteError::Timeout` specifying the recovery protocol (query `by_config_version(expected_version + 1)` and match `id` to determine if the write landed). Can be done inline during Slice 5.5 implementation. |

---

## Round Summary

| Round | Critical | High | Medium | Low | Outcome |
|-------|----------|------|--------|-----|---------|
| 1 | 2 | 5 | 4 | 2 | OCC contract added (`expected_version`); `BEGIN IMMEDIATE` adopted; cross-instance parent rejection added; `DesiredState` size cap added; write timeout added |
| 2 | 0 | 4 | 4 | 2 | `CAST(... AS BLOB)` for intent trigger; intent-length trigger semantics clarified; rollback hygiene on `HashCollision`; `ClockOverflow` error |
| 3 | 0 | 3 | 3 | 2 | `daemon_clock` module introduced; step-14 dispatch redesigned; `regen` version-bump guard added |
| 4 | 0 | 1 | 3 | 2 | Step-7 length check corrected to BLOB cast; step-14 diagnostic SELECT moved outside timeout-critical path; concurrent-dedup test seam specified |
| 5 | 0 | 2 | 3 | 1 | `changes()` reliability issue addressed; OCC `None` predicate respecified; `regen` version filter added; `daemon_clock::override_run_id_for_current_thread` introduced |
| 6 | 0 | 2 | 3 | 1 | `VersionOverflow` guard added; `write_timeout` documentation corrected; `id` tiebreaker removed from `children_of`; `MAX_PAGE_OFFSET` added |
| 7 | 0 | 2 | 3 | 1 | `HashCollision` rollback hygiene fixed; `#[must_use]` on `with_limits`; `in_range` ORDER BY changed to `(created_at_monotonic_nanos, daemon_run_id, config_version)`; `by_id_global` restricted to `pub(crate)` with grep test; `children_of` instance scoping added |
| 8 | 0 | 2 | 3 | 1 | `Deduplicated` path changed to `tx.rollback()`; `daemon_run_id` added as second sort key in `in_range`; `busy_timeout` relationship documented; `regen` partial-failure mode specified; `by_id_global` call-site grep test added |
| 9 | 0 | 2 | 3 | 1 | Step-7 instance-scoped; `INSERT OR IGNORE` → plain `INSERT`; two-stage monotonic clamp; `with_limits` default enforcement documented; `children_of` ordering rationalized |
| 10 | 1 | 1 | 0 | 0 | `regen` redefined as verification-only (hash migration deferred); step-14 collision dispatch instance-scoped |
| 11 | 0 | 1 | 2 | 1 | Step-14 `config_version` pivot documented as load-bearing; `children_of`/`in_range` offset instability documented; `regen` `BEGIN DEFERRED` read transaction required; `override_run_id_for_current_thread` restricted to `current_thread` |
| 12 | 0 | 1 | 2 | 1 | Step-14 "not found" arm → `InvariantViolated`; step-0 warn changed to check `bytes.len()`; `regen` `version > CURRENT` always exits non-zero; `InRangeCursor` added to `in_range` return type |
| 13 | 0 | 0 | 2 | 1 | `children_of` doc comment added for offset instability; `InRangeCursor` Phase 5 contract documented; `with_limits` construction-time warn downgraded to DEBUG |
| 14 | 0 | 0 | 2 | 1 | `InRangeCursor`/ORDER BY mismatch and `regen` CLI ambiguity recorded as constraints; `Timeout` ambiguity documented as known risk |
