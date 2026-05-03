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
