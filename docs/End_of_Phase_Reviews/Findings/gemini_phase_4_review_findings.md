# Phase 4 — Gemini Review Findings

**Reviewer:** gemini
**Date:** 2026-05-06
**Diff range:** 43f89ca..a948fa8
**Phase:** 4

---

[HIGH] Route updated_at is never updated by patch mutations
File: core/crates/core/src/mutation/apply.rs
Lines: 236-271
Description: The `Route` struct includes an `updated_at` field, but the `apply_route_patch` function only updates fields present in the `RoutePatch`. Since `apply_mutation` is designed to be pure and does not take a timestamp, and `RoutePatch` does not include one, the `updated_at` field remains at its creation value even after modifications.
Suggestion: Include an `updated_at` field in `RoutePatch` or pass a `now: UnixSeconds` argument to `apply_mutation` to ensure timestamps are maintained.

[WARNING] ImportFromCaddyfile lacks collision validation
File: core/crates/core/src/mutation/validate.rs
Lines: 61
Description: The `ImportFromCaddyfile` variant has no pre-conditions, allowing it to silently overwrite existing routes and upstreams in the `DesiredState`. `apply_import_caddyfile` perform unconditional `insert` operations on the maps, which could lead to accidental data loss if a user-supplied Caddyfile contains IDs already present in the state.
Suggestion: Implement checks in `validate.rs` to ensure imported IDs do not collide with existing ones, or enforce that the mutation payload is internally consistent.

[WARNING] Insufficient capability gating for TLS configuration
File: core/crates/core/src/mutation/capability.rs
Lines: 104-110
Description: The capability check for `SetTlsConfig` only requires the `tls` module if `patch.email` is provided. However, updating other fields like `on_demand_enabled` or `default_issuer` also requires the `tls` application to be available in Caddy.
Suggestion: Update `referenced_caddy_modules` to require the `tls` module if any field in `TlsConfigPatch` is being modified.

[WARNING] Incorrect module mapping for request header rules
File: core/crates/core/src/mutation/capability.rs
Lines: 32, 45
Description: `referenced_caddy_modules` maps `route.headers.request` to the `http.handlers.rewrite` module. In Caddy, `rewrite` is for URI manipulation; header operations (set/add/delete) are handled by the `http.handlers.headers` module.
Suggestion: Map both `request` and `response` header rules to the `http.handlers.headers` module.

[SUGGESTION] Missing validation for route forwarding completeness
File: core/crates/core/src/mutation/validate.rs
Lines: 14-20
Description: `CreateRoute` pre-conditions do not verify that a route specifies at least one upstream or a redirect rule. A route with neither is functionally a "black hole" and likely represents a configuration error that could lead to invalid Caddyfile generation in later phases.
Suggestion: Add a validation rule requiring at least one destination (non-empty `upstreams` or a `redirect`).

[SUGGESTION] Caddyfile import warnings are discarded
File: core/crates/core/src/mutation/apply.rs
Lines: 212-233
Description: The `ParsedCaddyfile` struct includes a `warnings` field generated during parsing, but `apply_import_caddyfile` discards this information. These warnings are not surfaced in the `MutationOutcome` or stored in the state, making it impossible for users to know if their import was only partially successful.
Suggestion: Include import warnings in the `MutationOutcome` or attach them to the resulting `AuditEvent`.

[SUGGESTION] HostPattern variant mismatch in validation
File: core/crates/core/src/mutation/validate.rs
Lines: 108
Description: `check_hostnames_valid` only checks if `validate_hostname` returns an error. It does not verify that the provided `HostPattern` variant matches the content (e.g., `HostPattern::Exact("*.example.com")` will pass validation). This creates an inconsistency between the discriminant and the payload that may cause logic errors in Phase 3 Caddyfile generation.
Suggestion: Assert that the provided enum variant matches the variant returned by the `validate_hostname` factory function.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-06 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Route updated_at is never updated by patch mutations | ✅ Fixed | `7d90fc1` | — | 2026-05-06 | F033 — doc clarifies updated_at is set by persistence layer |
| 2 | ImportFromCaddyfile lacks collision validation | 🔕 Superseded | — | — | — | Same root as F001/F004 (ImportFromCaddyfile validation) |
| 3 | Insufficient capability gating for TLS configuration | ✅ Fixed | `21e330d` | — | 2026-05-06 | F040 — any non-None TLS field requires tls module |
| 4 | Incorrect module mapping for request header rules | ✅ Fixed | `21e330d` | — | 2026-05-06 | F041 — headers.request/response → http.handlers.headers |
| 5 | Missing validation for route forwarding completeness | ✅ Fixed | `3787298` | — | 2026-05-06 | F057 — check_route_has_destination rejects black-hole routes |
| 6 | Caddyfile import warnings are discarded | ⏭️ Deferred | — | — | — | F058 — Phase 5; MutationOutcome schema changes deferred |
| 7 | HostPattern variant mismatch in validation | ✅ Fixed | `6e70eca` | — | 2026-05-06 | F059 — validate_hostname variant consistency check added |
