---
id: duplicate:area::phase-4-scope-guardian-review-findings:legacy-uncategorized
category: duplicate
kind: process
location:
  area: phase-4-scope-guardian-review-findings
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

# Phase 4 — Scope Guardian Review Findings

**Reviewer:** scope_guardian
**Date:** 2026-05-06
**Diff range:** 43f89ca..a948fa8
**Phase:** 4

---

[CRITICAL] MISSING schema_drift INTEGRATION TEST
File: core/crates/core/tests/
Lines: general
Description: Slice 4.10 explicitly requires `core/crates/core/tests/schema_drift.rs` with a `schemas_match_committed` test that runs `gen_mutation_schemas` and asserts `git status --porcelain docs/schemas/mutations/` is empty. The file does not exist. Only `mutation_props.rs` is present. Without this test, schema drift goes undetected in CI.
Question: Scope
TODO unit: 4.10 — Property tests, schema generation, mutation README
Suggestion: Create `core/crates/core/tests/schema_drift.rs` with a `#[test] fn schemas_match_committed()` that invokes the `gen_mutation_schemas` binary via `std::process::Command` and asserts the exit code is 0 and `git diff --exit-code docs/schemas/mutations/` reports no changes.

[HIGH] content_address UTILITY OUT OF SCOPE
File: core/crates/core/src/mutation/types.rs
Lines: 224-248
Description: `content_address(canonical_json_bytes: &[u8]) -> String` and a corresponding `sha2` dependency are not mentioned in any slice of Phase 4. The function computes a SHA-256 content address over canonical JSON bytes — that operation belongs to the snapshot writer in Phase 5 (ADR-0009, §6.5). Placing it here pre-empts Phase 5's design and bleeds snapshot-layer concerns into the mutation-type file.
Question: Scope
TODO unit: 4.6 — `Mutation` enum, `MutationId`, `expected_version` envelope
Suggestion: Remove `content_address` and its `sha2` dependency from `mutation/types.rs`. Move it to `core/crates/core/src/snapshot.rs` (or equivalent) when Phase 5 is implemented.

[HIGH] build.rs MISSING rerun-if-changed DIRECTIVES
File: core/crates/core/build.rs
Lines: 6-20
Description: The TODO (Slice 4.10) specifies two `cargo:rerun-if-changed` directives — `src/model` and `src/mutation` — so that the build script re-triggers when model or mutation files change. The implementation emits only `cargo:rerun-if-changed=build.rs`. Without the model/mutation directives, the `TRILITHON_SCHEMA_DIR` env var will not be refreshed when those source trees change, causing stale schemas in incremental builds.
Question: Coherence
TODO unit: 4.10 — Property tests, schema generation, mutation README
Suggestion: Add `println!("cargo:rerun-if-changed=src/model");` and `println!("cargo:rerun-if-changed=src/mutation");` to `build.rs`.

[WARNING] audit_vocab NOT EXPORTED — TEST COVERAGE ASSERTION WEAKENED
File: core/crates/core/src/audit.rs
Lines: 163-207
Description: The TODO (Slice 4.7) specifies that `display_strings_match_six_six_vocab` imports the vocabulary list from `core::storage::audit_vocab`. The implementation instead embeds a private `const VOCAB` array inside `#[cfg(test)]`. The coverage assertion ("every string in `audit_vocab` MUST appear at least once") becomes untestable from other modules, and Phase 6 has no shared constant to reference when writing audit rows.
Question: Coherence
TODO unit: 4.7 — `MutationOutcome`, `MutationError`, `Diff`, `AuditEvent` integration
Suggestion: Extract `VOCAB` as a public `const AUDIT_KIND_VOCAB: &[&str]` in `core::storage` (or `core::audit`) so Phase 6 can import it and the test can reference it by path rather than embedding a private copy.

[WARNING] EXTRA ValidationRule VARIANTS NOT IN SPEC
File: core/crates/core/src/mutation/error.rs
Lines: 59-76
Description: The TODO (Slice 4.7) lists exactly five `ValidationRule` variants: `HostnameInvalid`, `UpstreamReferenceMissing`, `PolicyPresetMissing`, `DuplicateRouteId`, `PolicyAttachmentMissing`. The diff adds three more: `DuplicateUpstreamId`, `RouteMissing`, `UpstreamStillReferenced`. These are needed by the slice 4.9 pre-condition logic, but they diverge from the spec without a recorded rationale.
Question: Scope
TODO unit: 4.7 — `MutationOutcome`, `MutationError`, `Diff`, `AuditEvent` integration
Suggestion: Either add these three variants to the phase spec/TODO as an amendment, or document in code comments why they extend the spec.

[SUGGESTION] audit.rs IS A FLAT FILE, NOT A DIRECTORY MODULE
File: core/crates/core/src/audit.rs
Lines: general
Description: The TODO (Slice 4.7) specifies `core/crates/core/src/audit/mod.rs` as the file path. The diff creates `audit.rs` instead. Both compile identically, but it differs from the spec and from the pattern established by `mutation/mod.rs`. If Phase 6 needs to add submodules (e.g. `audit/writer.rs`), refactoring will be required.
Question: Coherence
TODO unit: 4.7 — `AuditEvent` in `core/crates/core/src/audit/mod.rs`
Suggestion: Rename `src/audit.rs` → `src/audit/mod.rs` now to match the spec and avoid a future refactor.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-06 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Missing schema_drift integration test | ✅ Fixed | `d826850` | — | 2026-05-06 | F030 — created crates/core/tests/schema_drift.rs |
| 2 | content_address utility out of scope | ✅ Fixed | `de47342` | — | 2026-05-06 | F034 — moved to canonical_json.rs |
| 3 | build.rs missing rerun-if-changed directives | ✅ Fixed | `87e022a` | — | 2026-05-06 | F012 — added src/model and src/mutation directives |
| 4 | audit_vocab not exported — test coverage assertion weakened | ✅ Fixed | `21e330d` | — | 2026-05-06 | F044 — AUDIT_KIND_VOCAB now pub const in core::audit |
| 5 | Extra ValidationRule variants not in spec | ✅ Fixed | `6e70eca` | — | 2026-05-06 | F062 — doc comments reference slice that required each variant |
| 6 | audit.rs is a flat file, not a directory module | 🚫 Won't Fix | — | — | — | F061 — clippy::mod_module_files (workspace lint) forbids mod.rs |
