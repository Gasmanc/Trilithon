# Phase 9 — Scope Guardian Review Findings

**Reviewer:** scope_guardian
**Date:** 2026-05-15
**Diff range:** fd5127f..HEAD
**Phase:** 9

---

[WARNING] bootstrap_password_not_in_env test missing
File: core/crates/adapters/tests/
Lines: general
Description: The TODO for slice 9.4 requires five tests, including `bootstrap_password_not_in_env.rs` ("capture std::env::vars() after bootstrap; assert the password does not appear in any value"). The diff adds only four bootstrap tests. The env-vars test is absent from both the file list and the Cargo.toml [[test]] entries.
Suggestion: Add core/crates/adapters/tests/bootstrap_password_not_in_env.rs and a corresponding [[test]] entry in Cargo.toml.

[WARNING] ApiError enum diverges from 9.11 spec signature
File: core/crates/adapters/src/http_axum/auth_routes.rs
Lines: general
Description: The 9.11 spec defines ApiError without a `BadRequest` variant, but the implementation adds `BadRequest(String)` used by audit_routes (event kind validation → 400) and mutation handler. This represents a coherence deviation from the canonical error envelope spec.
Suggestion: Determine whether `BadRequest` should be formally added to the ApiError spec or whether `Unprocessable { detail }` (422) covers the unknown-event-kind case. If 400 is correct, update the spec.

[WARNING] health_handler lives in http_axum/mod.rs rather than health.rs
File: core/crates/adapters/src/http_axum.rs
Lines: 1327-1345
Description: Slice 9.1 specifies `src/http_axum/health.rs` as the file for the health handler. The diff places `health_handler` directly in `src/http_axum.rs` (the module root). This is a structural deviation from the spec's file layout, not a functional gap.
Suggestion: Either extract health_handler into src/http_axum/health.rs, or update the slice documentation to reflect the consolidated layout.

[SUGGESTION] stubs.rs is unspecified but appropriate for test infrastructure
File: core/crates/adapters/src/http_axum/stubs.rs
Lines: general
Description: The TODO does not reference stubs.rs. It introduces NoopStorage, NoopApplier, NoopDriftDetector, and make_test_app_state helper used by integration tests. This is test-support infrastructure, not production code.
Suggestion: No action required if the team accepts test infrastructure additions outside named work units. Document in a follow-up TODO entry if strict scope hygiene is required.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-16 — do not edit content above this line -->

See `docs/End_of_Phase_Reviews/Fixed/phase_9_fixed.md` for all fixed findings and `docs/End_of_Phase_Reviews/Unfixed/phase_9_unfixed.md` for deferred/wont-fix items. Key fixes: F002 (adopt 501), F005/F040 (bootstrap atomicity), F006 (snapshot ordering), F008/F009 (cookie/IP), F010 (rate limiter eviction), F015 (capability cache), F018 (timing/audit), F019 (SHA-256 doc), F023 (drift SQL), F028 (env-leak test), F037 (dead code).
