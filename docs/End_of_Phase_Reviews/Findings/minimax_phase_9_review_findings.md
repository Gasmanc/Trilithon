# Phase 9 — Minimax Review Findings

**Reviewer:** minimax
**Date:** 2026-05-15T00:00:00Z
**Diff range:** fd5127f..HEAD
**Phase:** 9

---

[HIGH] SILENT_AUDIT_WRITE_FAILURE_IN_CHANGE_PASSWORD
File: core/crates/adapters/src/http_axum/auth_routes.rs
Lines: 393-401
Description: The loop that emits `AuthSessionRevoked` audit rows discards errors with `let _ = ... await`, silently ignoring database failures. While the HTTP response (204 NO_CONTENT) is already sent, this means audit events are lost without any logging or error propagation.
Suggestion: At minimum log the error with `tracing::warn!` even if the response is unaffected. Alternatively, accumulate errors and return an error response if any audit write fails.

[HIGH] SILENT_AUDIT_WRITE_FAILURE_IN_DRIFT_ADOPT
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: 326-337
Description: The supplemental mutation.applied audit row in the adopt handler is emitted with `let _ = ... await` — audit failure is silently discarded with no logging.
Suggestion: Same as above — at minimum log the error. Consider returning an error response if audit write fails, since the caller has no way to know the mutation was applied but audit was not recorded.

[HIGH] SILENT_AUDIT_WRITE_FAILURE_IN_DRIFT_REAPPLY
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: 410-421
Description: Same silent discard pattern for the mutation.applied audit row in reapply handler.
Suggestion: Same fix.

[WARNING] ASSERTION_WITHOUT_COMMENT_IN_PRODUCTION_CODE
File: core/crates/adapters/src/http_axum/auth_routes.rs
Lines: 278-280
Description: `HeaderValue::from_str(&value).unwrap()` — the unwrap asserts that the cookie string format never produces an invalid HTTP header value, which is true by construction but not self-evident.
Suggestion: Add a comment `// SAFETY: cookie string format is always a valid HTTP header value` or restructure to make the invariant explicit without unwrap.

[WARNING] SHADOWED_VARIABLE_IN_DRIFT_HANDLER
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: 266-269
Description: The outer `correlation_id` is created on line 266 but a new inner scope variable with the same name shadows it on line 269 inside the `match`. The outer correlation_id is not used after line 269, so this is not a bug, but it could be clearer.
Suggestion: Use distinct names (`mutation_correlation_id` and `snap`) to avoid confusion.

[WARNING] BOOTSTRAP_PASSWORD_WRITABLE_BEFORE_CHECK_ON_FILE_PERMISSIONS
File: core/crates/adapters/src/auth/bootstrap.rs
Lines: 137-155
Description: The comment ordering implies the data_dir check happens before password generation, but in the code the password is generated before the check. If the check fails, we error out before the write (no plaintext leaks), but the comment order is misleading.
Suggestion: Reorder so the data_dir check happens before password generation, and update the step comments to match.

[SUGGESTION] DEAD_CODE_WITH_ALLOW_ANNOTATION
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: 341-397
Description: `build_snapshot_from_desired` is annotated `#[allow(dead_code)]` with comment "retained for future use when adopt needs to build a snapshot from running state". If the future use case doesn't materialize, this should be removed rather than carried indefinitely.
Suggestion: If not used within the current phase scope, remove it. If planned for a future slice, add a TODO tracking issue referenced in the allow annotation.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-16 — do not edit content above this line -->

See `docs/End_of_Phase_Reviews/Fixed/phase_9_fixed.md` for all fixed findings and `docs/End_of_Phase_Reviews/Unfixed/phase_9_unfixed.md` for deferred/wont-fix items. Key fixes: F002 (adopt 501), F005/F040 (bootstrap atomicity), F006 (snapshot ordering), F008/F009 (cookie/IP), F010 (rate limiter eviction), F015 (capability cache), F018 (timing/audit), F019 (SHA-256 doc), F023 (drift SQL), F028 (env-leak test), F037 (dead code).
