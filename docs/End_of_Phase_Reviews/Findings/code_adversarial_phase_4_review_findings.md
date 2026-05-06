# Phase 4 — Code Adversarial Review Findings

**Reviewer:** code_adversarial
**Date:** 2026-05-06
**Diff range:** 43f89ca..a948fa8
**Phase:** 4

---

[HIGH] IMPORT_CADDYFILE BYPASSES ALL PRE-CONDITION VALIDATION
File: /Users/carter/Coding/Trilithon/core/crates/core/src/mutation/validate.rs
Lines: 77-81
Description: `pre_conditions` returns `Ok(())` unconditionally for `ImportFromCaddyfile`. This means the import path skips every check that `CreateRoute` and `CreateUpstream` apply individually: duplicate route/upstream ID detection, upstream reference integrity (routes pointing to upstreams that don't exist yet in the batch), and RFC 1123 hostname validation. `apply_import_caddyfile` in `apply.rs` (lines 318-325) then calls `BTreeMap::insert` which silently overwrites any existing route or upstream with the same ID. A Caddyfile import can therefore clobber live routes without raising a `DuplicateRouteId` error, and can write routes whose hostnames contain invalid characters or whose upstream references point to IDs that are absent from both the existing state and the import batch.
Technique: Assumption Violation
Trigger: Any caller that passes a `ParsedCaddyfile` containing: (a) a route ID that already exists in `DesiredState`, (b) a route whose `upstreams` list contains an ID absent from both the existing state and the batch, or (c) a hostname that fails RFC 1123.
Suggestion: Add a dedicated `check_import_caddyfile` helper to `validate.rs` that (1) rejects duplicate IDs against current state (or adopts an explicit upsert/replace policy and documents it), (2) verifies that every upstream referenced by imported routes is either present in the existing state or present elsewhere in the same import batch, and (3) calls `check_hostnames_valid` on every imported route.

[HIGH] UPGRADE_POLICY DIFF RECORDS WRONG "BEFORE" VALUE
File: /Users/carter/Coding/Trilithon/core/crates/core/src/mutation/apply.rs
Lines: 182-196, 238-257
Description: `policy_attachment_preamble` takes `new_state` (the already-cloned mutable copy, line 54 of `apply.rs`) and reads `new_state.routes.get(route_id)` to build the `before` snapshot. For `apply_attach_policy`, `apply_detach_policy`, and `apply_upgrade_policy`, this is correct only because `new_state` has not yet been mutated at the time `preamble` runs. However, the function signature accepts `&DesiredState` with the name `new_state`, and the *intent* of the diff is to show the old value against the original `state`. The design is fragile: if any future caller mutates `new_state` before calling `preamble` (e.g., in a compound mutation phase), the `before` snapshot will silently reflect a post-mutation value, making the diff appear as a no-op. Compare with `apply_route_patch` (line 350) and `apply_upstream_patch` (line 395), which correctly pass `state` (the immutable original) for the `before` read — these two functions handle the pattern correctly; the policy helpers do not. There is also an inconsistency in `apply_set_global_config` and `apply_set_tls_config` (lines 264, 287), which also snapshot `before` from `new_state` rather than from `state`.
Technique: Assumption Violation
Trigger: Any future refactor that reorders operations within `apply_variant`, or any compound mutation that shares `new_state` across sub-operations, will silently record an incorrect `before` in the audit diff. Also affects the current `SetGlobalConfig` and `SetTlsConfig` variants.
Suggestion: Pass the original immutable `state: &DesiredState` into `policy_attachment_preamble`, `apply_set_global_config`, and `apply_set_tls_config` the same way `apply_route_patch` and `apply_upstream_patch` do, and read `before` from it. This makes the before/after contract uniform across all apply helpers and removes the fragile ordering dependency.

[HIGH] UPGRADE_POLICY APPLIES A NO-OP WHEN ATTACHMENT IS ABSENT AFTER VALIDATION PASSES
File: /Users/carter/Coding/Trilithon/core/crates/core/src/mutation/apply.rs
Lines: 238-257
Description: `apply_upgrade_policy` uses `if let Some(attachment) = route.policy_attachment.as_mut()` to perform the version mutation (line 248). If `policy_attachment` is `None`, the `if let` arm is silently skipped: `before` is `None`, `after` is also `None`, the function returns `Ok(vec![DiffChange { before: None, after: None }])`, and the version counter still increments by one. The pre-condition `check_upgrade_policy` (validate.rs line 215-222) does check that an attachment exists and would normally block this path. However, the two layers are structurally decoupled — `apply_variant` calls `apply_upgrade_policy` without any assumption that `pre_conditions` has already run, and `apply_mutation` does call them in order (validate then apply), but nothing in the type system enforces that sequence. If the order is ever reversed or `apply_upgrade_policy` is called directly (e.g., in a Phase 7 rollback path or a batch processor), a silent no-op mutation is emitted with a real version increment but zero state change, corrupting audit history.
Technique: Composition Failure
Trigger: Any path that calls `apply_upgrade_policy` without first having `check_upgrade_policy` run and succeed, or any case where `route.policy_attachment` is `None` despite validation claiming it is present.
Suggestion: Replace the `if let Some(attachment)` with `.ok_or_else(|| missing_policy_attachment_error(...))` and return `MutationError` rather than silently proceeding. This makes the apply layer self-defending regardless of whether validation ran first.

[WARNING] VERSION INCREMENT USES UNCHECKED ARITHMETIC ON i64
File: /Users/carter/Coding/Trilithon/core/crates/core/src/mutation/apply.rs
Lines: 55
Description: `new_state.version = state.version + 1;` uses the default `+` operator on `i64`. In Rust release builds, integer overflow wraps or panics depending on build flags, but the `core` crate has no overflow guards here. `DesiredState.version` is an `i64`; if a long-lived system increments it `i64::MAX` times, the next `+ 1` overflows. The conflict check on line 40 (`expected != state.version`) then becomes unreliable because the overflowed version may accidentally match a previous `expected_version` carried in an old in-flight mutation, allowing a stale mutation to be accepted as current.
Technique: Assumption Violation
Trigger: Extremely long-lived deployment with continuous mutations — `i64::MAX` is ~9.2 × 10^18, so this is a theoretical rather than near-term risk, but there is no detection or error path.
Suggestion: Use `state.version.checked_add(1).ok_or(MutationError::Forbidden { reason: ForbiddenReason::VersionOverflow })` and add `VersionOverflow` to `ForbiddenReason`. Costs one arithmetic check per mutation; pays off with a well-defined error instead of silent corruption.

[WARNING] SCHEMA-LEVEL VALIDATION GAP: `UpgradePolicy.to_version = 0` IS ACCEPTED
File: /Users/carter/Coding/Trilithon/core/crates/core/src/mutation/validate.rs
Lines: 201-249
Description: `check_upgrade_policy` validates that `to_version > attachment.preset_version` (line 244) and that the preset's registered version matches `to_version` (lines 232-242). Both checks together appear to prevent downgrade. However, the API accepts any `u32` for `to_version`, including `0`. If `AttachPolicy` is ever called with `preset_version: 0` (a valid `u32` with no lower-bound check), a route can carry `preset_version: 0`, and a subsequent `UpgradePolicy { to_version: 1 }` passes all checks. The issue is that `preset_version: 0` on `AttachPolicy` is never validated — there is no lower bound check anywhere. A `PresetVersion` record with `version: 0` in `DesiredState.presets` would anchor the whole preset lifecycle at version zero, which architecturally implies "no preset yet" rather than "preset at version 0".
Technique: Abuse Case
Trigger: A caller creates a `PresetVersion { version: 0, ... }` in `state.presets` and then calls `AttachPolicy { preset_version: 0 }`. Both pass all current validations. Version 0 is a sentinel-ish value with no formal definition in the schema.
Suggestion: Add a validation rule `PolicyPresetVersionZero` and reject `preset_version == 0` in `check_attach_policy`. Mirror it in `check_upgrade_policy`. Document in `PresetVersion` that versions start at 1.

[WARNING] STUB `$ref` IN PER-VARIANT SCHEMA FILES IS NOT RESOLVABLE
File: /Users/carter/Coding/Trilithon/core/crates/core/src/bin/gen_mutation_schemas.rs
Lines: 319-323
Description: Each per-variant stub file (e.g. `CreateRoute.json`) inserts `"$ref": "Mutation.json#/definitions/Mutation"`. JSON Schema resolvers use the `$ref` URI relative to the base URI of the document. When these files are served over HTTP or consumed by an editor's language server, the reference resolves correctly only if the resolver is configured with the schema directory as the root. The path `Mutation.json#/definitions/Mutation` is also incorrect: `schemars 0.8` emits definitions under `"$defs"` (draft 2020-12) or `"definitions"` (draft-07); the generator uses `"$schema": "http://json-schema.org/draft-07/schema#"` so `"definitions"` would be correct — but the actual generated root key depends on the schemars version and config. If the real key is `"$defs"`, the `$ref` path silently fails to resolve.
Technique: Assumption Violation
Trigger: Any tool or CI step that validates these schemas using a strict resolver will silently accept the stub as an empty schema (`{}` after failed `$ref` resolution) rather than rejecting the invalid reference.
Suggestion: After generating `Mutation.json`, inspect its top-level keys to determine whether the definition container is `"definitions"` or `"$defs"` and write the `$ref` accordingly. Add a post-generation smoke test that attempts to resolve the `$ref` and asserts the resulting schema is non-empty.

[WARNING] `DesiredState.version` IS PUBLIC AND MUTABLE — CALLER CAN CORRUPT THE CONCURRENCY ANCHOR
File: /Users/carter/Coding/Trilithon/core/crates/core/src/model/desired_state.rs
Lines: 413-415
Description: `DesiredState.version` is a `pub` field. `apply_mutation` assumes it can only ever advance by exactly `+1` through the controlled path in `apply.rs` (line 55). Any code that clones a `DesiredState` and then directly writes `state.version = N` bypasses the conflict check entirely. In Phase 4 this is test-only, but `DesiredState` is the primary aggregate and will be passed around in Phase 5+ snapshot persistence and Phase 7 rollback. A Phase 7 rollback implementation directly setting `new_state.version` to match the snapshot's `config_version` could bypass the `+1` invariant, causing the post-rollback state to have a version that does not advance monotonically.
Technique: Abuse Case
Trigger: Phase 7 rollback implementation directly setting `new_state.version` to match the snapshot's `config_version`, bypassing the `+1` invariant that `apply_mutation` enforces.
Suggestion: Consider making `version` a private field or exposing it only through a controlled mutator. At minimum, `apply_mutation` should assert that `new_state.version == state.version + 1` as a post-condition before returning `Ok`.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-06 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | ImportFromCaddyfile bypasses all pre-condition validation | ✅ Fixed | `7d90fc1` | — | 2026-05-06 | F001 — added check_import_caddyfile |
| 2 | UpgradePolicy diff records wrong "before" value | ✅ Fixed | `7d90fc1` | — | 2026-05-06 | F031 — pass original state for before-snapshot |
| 3 | UpgradePolicy applies a no-op when attachment is absent after validation | ✅ Fixed | `7d90fc1` | — | 2026-05-06 | F032 — apply layer now self-defending via ok_or_else |
| 4 | Version increment uses unchecked arithmetic on i64 | ✅ Fixed | `21e330d` | — | 2026-05-06 | F036 — checked_add with VersionOverflow error |
| 5 | Schema-level validation gap: UpgradePolicy to_version = 0 accepted | ✅ Fixed | `21e330d` | — | 2026-05-06 | F037 — PolicyPresetVersionZero rule added |
| 6 | Stub $ref not resolvable | ✅ Fixed | `87e022a` | — | 2026-05-06 | F015 — $ref now points to Mutation.json (root, no fragment) |
| 7 | DesiredState.version is public and mutable | ✅ Fixed | `3787298` | — | 2026-05-06 | F035 — debug_assert post-condition added in apply_mutation |
