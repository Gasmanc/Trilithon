# Phase 8 — Learnings Match Review Findings

**Reviewer:** learnings_match
**Date:** 2026-05-11
**Diff range:** 4402d00..HEAD
**Phase:** 8

---

[WARNING] Known pattern: serde-json-number-variant-before-f64-cast
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/runtime-errors/serde-json-number-variant-before-f64-cast-2026-05-05.md
Suggestion: Review docs/solutions/runtime-errors/serde-json-number-variant-before-f64-cast-2026-05-05.md — one_sentence_lesson: "When canonicalising serde_json::Value numbers, call n.is_f64() before n.as_f64()"

[WARNING] Known pattern: audit-diff-before-from-original-state
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/runtime-errors/audit-diff-before-from-original-state-2026-05-06.md
Suggestion: Review — one_sentence_lesson: "Audit diff before-state must be captured from the original state before mutation"

[WARNING] Known pattern: recursion-depth-guard-for-external-data
File: general
Lines: general
Description: This diff may repeat a known pattern from docs/solutions/security-issues/recursion-depth-guard-for-external-data-2026-05-03.md
Suggestion: Review — one_sentence_lesson: "Any recursive traversal over operator-supplied structures needs an explicit depth limit"
