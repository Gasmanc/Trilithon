---
id: duplicate:area::phase-4-findings:legacy-uncategorized
category: duplicate
kind: process
location:
  area: phase-4-findings
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

# Phase 4 Findings

## Slice 4.1
**Status:** complete
**Date:** 2026-05-03
**Commit:** b5952401842961acd277b13918ab0ead33a23d44
**Summary:** Implemented identifier newtypes (RouteId, UpstreamId, PolicyId, PresetId, MutationId) and primitive value types (UnixSeconds, JsonPointer, CaddyModule) in the core model. All types are ULID-bearing with full serde and Hash support. RFC 6901 JSON Pointer escaping implemented correctly.

### Simplify Findings
skipped [trivial]

### Items Fixed Inline
none

### Items Left Unfixed
none

## Slice 4.2
**Status:** complete
**Date:** 2026-05-03
**Commit:** 5df0601
**Summary:** Implemented Route, HostPattern with RFC 952/1123 hostname validator, Upstream/UpstreamDestination/UpstreamProbe, MatcherSet and all matcher types, HeaderRules/HeaderOp, and RedirectRule. All eight named tests pass. Fixed two gate failures during implementation: (1) `expect()` calls in tests replaced with `?` propagation; (2) nested wildcard `*.*.example.com` now correctly returns `InvalidWildcard` instead of `InvalidCharacter` by checking the remainder for `*` before label validation.

### Simplify Findings
- `validate_hostname`: the `TotalTooLong` check is duplicated in both `validate_hostname` and `validate_labels`; the one in `validate_labels` is only needed for the wildcard path (the non-wildcard path checks before calling). Minor duplication, not worth extracting.

### Items Fixed Inline
- Replaced `expect()` in test functions with `-> Result<(), Box<dyn std::error::Error>>` + `?` to satisfy the project's `disallowed_methods` clippy lint.
- Fixed `reject_double_wildcard` test: added early `*`-check on the wildcard suffix before calling `validate_labels` so the correct `InvalidWildcard` error is returned.

### Items Left Unfixed
none

## Slice 4.3
**Status:** complete
**Date:** 2026-05-03
**Commit:** 7217d9a
**Summary:** Added TlsConfig/TlsConfigPatch, GlobalConfig/GlobalConfigPatch, PolicyAttachment, and PresetVersion to the core model. Introduced a double_option serde helper in primitive.rs to correctly round-trip Option<Option<T>> (absent/clear/set states), working around serde's default behavior of mapping null to outer None.

### Simplify Findings
- Removed unnecessary re-export of `double_option` from model.rs top-level; the helper is used only via `crate::model::primitive::double_option` internally and exposing it as a public model re-export would leak an implementation detail.

### Items Fixed Inline
- Removed `double_option` re-export from model.rs to avoid leaking the internal serde helper.

### Items Left Unfixed
none

## Slice 4.4
**Status:** complete
**Date:** 2026-05-03
**Commit:** 545c795
**Summary:** Defined `DesiredState` as the aggregate over all domain model types using `BTreeMap` for deterministic key ordering. Added serde round-trip test covering one route, two upstreams, one preset, and one policy. Added `btreemap_iteration_is_sorted` test confirming ordered iteration when keys are inserted in reverse order.

### Simplify Findings
- Test helpers used inline string literals matching the key IDs — acceptable for test-only code.
- No redundant abstractions added; `empty()` constructor delegates to `Default`.

### Items Fixed Inline
- Removed redundant `.clone()` calls on last-use bindings (`route_id`, `up1_id`, `up2_id`, last use of `preset_id`) flagged by `clippy::redundant_clone`.

### Items Left Unfixed
none

## Slice 4.5
**Status:** complete
**Date:** 2026-05-03
**Commit:** a52621e
**Summary:** Implemented RoutePatch, UpstreamPatch, and ParsedCaddyfile placeholder types following the Option<Option<T>> convention for nullable patch fields. All three named tests pass: route_patch_serde_round_trip, route_patch_default_is_all_none, upstream_patch_round_trip. Module structure created with mutation/mod.rs re-exports and lib.rs updated with pub mod mutation.

### Simplify Findings
skipped [trivial]

### Items Fixed Inline
- Added `#[allow(clippy::option_option)]` to RoutePatch and UpstreamPatch struct definitions per CLAUDE.md rule for tracked suppression of intentional Option<Option<T>> usage.
- Converted test functions from panic-using `.expect()` to idiomatic Result<(), Box<dyn std::error::Error>> + `?` to satisfy disallowed_methods linter.
- Added `#![allow(clippy::mod_module_files)]` to mutation/mod.rs to maintain submodule file structure despite clippy's preference for single files.

### Items Left Unfixed
none

## Slice 4.6
**Status:** complete
**Date:** 2026-05-03
**Summary:** Implemented the closed `Mutation` enum with 13 variants (each carrying `expected_version`), `MutationKind` discriminant enum, and `MutationEnvelope` with `parse_envelope` that rejects missing `expected_version`. The `expected_version()` and `kind()` methods were promoted to `const fn` per the `missing_const_for_fn` clippy lint. All 6 tests pass and the gate is clean.
### Simplify Findings
- Match arms in `expected_version()` and `kind()` within `impl Mutation` were using `use Mutation::*` (wildcard import), which clippy rejects (`enum_glob_use`). Fixed inline by using fully-qualified `Self::` paths.
- Both accessor methods were eligible for `const fn` promotion per `missing_const_for_fn` lint. Promoted inline.
- `parse_envelope` was missing a `# Errors` rustdoc section. Added inline.
- Test modules were using `Default::default()` instead of type-qualified `T::default()`. Fixed inline (`default_trait_access` lint).
- Test modules were using `.expect()` which `expect_used` denies. Fixed inline with `#[allow(clippy::expect_used, ...)]` on the test module (matching the pattern used across the codebase).
### Items Fixed Inline
- Wildcard enum import replaced with `Self::` qualified paths in `impl Mutation`
- `fn expected_version()` and `fn kind()` promoted to `const fn`
- Added `# Errors` section to `parse_envelope` doc comment
- `Default::default()` → `MatcherSet::default()` / `HeaderRules::default()` in test helper
- Added `#[allow(clippy::expect_used, clippy::unwrap_used, ...)]` to both test modules
### Items Left Unfixed
none

## Slice 4.7
**Status:** complete
**Date:** 2026-05-03
**Summary:** Implemented `MutationError`, `MutationOutcome`, `Diff`, `DiffChange`, and `AuditEvent` with full `Display` coverage per architecture §6.6. All 41 `AuditEvent` variants map to unique wire kind strings verified by two tests. Gate passed after fixing four clippy violations introduced during initial implementation.
### Simplify Findings
- `mod.rs` not allowed by project clippy config — `audit/mod.rs` correctly moved to `audit.rs` (inline fix).
- Wildcard import `use AuditEvent::*` in `Display` impl flagged — replaced with `Self::` prefixes (inline fix).
- `serde_json::json!()` macro expands to `.unwrap()` calls which are disallowed — replaced with explicit `serde_json::Value` constructors (inline fix).
- Single-character string `"9"` used as pattern — replaced with char literal `'9'` (inline fix).
### Items Fixed Inline
- Moved `audit/mod.rs` → `audit.rs` to satisfy `clippy::mod_module_files` project rule.
- Replaced `use AuditEvent::*` with `Self::` prefixes in `Display` impl.
- Replaced `serde_json::json!(0)` / `serde_json::json!(1)` with `serde_json::Value::Number(...)` in tests.
- Changed `s.contains("9")` to `s.contains('9')` in `conflict_error_display_contains_versions` test.
### Items Left Unfixed
none

## Slice 4.8
**Status:** complete
**Date:** 2026-05-03
**Summary:** Implemented `Mutation::referenced_caddy_modules()` per variant and `check_capabilities()` in `core/crates/core/src/mutation/capability.rs`. All six acceptance tests pass. Three clippy violations were fixed inline during gate runs (match_same_arms, option_if_let_else, disallowed_methods for unwrap).
### Simplify Findings
- `Self::DetachPolicy` and `Self::SetGlobalConfig` arms returned identical `BTreeSet::new()` bodies — merged into a single arm (inline fix).
- `match first_missing { ... }` pattern flagged by `option_if_let_else` — replaced with `map_or` combinator (inline fix).
- `unwrap()` on the first missing module element is a disallowed method in production — replaced with iterator `.find()` + `map_or` which avoids the unreachable panic path entirely (inline fix).
### Items Fixed Inline
- Merged `DetachPolicy` + `SetGlobalConfig` arms to satisfy `clippy::match_same_arms`.
- Replaced `match first_missing` with `first_missing.map_or(...)` to satisfy `clippy::option_if_let_else`.
- Replaced `missing.iter().next().unwrap()` with a single `.find()` call to satisfy `clippy::disallowed_methods`.
### Items Left Unfixed
none

## Slice 4.9
**Status:** complete
**Date:** 2026-05-03
**Summary:** Implemented `apply_mutation` as a pure function over `&DesiredState`, `&Mutation`, `&CapabilitySet` in `apply.rs`, and `pre_conditions` with per-variant helpers in `validate.rs`. All 12 acceptance tests pass (10 spec + 2 additional integration tests). Gate passed after two fix rounds — first for rustfmt, then for 23 clippy violations addressed by refactoring both files into per-variant helper functions.

### Simplify Findings
- `apply_mutation` and `pre_conditions` both exceeded the 100-line function limit — extracted per-variant handler functions in both files to comply.
- `apply_create_route`, `apply_delete_route`, `apply_create_upstream`, `apply_delete_upstream`, `apply_set_global_config`, `apply_set_tls_config`, `apply_import_caddyfile` were initially `Result<Vec<DiffChange>, MutationError>` but are infallible — changed to return `Vec<DiffChange>` directly.
- `patch` and `path` parameter names in `apply_route_patch` / `apply_upstream_patch` triggered `clippy::similar_names` — renamed to `route_patch` / `upstream_patch` and `pointer`.
- Unnecessary `state_with_upstream` helper in tests (used by zero tests) — removed.
- `audit_event_for` could be `const fn` — marked as `const fn`.
- `format!("hostname '{}' ...", raw)` → `format!("hostname '{raw}' ...")` for inline variable in format string.
- Redundant `.clone()` on `preset_id` in test assertions — removed.

### Items Fixed Inline
- Extracted `apply_create_route`, `apply_delete_route`, `apply_create_upstream`, `apply_delete_upstream`, `apply_set_global_config`, `apply_set_tls_config`, `apply_import_caddyfile` into dedicated helpers to bring `apply_variant` under 100 lines.
- Extracted `check_route_id_unused`, `check_route_exists`, `check_upstreams_exist`, `check_hostnames_valid`, `check_update_route`, `check_attach_policy`, `check_detach_policy`, `check_upgrade_policy` from `pre_conditions` to bring it under 100 lines.
- Changed infallible helpers to return `Vec<DiffChange>` instead of `Result<Vec<DiffChange>, MutationError>`.
- Renamed `patch`/`path` params to `route_patch`/`pointer` to fix `similar_names` lint.
- Removed unused `state_with_upstream` test helper.
- Marked `audit_event_for` as `const fn`.
- Used format string variable interpolation `'{raw}'` style.
- Removed redundant test `.clone()` calls.

### Items Left Unfixed
none

## Slice 4.10
**Status:** complete
**Date:** 2026-05-03
**Commit:** 7ab795b
**Summary:** Added `proptest` and `schemars` to `trilithon-core`, derived `JsonSchema` on all 20+ mutation-path types, implemented a `gen_mutation_schemas` binary that writes 14 schema files, added three property tests (determinism, commutativity, version postcondition), and wired a `check-schemas` drift-detection recipe into the `just check` gate.

### Simplify Findings
- The `serde_json::json!{}` macro was triggering `clippy::disallowed_methods` on internal `unwrap()` calls inside the macro expansion. Replaced with explicit `BTreeMap<&str, Value>` construction to keep the binary lint-clean without suppression.
- `arb_desired_state` helper was unused after the proptest refactor — removed.
- The pre-commit `end-of-file-fixer` hook added missing trailing newlines to the 14 generated JSON schema files; re-staged and committed cleanly.

### Items Fixed Inline
- Replaced `serde_json::json!{}` with manual `BTreeMap` in `gen_mutation_schemas.rs` to avoid `clippy::disallowed_methods` on macro-internal `unwrap`.
- Removed dead `arb_desired_state` helper function from `mutation_props.rs`.
- Used `std::env::var_os` in `build.rs` to avoid `expect()` (which the workspace `deny(expect_used)` lint would reject).

### Items Left Unfixed
none

## gemini Review
**Date:** 2026-05-03

[HIGH] Hostname length limit bypass for wildcards
File: crates/core/src/model/route.rs
Lines: 104-118
Description: `validate_hostname` bypasses the 253-character total length limit for wildcard patterns because it returns early if the `*.` prefix is found. `validate_labels` only checks if the remainder (`rest`) is <= 253 characters. A wildcard pattern like `*.` followed by 252 characters would be 254 characters total but would be accepted, violating RFC 1035/1123.
Suggestion: Check `s.len() > 253` at the beginning of `validate_hostname`, before stripping the wildcard prefix.

[WARNING] Incorrect ValidationRule for UpstreamId conflict
File: crates/core/src/mutation/validate.rs
Lines: 33-35
Description: In `pre_conditions` for `CreateUpstream`, if the upstream ID already exists, it returns `ValidationRule::DuplicateRouteId`. This is a copy-paste error and should use a rule specific to Upstreams or a more generic "DuplicateId" rule.
Suggestion: Update the rule name or add `DuplicateUpstreamId` to the `ValidationRule` enum to avoid confusion in API error responses.

[WARNING] Missing validation for ImportFromCaddyfile
File: crates/core/src/mutation/validate.rs
Lines: 64
Description: `Mutation::ImportFromCaddyfile` is accepted in `pre_conditions` without any validation. This allows importing routes with invalid hostnames, duplicate IDs, or references to non-existent upstreams, which violates core domain invariants and can lead to an inconsistent `DesiredState`.
Suggestion: Implement validation for `ParsedCaddyfile` in `pre_conditions`, ensuring all contained routes and upstreams are valid and consistent with the current state.

[WARNING] Inefficient and non-granular diff for ImportFromCaddyfile
File: crates/core/src/mutation/apply.rs
Lines: 285-312
Description: `apply_import_caddyfile` generates diffs by serializing the entire `routes` and `upstreams` maps both before and after the operation. For large configurations, this is extremely inefficient and produces non-granular diffs.
Suggestion: Generate granular `DiffChange` entries for each route and upstream that is actually added or modified by the import.

[SUGGESTION] Non-specific error path in check_upstreams_exist
File: crates/core/src/mutation/validate.rs
Lines: 91-103
Description: `check_upstreams_exist` returns a path pointing to the `upstreams` collection but does not include the index of the offending upstream ID.
Suggestion: Update the function to include the index in the `JsonPointer` path.

[SUGGESTION] Misleading error for empty labels
File: crates/core/src/model/route.rs
Lines: 136-140
Description: If a hostname contains an empty label (e.g., `example..com`), `validate_labels` returns `HostnameError::HyphenBoundary`. While technically correct, it is misleading.
Suggestion: Add a specific check for `label.is_empty()` and return a more descriptive error.

## codex Review
**Date:** 2026-05-03

[CRITICAL] DELETE_UPSTREAM_CAN_BREAK_REFERENTIAL_INTEGRITY
File: crates/core/src/mutation/validate.rs
Lines: 44-53
Description: `DeleteUpstream` only checks that the upstream exists, not whether any route still references it. `apply_delete_upstream` then removes it unconditionally, allowing `DesiredState.routes[*].upstreams` to contain dangling IDs.
Suggestion: Before allowing `DeleteUpstream`, scan routes for references to the target upstream and reject when referenced (or perform coordinated route rewrites).

[HIGH] OPTION_OPTION_PATCH_CLEAR_SEMANTICS_BROKEN
File: crates/core/src/mutation/patches.rs
Lines: 46-50, 80
Description: `RoutePatch.redirects`, `RoutePatch.policy_attachment`, and `UpstreamPatch.max_request_bytes` are `Option<Option<T>>` but lack the `double_option` deserializer. With default serde behavior, JSON `null` is conflated with field absence, so "clear" operations cannot be expressed reliably.
Suggestion: Add `deserialize_with = "double_option::deserialize"` on all `Option<Option<T>>` patch fields and add null-vs-absent tests.

[HIGH] UPDATE_ROUTE_DOES_NOT_VALIDATE_PATCHED_HOSTNAMES
File: crates/core/src/mutation/validate.rs
Lines: 140-150
Description: `check_update_route` validates upstream references but never validates `patch.hostnames`. Invalid RFC 952/1123 hostnames can be introduced via `UpdateRoute`.
Suggestion: If `patch.hostnames` is present, run `check_hostnames_valid` on the patched values.

[HIGH] UPGRADE_POLICY_ACCEPTS_NONEXISTENT_TARGET_VERSION
File: crates/core/src/mutation/validate.rs
Lines: 205-232
Description: `check_upgrade_policy` only enforces `to_version > current_version`; it never verifies that the preset/version exists in `state.presets`.
Suggestion: Resolve the attached preset in `state.presets` and require the requested target version to exist/be valid before applying.

[HIGH] IMPORT_FROM_CADDYFILE_BYPASSES_STATE_INVARIANT_VALIDATION
File: crates/core/src/mutation/validate.rs
Lines: 75-78
Description: `ImportFromCaddyfile` has no precondition validation. Crafted payloads can introduce invalid hostnames or dangling references.
Suggestion: Validate imported routes/upstreams with the same checks used by create/update flows.

[WARNING] PER_VARIANT_SCHEMA_STUB_REFS_ARE_INVALID
File: crates/core/src/bin/gen_mutation_schemas.rs
Lines: 74-78
Description: Stub schemas reference `Mutation.json#/definitions/Mutation`, but `Mutation.json` does not define `definitions.Mutation` (the union is top-level).
Suggestion: Point `$ref` to a valid target.

[WARNING] VALIDATION_RULES_ARE_MISCLASSIFIED
File: crates/core/src/mutation/validate.rs
Lines: 34-38, 95-101
Description: Duplicate upstream ID errors are emitted as `DuplicateRouteId`, and missing route errors are emitted as `PolicyAttachmentMissing`.
Suggestion: Add/use dedicated rule variants (e.g., `DuplicateUpstreamId`, `RouteMissing`).

## qwen Review
**Date:** 2026-05-03

[WARNING] Wrong ValidationRule for duplicate upstream id
File: crates/core/src/mutation/validate.rs
Lines: 35-36
Description: `CreateUpstream` conflict uses `ValidationRule::DuplicateRouteId` but the entity is an upstream, not a route.
Suggestion: Add a `DuplicateUpstreamId` variant to `ValidationRule`.

[WARNING] Wrong ValidationRule for missing route
File: crates/core/src/mutation/validate.rs
Lines: 97-99
Description: `check_route_exists` uses `ValidationRule::PolicyAttachmentMissing` when the actual failure is "route not found".
Suggestion: Add a `RouteNotFound` or `RouteReferenceMissing` variant to `ValidationRule`.

[WARNING] unreachable!() in production code violates convention
File: crates/core/src/mutation/apply.rs
Lines: 109
Description: `unreachable!()` is used in the `Rollback` arm of `apply_variant`. The project convention forbids panics in production code.
Suggestion: Replace with `return Err(MutationError::Forbidden { reason: ForbiddenReason::RollbackTargetUnknown })`.

[WARNING] Incorrect patch application for Option<Option<T>> fields in TLS
File: crates/core/src/mutation/apply.rs
Lines: 305-313
Description: `apply_set_tls_config` may not fully implement three-state semantics for Option<Option<T>> fields.
Suggestion: Verify the double_option deserialization path is used consistently.

[SUGGESTION] Misleading test name — "idempotency" vs determinism
File: crates/core/tests/mutation_props.rs
Lines: 72-98
Description: The test `idempotency_on_mutation_id` verifies determinism, not idempotency.
Suggestion: Rename to `apply_mutation_is_deterministic`.

[SUGGESTION] Outdated comment in build.rs
File: crates/core/build.rs
Lines: 8
Description: Comment references wrong path for CARGO_MANIFEST_DIR.
Suggestion: Update comment to match actual path.

[SUGGESTION] Overly broad #[allow(clippy::option_option)] on patches structs
File: crates/core/src/mutation/patches.rs
Lines: 26, 64
Description: The allow is struct-level but only a subset of fields use Option<Option<T>>.
Suggestion: Move allow to only the double-Option fields.

[SUGGESTION] Suppression comment missing standard format fields
File: crates/core/src/mutation/capability.rs
Lines: 103
Description: The `zd:CAP-PRESET` comment doesn't follow CLAUDE.md format exactly.
Suggestion: Align to `// zd:CAP-PRESET expires:2026-12-31 reason:phase-18 preset module derivation`.

## glm Review
**Date:** 2026-05-03

[WARNING] `unreachable!` in production code path
File: core/crates/core/src/mutation/apply.rs
Lines: 109
Description: `unreachable!()` is used in the `Mutation::Rollback` arm of `apply_variant`. The project conventions forbid panics in production code.
Suggestion: Return `Err(MutationError::Forbidden { reason: ForbiddenReason::RollbackTargetUnknown })` instead.

[WARNING] Duplicate `AuditEvent` enum across modules
File: core/crates/core/src/audit.rs vs core/crates/core/src/storage/types.rs
Lines: 11 (both files)
Description: Two separate `AuditEvent` enums exist with the same name but different types.
Suggestion: Either merge into a single enum or rename the storage variant to avoid shadowing.

[WARNING] Duplicate `UnixSeconds` type alias
File: core/crates/core/src/model/primitive.rs vs core/crates/core/src/storage/types.rs
Lines: general
Description: `pub type UnixSeconds = i64` is defined in both modules.
Suggestion: Remove the alias from `storage::types` and update remaining imports.

[WARNING] `ValidationRule::PolicyAttachmentMissing` overloaded for unrelated failures
File: core/crates/core/src/mutation/validate.rs
Lines: 98, 162, 191, 197, 215, 223
Description: `PolicyAttachmentMissing` is used for six distinct failure modes.
Suggestion: Add `RouteNotFound` and `RouteHasNoAttachment` variants to `ValidationRule`.

[WARNING] `ValidationRule::DuplicateRouteId` used for upstream duplicate
File: core/crates/core/src/mutation/validate.rs
Lines: 36
Description: `CreateUpstream` returns `ValidationRule::DuplicateRouteId` when the upstream ID already exists.
Suggestion: Add `DuplicateUpstreamId` variant.

[HIGH] `UpdateRoute` patch does not validate hostnames
File: core/crates/core/src/mutation/validate.rs
Lines: 141-151
Description: `check_update_route` validates existing route and upstream refs but not patched hostnames.
Suggestion: Add hostname validation when `patch.hostnames` is present.

[WARNING] `DeleteUpstream` does not check for orphan route references
File: core/crates/core/src/mutation/validate.rs
Lines: 44-53
Description: `DeleteUpstream` doesn't verify no routes reference the upstream being deleted.
Suggestion: Scan `state.routes` for references before allowing deletion.

[SUGGESTION] `gen_mutation_schemas` binary in `core` crate blurs three-layer boundary
File: core/crates/core/src/bin/gen_mutation_schemas.rs
Lines: general
Description: Binary with I/O lives inside the core crate directory.
Suggestion: Consider moving to `cli` crate or a dedicated xtask.

[SUGGESTION] `RoutePatch` doc comment inaccurate about Option<Option<T>> convention
File: core/crates/core/src/mutation/patches.rs
Lines: 29-35
Description: Doc comment claims all fields follow triple-state pattern but several use single Option.
Suggestion: Update doc comment to clarify which fields use single vs double Option.

[SUGGESTION] `MutationKind` missing `schemars::JsonSchema` derive
File: core/crates/core/src/mutation/types.rs
Lines: 193
Description: `Mutation` derives `schemars::JsonSchema` but `MutationKind` does not.
Suggestion: Add `schemars::JsonSchema` derive to `MutationKind`.

## minimax Review
**Date:** 2026-05-03

[WARNING] `schemars` in core `[dependencies]` vs dev/build dependency
File: core/crates/core/Cargo.toml
Lines: 13
Description: `schemars = "0.8"` in main [dependencies] introduces a compile-time dependency not needed for core logic.
Suggestion: Consider gating behind a feature flag or build-dependency.

[WARNING] Hardcoded relative path to workspace schema dir
File: core/crates/core/build.rs
Lines: 16
Description: The path `../../../docs/schemas/mutations` is hardcoded and fragile.
Suggestion: Validate the path or use workspace-relative resolution.

[WARNING] `gen_mutation_schemas` uses `process::exit(1)` bypassing drop semantics
File: core/crates/core/src/bin/gen_mutation_schemas.rs
Lines: 99
Description: `std::process::exit(1)` bypasses Rust drop semantics.
Suggestion: Use `std::process::ExitCode` or propagate through Result.

[SUGGESTION] Schema stubs use non-standard `x-variant` extension field
File: core/crates/core/src/bin/gen_mutation_schemas.rs
Lines: 91
Description: `x-variant` is not a JSON Schema standard field.
Suggestion: Remove `x-variant`; `title` already identifies the variant.

[WARNING] No 13-variant idempotency test coverage in proptest
File: core/crates/core/tests/mutation_props.rs
Lines: general
Description: Proptest suite only exercises CreateRoute; doesn't cover all 13 variants.
Suggestion: Add property tests covering all 13 variants.

[WARNING] All-None GlobalConfigPatch is unvalidated no-op
File: core/crates/core/src/model/global.rs
Lines: 59
Description: A SetGlobalConfig with all-None patch is accepted as a no-op with no warning.
Suggestion: Validate or treat as deliberate no-op with a warning.

[WARNING] `build.rs` missing `cargo:rerun-if-changed=` for schema output directory
File: core/crates/core/build.rs
Lines: 20
Description: Schema files not tracked as build input, stale builds won't detect need to regenerate.
Suggestion: Emit `cargo:rerun-if-changed=docs/schemas/mutations/`.

[SUGGESTION] `DesiredState::empty()` is identical to `Default::default()`
File: core/crates/core/src/model/desired_state.rs
Lines: 37
Description: `empty()` delegates entirely to `Self::default()` with no semantic distinction.
Suggestion: Remove `empty()` and use `DesiredState::default()` directly.

[WARNING] `apply_mutation` error variants lack context fields
File: core/crates/core/src/mutation/error.rs
Lines: general
Description: Some error variants carry no payload, making debugging difficult.
Suggestion: Enrich error variants with context fields like version numbers, IDs.

[SUGGESTION] Duplicate `Debug` derive and manual impl on `AuditEvent`
File: core/crates/core/src/audit.rs
Lines: 72
Description: `AuditEvent` derives `Debug` but also has a manual `impl fmt::Debug` block.
Suggestion: Remove the derive if manual impl exists, or vice versa.

[WARNING] Schema stubs lack `$id` field
File: docs/schemas/mutations/*.json
Lines: general
Description: Per-variant stub schema files lack `$id` field for JSON Schema registry identification.
Suggestion: Add `"$id": "<VariantName>"` to each stub.

## kimi Review
**Date:** 2026-05-03

[CRITICAL] Three-state patch semantics broken on RoutePatch and UpstreamPatch
File: core/crates/core/src/mutation/patches.rs
Lines: 27-55, 63-81
Description: `RoutePatch.redirects`, `RoutePatch.policy_attachment`, and `UpstreamPatch.max_request_bytes` are typed `Option<Option<T>>` but do NOT use `deserialize_with = "double_option::deserialize"`. Default serde collapses `null` to outer `None`, so clients cannot express "clear this field."
Suggestion: Add `deserialize_with = "double_option::deserialize"` to each `Option<Option<T>>` field on both patch structs, and add serde round-trip tests.

[HIGH] ValidationRule misuse: PolicyAttachmentMissing for missing routes
File: core/crates/core/src/mutation/validate.rs
Lines: 95-104, 141-151, 154-183, 185-203, 205-233
Description: `check_route_exists` returns `ValidationRule::PolicyAttachmentMissing` when the route is missing. This is semantically wrong.
Suggestion: Add `RouteMissing` variant to `ValidationRule` and use it for every "route does not exist" path.

[HIGH] ValidationRule misuse: DuplicateRouteId for upstream collision
File: core/crates/core/src/mutation/validate.rs
Lines: 33-42
Description: `CreateUpstream` rejects duplicates with `ValidationRule::DuplicateRouteId`.
Suggestion: Add `DuplicateUpstreamId` variant to `ValidationRule`.

[HIGH] DesiredState.policies is dead state — never populated by any mutation handler
File: core/crates/core/src/model/desired_state.rs
Lines: 28-29
Description: `policies: BTreeMap<PolicyId, PolicyAttachment>` is declared but no mutation handler ever inserts/reads/removes entries. Will affect canonical hashing in Phase 5.
Suggestion: Remove `policies` and `PolicyId` until there is a real owner, OR wire `AttachPolicy` to populate it.

[HIGH] PolicyAttachment and RoutePolicyAttachment are structural duplicates
File: core/crates/core/src/model/policy.rs
Lines: 22-28
Description: Two types with identical fields and semantics exist in different modules.
Suggestion: Pick one canonical name and delete the other.

[WARNING] apply_set_global_config / apply_set_tls_config emit no-op diffs
File: core/crates/core/src/mutation/apply.rs
Lines: 273-323
Description: Both functions unconditionally push a DiffChange even when the patch made no actual changes.
Suggestion: Skip diff emission if before == after.

[WARNING] apply_import_caddyfile emits coarse "whole-map" diffs and silently overwrites
File: core/crates/core/src/mutation/apply.rs
Lines: 325-354
Description: Two coarse DiffChange entries for entire maps, and existing entities silently overwritten.
Suggestion: Emit per-entity DiffChanges and document the collision policy.

[WARNING] Hostname total-length check bypassed for wildcards
File: core/crates/core/src/model/route.rs
Lines: 110-135
Description: For wildcard hostnames, the 253-char total length check excludes the `*.` prefix.
Suggestion: Check `s.len() > 253` before the wildcard branch.

[WARNING] Identifier newtypes have no ULID construction/deserialization validation
File: core/crates/core/src/model/identifiers.rs
Lines: 8-55
Description: `pub String` inner field plus derived `Deserialize` means any string is accepted as a valid ID.
Suggestion: Add private inner field + checked constructor, or downgrade docs to "opaque string identifier".

[WARNING] proptest commutativity test only checks key membership not state equality
File: core/crates/core/tests/mutation_props.rs
Lines: 134-152
Description: Test only checks route presence, not full state equality.
Suggestion: Replace membership checks with `prop_assert_eq!(ab.new_state, ba.new_state)`.

[WARNING] storage::types::AuditEvent and audit::AuditEvent name collision
File: core/crates/core/src/audit.rs
Lines: 9-103
Description: Two public types named `AuditEvent` in different modules.
Suggestion: Rename one before Phase 6 starts persisting events.

[SUGGESTION] Dead audit_event_for arm for Rollback
File: core/crates/core/src/mutation/apply.rs
Lines: 463-480
Description: The Rollback arm is unreachable in Phase 4 since pre_conditions always rejects it.
Suggestion: Annotate with `// reachable in Phase 7` or add a tracked id comment.

[SUGGESTION] MutationOutcome.kind field should be renamed audit_event
File: core/crates/core/src/mutation/outcome.rs
Lines: 6-15
Description: Field named `kind` carries an `AuditEvent` but `kind` is used for discriminants elsewhere.
Suggestion: Rename to `audit_event`.

[SUGGESTION] Diff serialization swallows errors silently via .ok()
File: core/crates/core/src/mutation/apply.rs
Lines: general
Description: Every `apply_*` function uses `serde_json::to_value(...).ok()`, silently producing None on serialization failure.
Suggestion: Propagate errors or use `.expect()` with load-bearing message.

[SUGGESTION] build.rs hardcodes relative path to schema dir
File: core/crates/core/build.rs
Lines: 12-16
Description: Hardcoded `../../../docs/schemas/mutations` path is fragile.
Suggestion: Resolve workspace root via `cargo metadata`.

## Phase-End Simplify
**Date:** 2026-05-03

**SKIPPED** VOCAB const duplicates AUDIT_KINDS slice — `audit_vocab::AUDIT_KINDS` does not exist in Phase 4; local VOCAB kept with comment noting future consolidation
**DONE** check_hostnames_valid discards HostnameError details — hint now includes specific error message — commit 23ba033
**DONE** Dead second `s.len() > 253` check in validate_hostname — removed unreachable duplicate — commit 23ba033
**DONE** `serde_json::to_value(...).ok()` repeated ~20× — extracted `to_json<T>` helper in apply.rs — commit 23ba033
**DONE** Policy mutation preamble copy-pasted 3× — extracted `policy_attachment_preamble` helper — commit 23ba033
**DONE** Four inline "route not found" checks — replaced with `check_route_exists()` in all three policy validators — commit 23ba033
**DONE** `check_delete_upstream` wrong ValidationRule — added `UpstreamStillReferenced` variant to `ValidationRule` — commit 23ba033
**SKIPPED** capability.rs route-module derivation duplication — only 2 call sites, three-use rule not met
**DONE** Rename `caps_with_everything` → `empty_caps` in proptest — commit 23ba033
