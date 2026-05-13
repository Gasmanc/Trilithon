# Phase 7 — Learnings Match Review Findings

**Reviewer:** learnings_match
**Date:** 2026-05-10
**Diff range:** ddda146..HEAD
**Phase:** 7

---

[WARNING] Known pattern: sqlite-begin-immediate-read-check-write
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/runtime-errors/sqlite-begin-immediate-read-check-write-2026-05-05.md
Suggestion: Review docs/solutions/runtime-errors/sqlite-begin-immediate-read-check-write-2026-05-05.md before proceeding — one_sentence_lesson: "SQLite read-check-write sequences must use BEGIN IMMEDIATE (not DEFERRED) to acquire the write lock before the invariant check, preventing another writer from inserting between the read and the INSERT."

[WARNING] Known pattern: sqlite-manual-tx-rollback-early-exit
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/runtime-errors/sqlite-manual-tx-rollback-early-exit-2026-05-05.md
Suggestion: Review docs/solutions/runtime-errors/sqlite-manual-tx-rollback-early-exit-2026-05-05.md before proceeding — one_sentence_lesson: "When managing SQLite transactions with raw SQL (BEGIN IMMEDIATE / COMMIT), every early-exit code path — including invariant failures and duplicate detection — must issue an explicit ROLLBACK, or the write lock is held until the connection drops."

[WARNING] Known pattern: version-counter-checked-add-overflow
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/runtime-errors/version-counter-checked-add-overflow-2026-05-06.md
Suggestion: Review docs/solutions/runtime-errors/version-counter-checked-add-overflow-2026-05-06.md before proceeding — one_sentence_lesson: "Version counters that use unchecked integer addition will panic at i64::MAX — use checked_add and map None to a domain error so the caller can handle it"

[WARNING] Known pattern: audit-diff-before-from-original-state
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/runtime-errors/audit-diff-before-from-original-state-2026-05-06.md
Suggestion: Review docs/solutions/runtime-errors/audit-diff-before-from-original-state-2026-05-06.md before proceeding — one_sentence_lesson: "Audit diff before-state must be captured from the original state before mutation, not from the clone being mutated — reading from new_state always produces identical before/after pairs"

[WARNING] Known pattern: apply-layer-self-defend-missing-field
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/runtime-errors/apply-layer-self-defend-missing-field-2026-05-06.md
Suggestion: Review docs/solutions/runtime-errors/apply-layer-self-defend-missing-field-2026-05-06.md before proceeding — one_sentence_lesson: "An apply function that silently no-ops when a required field is absent produces a successful mutation with no observable effect — always propagate an error so the caller knows the operation was a no-op"

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-13 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | BEGIN IMMEDIATE pattern (general advisory) | 🔕 Superseded | — | — | — | Root cause covered by F001/F003; F003 already fixed in Phase 7 |
| 2 | Rollback early-exit (general advisory) | 🔕 Superseded | — | — | — | Root cause covered by F007; fixed in 36af1e7 |
| 3 | Version overflow (general advisory) | 🔕 Superseded | — | — | — | Advisory reminder; not an independent finding |
| 4 | Audit-diff pattern (general advisory) | 🔕 Superseded | — | — | — | Advisory reminder; not an independent finding |
| 5 | Apply-layer self-defend (general advisory) | 🔕 Superseded | — | — | — | Advisory reminder; not an independent finding |
