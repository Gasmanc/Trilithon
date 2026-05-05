# Adversarial Review — Phase 05 — Round 9

**Design summary:** A content-addressed, append-only snapshot store backed by SQLite using optimistic concurrency control, SHA-256 content addressing, instance-scoped versioning, and a compound cross-restart sort key for ordered history replay.

**Prior rounds:** 8 prior rounds reviewed — all previously identified issues are marked as addressed in the design. No prior findings are re-raised below.

---

## Findings

### [HIGH] Pre-transaction dedup check (step 7) is not instance-scoped — cross-instance content match returns `Deduplicated` without inserting a row for the requesting instance

**Category:** Authentication & Authorization

**Trigger:** Step 7 queries `SELECT length(CAST(desired_state_json AS BLOB)) AS json_len FROM snapshots WHERE id = ? LIMIT 1`. This query has no `caddy_instance_id` filter. If instance A and instance B happen to write identical `DesiredState` content (same SHA-256 `id`), instance B's step-7 pre-check finds the row written by instance A. When lengths match, step 7 fetches the full body, byte-compares (equal), and returns `WriteOutcome::Deduplicated { snapshot: fetch_full_row(id).await? }` — **without the calling instance (B) ever having a row inserted**. Instance B receives a `Deduplicated` result but has zero rows in `snapshots` for its own `caddy_instance_id`.

**Consequence:** The caller of instance B believes the write succeeded (`Deduplicated` is a success outcome), but `SELECT * FROM snapshots WHERE caddy_instance_id = 'B'` returns no rows. Phase 7 rollback, Phase 9 history API, and any consumer that queries by `caddy_instance_id` will see instance B as having no configuration history, while instance B's caller believes it does. Data loss with a success signal.

**Design assumption violated:** The design assumes that a `Deduplicated` result always means "a row for this instance exists". This is true for the within-transaction step-14 dispatch (which fires inside `BEGIN IMMEDIATE` after `INSERT OR IGNORE` was suppressed) but false for the pre-transaction step-7 check, which spans all instances.

**Suggested mitigation:** Add `AND caddy_instance_id = ?` (binding `self.instance_id`) to both queries in step 7: the length-check query and the full-body-fetch query. With instance scoping, a match in step 7 can only occur if a prior write from this same instance produced that id — which is the only case where returning `Deduplicated` without inserting is correct.

---

### [HIGH] `INSERT OR IGNORE` silently masks non-UNIQUE constraint violations — any future `CHECK` constraint or trigger that raises `ABORT` is swallowed

**Category:** Logic Flaws

**Trigger:** Step 13 uses `INSERT OR IGNORE`. SQLite's `OR IGNORE` conflict resolution applies to ALL constraint violations, not just `UNIQUE`. If a future migration adds a `CHECK` constraint on `desired_state_json` (e.g., `CHECK(json_valid(desired_state_json))`), or if a new `BEFORE INSERT` trigger raises `ABORT` for an application-level rule, `INSERT OR IGNORE` silently discards the row instead of surfacing the error. Step 14's post-insert existence check then finds no row — routes to the collision path — runs the diagnostic SELECT, finds no `id` at all — surfaces as `VersionRace`. The real cause (constraint violation) is permanently obscured.

**Consequence:** A future schema-level invariant violation is silently misclassified as a version race. The caller receives `WriteError::VersionRace` (retryable) for what is actually a permanent data integrity failure. All retries will produce the same mismatch. The system enters a silent retry loop with no diagnostic signal.

**Design assumption violated:** The design uses `INSERT OR IGNORE` to gracefully handle the UNIQUE(id) dedup case. It assumes the only constraint that can fire is the UNIQUE index. This assumption is fragile: the existing `snapshots_intent_length_cap` trigger raises `ABORT` (not `IGNORE`), which SQLite escalates regardless of the conflict-resolution clause. However, future row-level constraints could be silently swallowed.

**Suggested mitigation:** Replace `INSERT OR IGNORE` with a plain `INSERT`. The step-14 post-insert existence check already handles the case where the INSERT was suppressed by the `UNIQUE(id)` constraint (a concurrent same-content insert from the same or another instance). With a plain `INSERT`, any non-UNIQUE constraint violation propagates as a `sqlx::Error` instead of being silently absorbed. The step-14 dispatch is unaffected: if the INSERT raised a UNIQUE violation, it is an error — catch it, roll back the transaction, and proceed to step-14 diagnostics. If it raised any other constraint error, propagate it as `WriteError::Sqlx`.

---

### [MEDIUM] `created_at_monotonic_nanos` stored as SQLite `INTEGER` is signed i64 — u64 values above `i64::MAX` store as negative, breaking `ORDER BY`

**Category:** Logic Flaws

**Trigger:** Step 6 computes `created_at_monotonic_nanos` as a `u64`, clamping u128 overflow to `u64::MAX` with a warning. SQLite `INTEGER` is a signed 64-bit type; the maximum value it can store is `i64::MAX` (approximately 9.2 × 10^18 ns ≈ 292 years of uptime). A `u64` value in the range `(i64::MAX, u64::MAX]` is stored as a negative `INTEGER` in SQLite. After ~292 years of continuous uptime, every subsequent `created_at_monotonic_nanos` value overwrites to a negative number, and `ORDER BY created_at_monotonic_nanos ASC` places these rows before all legitimate (positive) rows — breaking the compound sort key that `in_range` and `children_of` rely on.

**Consequence:** After ~292 years of daemon uptime without a restart, `in_range` ordering degrades silently. `ORDER BY created_at_monotonic_nanos ASC` places all post-saturation snapshots before pre-saturation snapshots. The `daemon_run_id ASC` secondary key does not rescue ordering here because the negative value comparison happens on the primary key.

**Design assumption violated:** The design clamps u128 → u64 but does not clamp u64 → i64 (SQLite's actual storage range). A second clamp is needed before binding the value to the SQL parameter.

**Suggested mitigation:** After the u64 saturation clamp, add a second clamp: `let created_at_monotonic_nanos = created_at_monotonic_nanos.min(i64::MAX as u64);` with a `tracing::warn!` if the clamp fires. This ensures the value bound to the SQL parameter is always representable as a non-negative SQLite `INTEGER`. Document: "The effective maximum continuous uptime before `created_at_monotonic_nanos` saturates is ~292 years; at saturation, ordering degrades to `daemon_run_id` + `config_version` as tiebreakers."

---

### [MEDIUM] `with_limits` default values are undocumented and untested — callers relying on defaults have no signal if the defaults are wrong for their workload

**Category:** Logic Flaws

**Trigger:** `SnapshotWriter::new` constructs with `DEFAULT_MAX_DESIRED_STATE_BYTES = 10 MiB` and `DEFAULT_WRITE_TIMEOUT = 5s`. Callers who never call `with_limits` silently inherit these values. The only guidance is a doc comment on `with_limits`. There is no `tracing::warn!` in `write()` if the caller is using defaults with a large payload that might exceed the timeout, and no test that exercises the default path end-to-end.

**Consequence:** A caller deploying a 9 MiB `DesiredState` without calling `with_limits` will see spurious `Timeout` errors under load — the timeout formula says they need `> 1ms/KiB * 9216 + busy_timeout + 500ms = ~14.7s+` for a 9 MiB state. The 5s default is insufficient. There is no warning at write time to prompt them to call `with_limits`.

**Design assumption violated:** The design assumes callers read the `with_limits` doc comment and adjust accordingly. In practice, a caller who never calls `with_limits` receives no signal that their default configuration may be inadequate.

**Suggested mitigation:** In `SnapshotWriter::write()`, before beginning the operation, run the same formula check that `with_limits` runs — warn if the current limits appear undersized for the actual payload being written. This gives callers a runtime signal even if they never called `with_limits`. The check costs one comparison and one `tracing::warn!` call — negligible. Add a test `tests::default_limits_emit_warn_for_large_payload` using `tracing_test`.

---

### [MEDIUM] `children_of` ordering uses wall-clock `created_at_ms` as primary key — same cross-restart problem as `in_range` before round-8 fix

**Category:** Logic Flaws

**Trigger:** `children_of` sorts by `ORDER BY created_at_ms ASC, config_version ASC`. Round 8 corrected `in_range` to use `(created_at_monotonic_nanos, daemon_run_id, config_version)` for precisely this reason: NTP corrections can produce non-monotonic `created_at_ms` values, reordering snapshots that were written in wall-clock sequence. The same issue applies to `children_of`: two child snapshots written in sequence across an NTP adjustment or daemon restart have their order inverted by `created_at_ms ASC`.

**Consequence:** `children_of` returns children in an order that may not reflect the actual write sequence — specifically when writes straddle an NTP correction or a daemon restart within the same millisecond bucket. For a Phase 7 consumer that replays a config change chain, wrong child ordering produces incorrect rollback targets.

**Design assumption violated:** The round-8 fix for `in_range` reflects the design's awareness that `created_at_ms` is not monotonic. `children_of` was not updated in the same round, leaving it with the pre-fix ordering.

**Suggested mitigation:** Update `children_of` ORDER BY to `ORDER BY created_at_ms ASC, created_at_monotonic_nanos ASC, daemon_run_id ASC, config_version ASC`. This matches the `in_range` fix: `created_at_ms` provides coarse filtering, `created_at_monotonic_nanos` provides fine ordering within a run, `daemon_run_id` resolves cross-restart ordering, and `config_version` is the final tiebreaker.

---

### [LOW] `regen-snapshot-hashes` has no `--strict` flag — operators cannot get a non-zero exit when legacy rows are present without configuring an external grep

**Category:** Logic Flaws

**Trigger:** The tool exits non-zero only when `N == 0 AND M > 0` (entirely legacy DB). An operator running the tool in a CI pipeline after a partial migration — where both current and legacy rows exist (`N > 0 AND M > 0`) — receives exit 0. The operator must parse the summary text to detect skipped rows. CI pipelines typically treat non-zero exit as failure; text parsing is fragile.

**Consequence:** A CI integrity check using `regen-snapshot-hashes` on a DB with mixed canonical versions passes (exit 0) when the operator expected failure. The skipped rows are reported only in the summary text.

**Suggested mitigation:** Add a `--strict` flag. When passed, the tool exits non-zero if `M_skipped > 0` (any rows at a legacy `canonical_json_version`). Default behavior (exit 0 when `N > 0`) is unchanged. Document in the CLI help: `"--strict: exit non-zero if any rows at a legacy canonical_json_version are skipped"`. This gives CI pipelines a clean binary signal without text parsing.

---

## Summary

**Critical:** 0 &nbsp; **High:** 2 &nbsp; **Medium:** 3 &nbsp; **Low:** 1

**Top concern:** The pre-transaction dedup check in step 7 has no `caddy_instance_id` filter. A cross-instance content collision causes `Deduplicated` to be returned to instance B's caller while instance B has zero rows in the `snapshots` table — data loss with a success signal. The fix is two words: add `AND caddy_instance_id = ?` to both step-7 queries.
