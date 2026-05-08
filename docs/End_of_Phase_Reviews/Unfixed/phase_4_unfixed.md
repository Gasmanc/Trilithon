---
id: duplicate:area::phase-4-unfixed-findings:legacy-uncategorized
category: duplicate
kind: process
location:
  area: phase-4-unfixed-findings
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

# Phase 4 — Unfixed Findings

**Run date:** 2026-05-06
**Total unfixed:** 12 (8 deferred · 4 won't fix · 0 conflicts pending)

| ID | Severity | Consensus | Title | File | Status | Reason |
|----|----------|-----------|-------|------|--------|--------|
| F002 | HIGH | SINGLE | DesiredState.policies is dead state — never populated | `crates/core/src/model/desired_state.rs` | deferred | Phase 5+ scope — policies field belongs to preset registry design |
| F003 | HIGH | SINGLE | PolicyAttachment and RoutePolicyAttachment are structural duplicates | `crates/core/src/mutation/types.rs` | deferred | Phase 5+ scope — policy model consolidation requires spec alignment |
| F019 | SUGGESTION | SINGLE | Outdated comment in build.rs references wrong path | `crates/core/build.rs` | won't fix | No path mismatch found in current code |
| F021 | SUGGESTION | SINGLE | capability.rs zd suppression comment format | `crates/core/src/mutation/capability.rs` | won't fix | Already in correct zd: format |
| F022 | SUGGESTION | SINGLE | gen_mutation_schemas binary lives in core crate — architectural concern | `crates/core/Cargo.toml` | deferred | Phase 5+ build restructure; binary move requires workspace changes |
| F025 | SUGGESTION | SINGLE | DesiredState::empty() is alias for Default::default() — unnecessary API surface | `crates/core/src/model/desired_state.rs` | deferred | 18+ call sites; empty() has semantic value; Claude assessment: low priority |
| F026 | SUGGESTION | SINGLE | AuditEvent has both derived Debug and manual Display — unclear canonical | `crates/core/src/audit.rs` | won't fix | clippy::mod_module_files prevents audit/mod.rs rename; both impls serve distinct purposes |
| F046 | WARNING | SINGLE | serde_json Number precision trap in DesiredState JSON canonicalization | general | deferred | Forward-looking Phase 5 guard; no Number iteration in Phase 4 code paths |
| F047 | WARNING | SINGLE | Optimistic concurrency TOCTOU gap — needs BEGIN IMMEDIATE for SQLite | general | deferred | Phase 5 adapter guidance; pure-core Phase 4 layer is correct |
| F048 | WARNING | SINGLE | Schema-version DB column should be in Phase 5 migration, not defaulted at read time | general | deferred | Forward-looking Phase 5 migration note; no DB exists yet |
| F058 | SUGGESTION | SINGLE | Caddyfile import warnings are discarded — not surfaced in MutationOutcome | `crates/core/src/mutation/apply.rs` | deferred | MutationOutcome schema changes belong in Phase 5 apply-layer wiring |
| F061 | SUGGESTION | SINGLE | audit.rs should be audit/mod.rs per spec | `crates/core/src/audit.rs` | won't fix | clippy::mod_module_files (workspace lint) forbids mod.rs naming |
