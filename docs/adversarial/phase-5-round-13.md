# Adversarial Review — Phase 05 — Round 13

**Design summary:** A content-addressed, append-only SQLite snapshot store for a Caddy reverse-proxy configuration daemon, implemented in a three-layer Rust workspace. Writes are guarded by OCC version checks, database-level immutability triggers, SHA-256 content addressing, and a multi-step deduplication protocol. A separate verification-only CLI command re-hashes stored snapshots against the current canonical JSON version.

**Prior rounds:** 12 prior rounds reviewed — all previously identified issues are marked as addressed. No prior findings are re-raised below.

---

## Findings

### [MEDIUM] `children_of` has the same offset-instability as `in_range` but returns no cursor and has no documented skew-detection mechanism

**Category:** Logic Flaws

**Trigger:** A caller paginates through `children_of(parent, page)` using `LIMIT ? OFFSET ?`. A concurrent writer inserts a new child of `parent` whose four-key sort position falls before the current page offset. The next page request with `OFFSET = page * limit` causes the result set to shift — the last row of the previous page is either skipped or duplicated on the next page. The same offset instability documented for `in_range` applies here identically.

**Consequence:** The caller iterating all children of a parent node silently misses snapshots or sees duplicates. Unlike `in_range`, there is no `next_cursor` / `ChildrenCursor` return value for `children_of`, so callers cannot detect that skew has occurred. The design documents the gap for `in_range` but leaves `children_of` with neither a cursor nor a documentation warning, making the behavior asymmetric and the omission invisible.

**Design assumption violated:** The design treats `children_of` as subject to the same ordering rules as `in_range` (four-key ORDER BY) but does not extend the same skew-detection mechanism. The assumption appears to be that `children_of` callers are only used for chain reconstruction via parent pointers (which is stable), but the API signature does not enforce this; any caller can use it for ordered child iteration.

**Suggested mitigation:** Either (a) add an explicit doc comment to `children_of` stating that offset-based pagination is subject to insertion skew, that callers requiring completeness must use parent-pointer traversal, and that a keyset cursor is deferred to Phase 7 — matching the `in_range` treatment; or (b) add a `ChildrenResult` wrapper returning a `next_cursor: Option<ChildrenCursor>` analogous to `InRangeCursor`, since the four-key ORDER BY is already defined. Option (a) is lower scope; option (b) is consistent with the `in_range` design.

---

### [MEDIUM] `InRangeCursor` provides no actionable recovery contract — callers that detect skew have no documented path to resume correctly

**Category:** Logic Flaws

**Trigger:** A client paginates `in_range`, receives page 1 with `next_cursor = Some(c)`. A concurrent insert causes skew. The client detects the skew by comparing `c` against the first row of page 2 — they do not match in sort order. What should the client do? The design says "use `next_cursor` to detect skew" and documents keyset pagination as Phase 7. No documented recovery action exists for the period between Phase 5 and Phase 7.

**Consequence:** Callers that detect skew have three options — all undocumented: (1) restart from offset 0 (O(N) scan, potentially triggering skew again), (2) accept the gap and move on (silent data loss in audit use cases), (3) wait and retry (no backoff or retry guidance provided). A Phase 9 HTTP endpoint built on `in_range` with no recovery documentation will silently expose incomplete results to API consumers.

**Design assumption violated:** The design assumes that providing a cursor for skew *detection* is sufficient for Phase 5, and that callers will wait until Phase 7 for resumption. But callers do not have a documented fallback for Phase 5 operation. A detection mechanism without a recovery path is incomplete.

**Suggested mitigation:** Add a doc comment to `InRangeResult` and `InRangeCursor` explicitly stating: "(1) the cursor is detection-only in Phase 5 — not a seek key; (2) on skew detection, callers MUST restart from offset 0 if completeness is required, or document that they tolerate gaps; (3) keyset seek will be added in Phase 7 to make resumption O(1)." This converts the gap from a silent trap into a documented contract. Add a test `tests::in_range_next_cursor_is_none_when_no_rows_returned` and `tests::in_range_next_cursor_matches_last_row_sort_key` to pin the contract in code.

---

### [LOW] `with_limits` construction-time formula warn is always-on for deployments that reserve headroom in `max_desired_state_bytes` — permanently noisy for well-configured systems

**Category:** Logic Flaws

**Trigger:** An operator sets `with_limits(max_bytes=10_MB, write_timeout=5s)` as a conservative ceiling. The construction-time formula computes `5s < 1ms/KiB * 10_MB + busy_timeout + 500ms ≈ 10.5s` and emits a `WARN`. At runtime, every write is 1 KB — the per-call Step-3a warn never fires. The construction-time warn fires once at startup and remains misleading forever: the operator's configuration is correct for their actual workload, but the ceiling-based formula says it is wrong.

**Consequence:** Operators in a production deployment see a startup `WARN` that encourages them to reduce `max_desired_state_bytes` or increase `write_timeout`. If they reduce `max_desired_state_bytes` to silence the warn, they lose the safety headroom. If they increase `write_timeout`, they mask real timeouts. If they learn to ignore the warn, the warn stops being useful for genuine misconfiguration.

**Design assumption violated:** The construction-time formula check assumes `max_bytes` is representative of expected write sizes, not a ceiling. The per-call Step-3a warn was added (R12) to address exactly this gap, making the construction-time warn redundant and noisier than useful.

**Suggested mitigation:** Change the construction-time formula warn from `WARN` to `DEBUG`. Keep the check to preserve the diagnostic value for developers reading verbose logs, but stop surfacing it as a production-level warning. The per-call Step-3a `WARN` is the actionable signal for production; the construction-time check is informational.

---

## Summary

**Critical:** 0 &nbsp; **High:** 0 &nbsp; **Medium:** 2 &nbsp; **Low:** 1

**Top concern:** `InRangeCursor` is positioned as a skew-detection mechanism but provides no documented recovery path when skew is detected — callers have no guidance on what to do when the cursor signals that their paginated result is incomplete, leaving Phase 9 API consumers with silent correctness gaps.

**Recommended action before proceeding:** Address the `InRangeCursor` recovery contract and the `children_of` documentation gap before Phase 6. The construction-time warn downgrade is cosmetic and can be done inline.
