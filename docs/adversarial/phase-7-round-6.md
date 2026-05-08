# Adversarial Review — Phase 7 — Round 6

**Prior rounds:** R1 (F001–F010), R2 (F011–F019), R3 (F020–F026), R4 (F027–F031), R5 (F032–F034). All 34 findings unaddressed.

---

## Summary

Round 6 performed a final sweep of five targeted surfaces. Four of them either resolved to no concrete failure scenario within Phase 7's scope, overlapped with prior findings, or depend on Phase 9 design decisions not yet made. One genuine finding was constructed: `AppliedStateTag` wire strings are not pinned against refactors, making historical audit queries silently break if a variant is renamed.

---

## Findings

### F035 — `AppliedStateTag` serialised strings not pinned; downstream rename breaks all historical audit queries
**Severity:** LOW
**Category:** assumption-violation
**Slice:** 7.7

**Attack:** A developer renames `AppliedStateTag::TlsIssuing` to `AppliedStateTag::PendingTls` during a naming cleanup. `serde(rename_all = "kebab-case")` derives the wire string as `"pending-tls"`. All historical `audit_log.notes` rows containing `"applied_state":"tls-issuing"` now fail any query filtering `notes->>'applied_state' = 'tls-issuing'`. No compile-time signal fires. Audit viewer filters, Phase 9 alert rules, and LLM tool-gateway queries that inspect the notes column silently return zero results for pre-rename rows.

**Why the design doesn't handle it:** `AppliedStateTag` uses derived `#[serde(rename_all = "kebab-case")]` with no explicit per-variant `#[serde(rename = "...")]` and no vocabulary test analogous to the `AuditEvent`/`AUDIT_KIND_VOCAB` pattern already used in `audit.rs`. The project has solved this problem for the `kind` column but has not extended the pattern to the `notes` column.

**Blast radius:** Historical audit queries silently return zero results after any rename. Monitoring rules watching for `"tls-issuing"` states stop firing. The breakage is invisible until an operator queries historical data or investigates a silent alert.

**Recommended mitigation:** Add `#[serde(rename = "tls-issuing")]` (explicit, not derived) to each `AppliedStateTag` variant. Add a unit test — analogous to `display_strings_match_audit_vocab` in `audit.rs` — that maps each variant to its expected wire string, causing a test failure on any future rename before it reaches a deployed database.

---

## Surfaces with no finding

**`validate()` stub silently passes:** `apply_mutation()` already calls `validate::pre_conditions()` before any state is applied. The stub's impact depends on Phase 9 caller design, which is out of Phase 7's scope. No concrete failure scenario constructible within Phase 7.

**TOCTOU on snapshot load before `apply()`:** Overlaps with F004 and F007 (prior rounds). No new angle.

**Migration checksum mutation:** Universal risk across all migrations, not Phase 7-specific. Developer process concern, not a design flaw.

**`pub` fields on `CaddyApplier` bypassing constructor guards:** Overlaps with F029 (`instance_id` no validation). No independent finding.

---

## Severity summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 0 |
| LOW      | 1 (F035) |

---

## Round 6 verdict

**The design surface is exhausted.** Six rounds across 13 failure categories produced 35 findings. Round 6 found one LOW finding. No major untouched surfaces remain.

**Proceed to `--final`.** F035 is a one-line fix per variant plus a test — addressable during implementation, not a design blocker. The 34 prior findings should be formally documented in the decision doc with explicit acceptance rationale or mitigation commitments before Phase 7 implementation begins.

**Must-fix before implementation starts:** F034 (cross-instance rollback exploitable via LLM gateway), F033 (AuditWriter backpressure contract), F020 (rollback() lock invariant), F017 (F001+F004+F007 compound). These four represent the highest-impact cluster requiring coordinated design changes.
