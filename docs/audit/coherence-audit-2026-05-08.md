---
audit_date: 2026-05-08
verdict: DEGRADED
n_pass: 15
n_warn: 4
n_fail: 0
strict_mode: false
invariant_set_hash: 29058f87d02dd16b5ec4fa4c3fd6b120cb4d8c67bb86de6f7c2880a8f410d621
main_sha: fe49ad36dffddf2b5f6ca30ac3c74abc6a98bbe6
---

# Coherence Audit — 2026-05-08

## Verdict: DEGRADED

First-run baseline audit. All structural invariants pass; 4 warnings are tracking-infrastructure gaps expected on initial adoption (cache files not yet populated, legacy migration dates).

## Passing (15)

- ✓ #1  contracts-md-fresh — contracts.md (fe49ad3, ts:1778237038) regenerated after contract-roots.toml (8d7edfc, ts:1778231522); 0 contracts declared, empty registry is the correct baseline
- ✓ #2  registry-verify-clean — 0 `// contract:` markers in source, `[roots]` empty; fresh extract would also produce 0 contracts — registry is in sync
- ✓ #3  cross-cutting-phases-have-registry-diff — no cross-cutting phases completed since adoption baseline 0a795583 — check vacuously passes
- ✓ #4  all-findings-have-valid-ids — 56/56 finding files carry Foundation 0 frontmatter (id, status, finding_kind); 0 invalid
- ✓ #5  dedup-working — 0 duplicate IDs across 56 finding files
- ✓ #7  accepted-as-is-discipline — 0 accepted-as-is findings; check vacuously passes
- ✓ #8  pending-revalidation-not-stale — 0 findings with status pending-revalidation
- ✓ #9  every-seam-has-test-with-asserts — seams.md has `seams: []`; no active seams to check
- ✓ #10 cross-cutting-phases-touched-seam-tests — no cross-cutting phases since adoption baseline; check vacuously passes
- ✓ #11 cross-phase-tests-pass-on-main — no tests/cross_phase/ directory; no tests to fail
- ✓ #12 zd-suppressions-not-expired — 11 active suppressions, all expire in future (earliest: 2026-08-01 for zd:phase-01)
- ✓ #13 suppressions-not-extended-without-reason — no suppression bump commits found; all 11 suppressions are first-use entries
- ✓ #16 project-audit-within-cadence — last project audit 7 days ago (2026-05-01-A3), 0 phases completed since; within 30-day / 5-phase thresholds
- ✓ #18 every-merge-has-merge-review — 0 merges on main since adoption baseline 0a795583
- ✓ #19 invariant-set-stable-or-explained — first coherence audit run; no prior invariant set hash to compare against

## Warnings (4)

- ⚠ #6  mean-age-under-threshold — 48/48 open findings have `created_at: migration`; mean age uncomputable from legacy-migration timestamp; assign real ISO dates when creating new findings
   Recommendation: new findings going forward will have real dates; this warn will auto-clear as legacy findings are resolved or re-dated

- ⚠ #14 advisory-checks-actioned — `.claude/cache/advisory-history.json` absent; /where advisory tracking not yet recording; will activate once `/where` emits its first advisory finding
   Recommendation: run `/where` once to seed the advisory cache

- ⚠ #15 just-check-runs-vs-merges — `.claude/cache/just-check-runs.json` absent; gate-run hook not yet recording; will activate once justfile hook writes first entry
   Recommendation: the SubagentStop hook is installed; records will appear after first `just check` run through a phase

- ⚠ #17 audit-cost-not-runaway — `.claude/cache/audit-runs.json` absent; token cost growth tracking not yet active; will activate once this audit's entry seeds the file
   Recommendation: this audit will write the seed entry — warn clears on next audit

## Failing (0)

_No failing invariants._

## Trend (first run — no prior baseline)

- This is the first coherence audit on this project. No trend deltas available.
- Open findings: 48 (48 from legacy migration, 0 organic)
- Active seams: 0
- Contract roots: 0
- Suppressions: 11 (all valid)

## Recommended actions (ranked)

1. Seed advisory history — run `/where` once to create `.claude/cache/advisory-history.json` (#14, warn)
2. Add contract roots — populate `docs/architecture/contract-roots.toml` with actual public API symbols (e.g. `Storage` trait, core error types) so `#1` and `#2` track real contracts, not an empty registry
3. Enumerate seams — add at least one entry to `docs/architecture/seams.md` as phase 6 (audit-log) work lands; the Storage↔AuditLog boundary is a natural first seam
4. Audit token tracking — write `.claude/cache/audit-runs.json` initial entry (done at end of this run, #17 will clear next audit)

## Notes on DEGRADED verdict

All 4 warns are tracking-infrastructure gaps, not structural failures. This is the expected and correct state for a first-run adoption audit. The verdict will upgrade to WORKING once:
- The three cache files are seeded (advisory-history, just-check-runs, audit-runs)
- At least a few new findings carry real `created_at` dates
