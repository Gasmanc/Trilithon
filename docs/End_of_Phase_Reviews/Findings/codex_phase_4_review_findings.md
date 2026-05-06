# Phase 4 — Codex Review Findings

**Reviewer:** codex
**Date:** 2026-05-06
**Diff range:** 43f89ca..a948fa8
**Phase:** 4

---

[HIGH] IMPORT_VALIDATION_BYPASS
File: core/crates/core/src/mutation/validate.rs
Lines: 77-80
Description: `ImportFromCaddyfile` is unconditionally accepted with no precondition checks. This bypasses route/upstream invariants enforced for other variants (hostname validation, upstream reference existence, duplicate-ID checks), allowing invalid desired state to be committed.
Suggestion: Add a dedicated `check_import_from_caddyfile` path that validates parsed routes/upstreams before apply, including duplicate IDs (against state and within payload), hostname validity, and upstream references after merge.

[HIGH] IMPORT_CAN_SILENTLY_OVERWRITE_EXISTING_ENTITIES
File: core/crates/core/src/mutation/apply.rs
Lines: 318-325
Description: Import uses `BTreeMap::insert` for routes/upstreams without conflict handling, so existing entities with the same IDs are silently replaced. This can cause unintended destructive changes with no explicit mutation intent.
Suggestion: Reject ID collisions in validation (preferred), or make overwrite behavior explicit with a dedicated mode/flag and emit per-entity diff changes/audit details.

[WARNING] PER_VARIANT_SCHEMA_REF_TARGET_IS_INVALID
File: core/crates/core/src/bin/gen_mutation_schemas.rs
Lines: 75-78
Description: Per-variant stubs reference `Mutation.json#/definitions/Mutation`, but the generated root schema does not define `definitions.Mutation`; `Mutation` is the root schema. Generated variant files therefore contain broken `$ref` pointers.
Suggestion: Point stubs to `Mutation.json#` (or generate an actual `definitions.Mutation` node and reference that consistently).

[WARNING] DOCUMENTED_SCHEMA_CHECK_TARGET_DOES_NOT_EXIST
File: docs/schemas/mutations/README.md
Lines: 14-18
Description: Documentation instructs `just check-schemas`, but `justfile` has no `check-schemas` recipe. This breaks the documented drift-check workflow and makes schema drift easier to miss.
Suggestion: Add a `check-schemas` target in `justfile` (generate + `git diff --exit-code`) or update README to the real command.

[WARNING] MALFORMED_ENVELOPE_CAN_BE_MISCLASSIFIED_AS_MISSING_EXPECTED_VERSION
File: core/crates/core/src/mutation/envelope.rs
Lines: 59-67
Description: The parser checks `mutation_val.get("expected_version")` without first enforcing that `mutation` is an object. Non-object malformed payloads (e.g., string/array) can be reported as `MissingExpectedVersion` instead of `Malformed`, producing incorrect rejection classification.
Suggestion: Validate `mutation` is a JSON object first; only emit `MissingExpectedVersion` when the object exists but lacks the key.

[SUGGESTION] PROPERTY_TESTS_ONLY_EXERCISE_CREATEROUTE_PATH
File: core/crates/core/tests/mutation_props.rs
Lines: 50-170
Description: Current property tests generate only `CreateRoute` mutations, leaving patch semantics, policy mutations, import behavior, and conflict/capability edge cases uncovered by property-based testing.
Suggestion: Expand generators to cover all mutation variants (or a representative matrix), especially `Option<Option<T>>` patch behavior and import/capability/forbidden branches.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-06 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | IMPORT_VALIDATION_BYPASS | 🔕 Superseded | — | — | — | Same root as F001 (upgraded to UNANIMOUS) |
| 2 | IMPORT_CAN_SILENTLY_OVERWRITE_EXISTING_ENTITIES | 🔕 Superseded | — | — | — | Same root as F004 (upgraded to UNANIMOUS) |
| 3 | PER_VARIANT_SCHEMA_REF_TARGET_IS_INVALID | ✅ Fixed | `87e022a` | — | 2026-05-06 | F015 — $ref points to Mutation.json root |
| 4 | DOCUMENTED_SCHEMA_CHECK_TARGET_DOES_NOT_EXIST | ✅ Fixed | `21e330d` | — | 2026-05-06 | F038 — just check-schemas recipe added to Justfile |
| 5 | MALFORMED_ENVELOPE_CAN_BE_MISCLASSIFIED | ✅ Fixed | `21e330d` | — | 2026-05-06 | F039 — object check before expected_version lookup |
| 6 | PROPERTY_TESTS_ONLY_EXERCISE_CREATEROUTE_PATH | ✅ Fixed | `21e330d` | — | 2026-05-06 | F010 — proptest now covers CreateUpstream, SetGlobalConfig, SetTlsConfig |
