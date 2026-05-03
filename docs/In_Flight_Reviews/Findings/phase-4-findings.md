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
