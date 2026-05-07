# Phase 1 — Learnings Match Review Findings

**Reviewer:** learnings_match
**Date:** 2026-05-06T13:30:00Z
**Diff range:** 3734e02^..be773df -- core/ web/
**Phase:** 1

---

No direct pattern matches from `docs/solutions/` for Phase 1 code. The top 5 closest historical patterns (from later phases) are:

[WARNING] Known pattern: version-counter-checked-add-overflow
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/runtime-errors/version-counter-checked-add-overflow-2026-05-06.md
Suggestion: Review docs/solutions/runtime-errors/version-counter-checked-add-overflow-2026-05-06.md before proceeding — one_sentence_lesson: "Version counters that use unchecked integer addition will panic at i64::MAX — use checked_add and map None to a domain error so the caller can handle it"

[WARNING] Known pattern: sqlite-manual-tx-rollback-early-exit
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/runtime-errors/sqlite-manual-tx-rollback-early-exit-2026-05-05.md
Suggestion: Review docs/solutions/runtime-errors/sqlite-manual-tx-rollback-early-exit-2026-05-05.md before proceeding — one_sentence_lesson: "When managing SQLite transactions with raw SQL (BEGIN IMMEDIATE / COMMIT), every early-exit code path — including invariant failures and duplicate detection — must issue an explicit ROLLBACK, or the write lock is held until the connection drops."

[WARNING] Known pattern: schema-version-column-at-creation
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/best-practices/schema-version-column-at-creation-2026-05-05.md
Suggestion: Review docs/solutions/best-practices/schema-version-column-at-creation-2026-05-05.md before proceeding — one_sentence_lesson: "When a Rust model field carries a schema-version marker, add a DB column for it immediately rather than defaulting at read time — retrofitting after a format bump is much more expensive than adding the column upfront."

[WARNING] Known pattern: cidr-validate-at-mutation-boundary
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/security-issues/cidr-validate-at-mutation-boundary-2026-05-06.md
Suggestion: Review docs/solutions/security-issues/cidr-validate-at-mutation-boundary-2026-05-06.md before proceeding — one_sentence_lesson: "CIDR strings accepted at the API boundary must be validated at mutation time — invalid notation stored in DesiredState cannot be caught by Caddy until config push fails at apply time"

[WARNING] Known pattern: caddy-admin-api-put-not-json-patch
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/runtime-errors/caddy-admin-api-put-not-json-patch-2026-05-06.md
Suggestion: Review docs/solutions/runtime-errors/caddy-admin-api-put-not-json-patch-2026-05-06.md before proceeding — one_sentence_lesson: "Caddy's admin API PUT /config/[path] expects the replacement value directly — not an RFC6902 JSON Patch ops array — so any mutation of live Caddy config must use PUT with the replacement value, not PATCH with a patch document"

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-07 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Advisory: version-counter-checked-add-overflow | 🚫 Won't Fix | — | — | 2026-05-07 | Advisory only — no Phase 1 code; apply when version counters are introduced (F055) |
| 2 | Advisory: sqlite-manual-tx-rollback-early-exit | 🚫 Won't Fix | — | — | 2026-05-07 | Advisory only — no Phase 1 code; apply when SQLite transactions are introduced (F056) |
| 3 | Advisory: schema-version-column-at-creation | 🚫 Won't Fix | — | — | 2026-05-07 | Advisory only — no Phase 1 code; apply when DB schema is introduced (F057) |
| 4 | Advisory: cidr-validate-at-mutation-boundary | 🚫 Won't Fix | — | — | 2026-05-07 | Advisory only — no Phase 1 code; apply when CIDR config fields are introduced (F058) |
| 5 | Advisory: caddy-admin-api-put-not-json-patch | 🚫 Won't Fix | — | — | 2026-05-07 | Advisory only — no Phase 1 code; apply when Caddy integration is implemented (F059) |
