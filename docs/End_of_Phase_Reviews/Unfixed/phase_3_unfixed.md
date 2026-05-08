---
id: scope:area::phase-3-unfixed-findings:legacy-uncategorized
category: scope
kind: process
location:
  area: phase-3-unfixed-findings
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

# Phase 3 — Unfixed Findings

**Run date:** 2026-05-05
**Total unfixed:** 1 (1 deferred · 0 won't fix · 0 conflicts pending)

| ID | Severity | Consensus | Title | File | Status | Reason |
|----|----------|-----------|-------|------|--------|--------|
| F010 | SUGGESTION | SINGLE | Sentinel Raw JSON Map With String Literals | `core/crates/adapters/src/caddy/sentinel.rs` | deferred | Design-level change: define typed SentinelValue struct with Serialize/Deserialize. Deferred to Phase 6 sentinel redesign. |
