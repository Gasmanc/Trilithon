---
id: scope:area::phase-2-learnings-match-review-findings:legacy-uncategorized
category: scope
kind: process
location:
  area: phase-2-learnings-match-review-findings
  multi: false
finding_kind: legacy-uncategorized
phase_introduced: unknown
status: open
created_at: migration
created_by: legacy-migration
last_verified_at: 0a795583ea9c4266e7d9b0ae0f56fd47d2ecf574
severity: medium
do_not_autofix: false
---

# Phase 2 — Learnings Match Review Findings

**Reviewer:** learnings_match
**Date:** 2026-05-07
**Diff range:** be773df..cfba489
**Phase:** 2

---

[WARNING] Known pattern: sqlite-begin-immediate-read-check-write
File: general
Lines: general
Description: Pattern from docs/solutions/ may match this diff
Suggestion: Review docs/solutions/runtime-errors/sqlite-begin-immediate-read-check-write-2026-05-05.md — SQLite read-check-write sequences must use BEGIN IMMEDIATE (not DEFERRED) to acquire the write lock before the invariant check, preventing another writer from inserting between the read and the INSERT.

[WARNING] Known pattern: migration-bootstrap-no-such-table-2026-05-03
File: general
Lines: general
Description: Pattern from docs/solutions/ may match this diff
Suggestion: Review docs/solutions/runtime-errors/migration-bootstrap-no-such-table-2026-05-03.md — When reading migration state from a table that may not exist yet, match on the "no such table" error message to return version 0 for a fresh DB, and propagate everything else.

[WARNING] Known pattern: sqlite-manual-tx-rollback-early-exit
File: general
Lines: general
Description: Pattern from docs/solutions/ may match this diff
Suggestion: Review docs/solutions/runtime-errors/sqlite-manual-tx-rollback-early-exit-2026-05-05.md — When managing SQLite transactions with raw SQL (BEGIN IMMEDIATE / COMMIT), every early-exit code path must issue an explicit ROLLBACK, or the write lock is held until the connection drops.

[WARNING] Known pattern: sqlite-extended-error-codes-mask-2026-05-03
File: general
Lines: general
Description: Pattern from docs/solutions/ may match this diff
Suggestion: Review docs/solutions/runtime-errors/sqlite-extended-error-codes-mask-2026-05-03.md — Always match SQLite error codes on `code & 0xFF` — extended codes include the base code in their low 8 bits and won't match a bare code number.

[WARNING] Known pattern: storage-trait-error-variant-parity
File: general
Lines: general
Description: Pattern from docs/solutions/ may match this diff
Suggestion: Review docs/solutions/runtime-errors/storage-trait-error-variant-parity-2026-05-05.md — When a Storage trait has multiple implementations, every impl must return the same error variant for the same failure condition — divergence makes tests pass while production silently breaks.
