---
id: scope:area::phase-4-learnings-match-review-findings:legacy-uncategorized
category: scope
kind: process
location:
  area: phase-4-learnings-match-review-findings
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

# Phase 4 — Learnings Match Review Findings

**Reviewer:** learnings_match
**Date:** 2026-05-06
**Diff range:** 43f89ca..a948fa8
**Phase:** 4

---

[WARNING] Known pattern: no-unreachable-in-production-match-2026-05-03
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/conventions/no-unreachable-in-production-match-2026-05-03.md. This doc was written about a `Mutation` enum and `apply_variant` — exactly the pattern Phase 4 is building. Every arm of the new `Mutation` enum in `apply_mutation` must return a structured error, not `unreachable!()`.
Suggestion: Review docs/solutions/conventions/no-unreachable-in-production-match-2026-05-03.md before proceeding — one_sentence_lesson: "Replace unreachable!() in production match arms with a proper error return — concurrent writes or future enum additions can reach arms that look impossible at review time, and a panic is worse than a handled error."

[WARNING] Known pattern: serde-json-number-variant-before-f64-cast
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/runtime-errors/serde-json-number-variant-before-f64-cast-2026-05-05.md. The doc explicitly references `DesiredState` JSON canonicalisation. If Phase 4 adds JSON schema generation or serialisation for `DesiredState`, this precision trap applies directly.
Suggestion: Review docs/solutions/runtime-errors/serde-json-number-variant-before-f64-cast-2026-05-05.md before proceeding — one_sentence_lesson: "When canonicalising serde_json::Value numbers, call n.is_f64() before n.as_f64() — i64/u64 values silently lose precision when routed through f64 for integers larger than 2^53."

[WARNING] Known pattern: sqlite-begin-immediate-read-check-write
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/runtime-errors/sqlite-begin-immediate-read-check-write-2026-05-05.md. Phase 4 introduces optimistic concurrency on `DesiredState`. Any path that reads the current state, checks a version invariant, then writes the updated state must use BEGIN IMMEDIATE if backed by SQLite — or the optimistic-concurrency guard will have a TOCTOU window.
Suggestion: Review docs/solutions/runtime-errors/sqlite-begin-immediate-read-check-write-2026-05-05.md before proceeding — one_sentence_lesson: "SQLite read-check-write sequences must use BEGIN IMMEDIATE (not DEFERRED) to acquire the write lock before the invariant check, preventing another writer from inserting between the read and the INSERT."

[WARNING] Known pattern: schema-version-column-at-creation
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/best-practices/schema-version-column-at-creation-2026-05-05.md. If `DesiredState` or associated types carry a schema or encoding version field, add the DB column in the same migration — not as a default at read time.
Suggestion: Review docs/solutions/best-practices/schema-version-column-at-creation-2026-05-05.md before proceeding — one_sentence_lesson: "When a Rust model field carries a schema-version marker, add a DB column for it immediately rather than defaulting at read time — retrofitting after a format bump is much more expensive than adding the column upfront."

[WARNING] Known pattern: three-state-patch-double-option-2026-05-03
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/architecture-patterns/three-state-patch-double-option-2026-05-03.md. If any variant in the `Mutation` enum accepts optional fields that can be cleared (set back to absent), plain `Option<T>` is insufficient — `Option<Option<T>>` with `double_option::deserialize` is needed to distinguish clear from no-op.
Suggestion: Review docs/solutions/architecture-patterns/three-state-patch-double-option-2026-05-03.md before proceeding — one_sentence_lesson: "Plain Option<T> in a PATCH struct cannot distinguish 'set to null/clear' from 'field absent/unchanged' — use a three-state type like double_option so callers can explicitly clear optional fields."

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-06 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Known pattern: no-unreachable-in-production-match | 🔕 Superseded | — | — | — | Already fixed in prior phase (cf425a4) |
| 2 | Known pattern: serde-json-number-variant-before-f64-cast | ⏭️ Deferred | — | — | — | F046 — forward-looking Phase 5 guard; no Number iteration in Phase 4 |
| 3 | Known pattern: sqlite-begin-immediate-read-check-write | ⏭️ Deferred | — | — | — | F047 — Phase 5 adapter guidance; pure-core Phase 4 is correct |
| 4 | Known pattern: schema-version-column-at-creation | ⏭️ Deferred | — | — | — | F048 — Phase 5 migration note; no DB exists yet |
| 5 | Known pattern: three-state-patch-double-option | 🔕 Superseded | — | — | — | Already fixed in prior phase (cf425a4) |
