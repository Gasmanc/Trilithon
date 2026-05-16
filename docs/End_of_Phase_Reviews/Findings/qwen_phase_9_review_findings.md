# Phase 9 — Qwen Review Findings

**Reviewer:** qwen
**Date:** 2026-05-15
**Diff range:** fd5127f..HEAD
**Phase:** 9

---

[CRITICAL] Rate limiter backoff exponent grows unbounded — permanent lockout after 68+ failures
File: core/crates/adapters/src/auth/rate_limit.rs
Lines: 58-64
Description: record_failure's exponent `bucket.failure_count - 4` grows unbounded. After 68+ consecutive failures, `2_i64.saturating_pow(64)` saturates to i64::MAX, setting next_allowed_at_unix to near-overflow. All subsequent check calls compute secs = i64::MAX - now, which overflows or always exceeds u32::MAX — hitting unwrap_or(u32::MAX). Bucket permanently locked with retry_after = 4294967295 (~136 years).
Suggestion: Cap the exponent before saturating_pow: `let exponent = (bucket.failure_count - 4).min(10);` (2^10 = 1024 > 60 cap makes values beyond 10 meaningless).

[WARNING] Drift adopt handler re-applies desired state instead of adopting running state
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: 127-178
Description: The adopt handler re-applies the desired state snapshot through the applier — effectively identical to reapply. True adopt would take the running Caddy config hash and set it as the new desired state.
Suggestion: Implement true adopt by fetching the running config from Caddy and persisting it as desired state, or rename the semantic to match actual behaviour.

[WARNING] Inconsistent error handling in drift route handlers — uses raw tuples instead of ApiError
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: 54-62 and throughout
Description: Drift handlers use serde_json::json! and return raw (StatusCode, Json(Value)) tuples for internal errors, while all other route handlers use the unified ApiError enum. Error responses have different JSON shapes.
Suggestion: Convert drift handlers to return Result<..., ApiError> consistently with other handlers.

[SUGGESTION] Dead code build_snapshot_from_desired retained with allow(dead_code)
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: 339-397
Description: Annotated #[allow(dead_code)] with reason "retained for future use." Contradicts the no-suppressions-without-tracked-id rule and no-TODOs-in-committed-code rule.
Suggestion: Remove the dead code now and re-implement when needed.

[SUGGESTION] Session touch constructs returned Session with revoked_at: None unconditionally
File: core/crates/adapters/src/auth/sessions.rs
Lines: 157-181
Description: The returned Session always has revoked_at: None even for sessions that the method correctly identifies as revoked/expired (which return None). Violates principle of least surprise.
Suggestion: Preserve the actual revoked_at value from the row in the returned Session.

[SUGGESTION] Rate limiter lacks cleanup of expired buckets — unbounded memory growth
File: core/crates/adapters/src/auth/rate_limit.rs
Lines: 30-35
Description: DashMap<IpAddr, BucketState> grows without bound. Expired entries are never evicted.
Suggestion: Add periodic cleanup task removing entries where now_unix >= next_allowed_at_unix, or cap map size with an eviction policy.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-16 — do not edit content above this line -->

See `docs/End_of_Phase_Reviews/Fixed/phase_9_fixed.md` for all fixed findings and `docs/End_of_Phase_Reviews/Unfixed/phase_9_unfixed.md` for deferred/wont-fix items. Key fixes: F002 (adopt 501), F005/F040 (bootstrap atomicity), F006 (snapshot ordering), F008/F009 (cookie/IP), F010 (rate limiter eviction), F015 (capability cache), F018 (timing/audit), F019 (SHA-256 doc), F023 (drift SQL), F028 (env-leak test), F037 (dead code).
