# Phase 9 — Kimi Review Findings

**Reviewer:** kimi
**Date:** 2026-05-15T00:00:00Z
**Diff range:** fd5127f..HEAD
**Phase:** 9

---

[CRITICAL] INSECURE_TOKEN_GENERATION_IN_STUBS
File: core/crates/adapters/src/http_axum/stubs.rs
Lines: 85-90
Description: `random_token()` uses only 32 bits of nanosecond timestamps, making tokens extremely predictable. Anyone who knows the approximate time a session was created can brute-force the token in under a second. While this is stub/test infrastructure, there is no comment clarifying it is NOT production-ready, and GLM's CRITICAL finding shows the production daemon routes through these stubs.
Suggestion: Replace with cryptographically secure random generation using `rand::thread_rng()` or `uuid::Uuid::new_v4()`. Add an explicit comment that this is test-only infrastructure and must never be used in production paths.

[HIGH] DRIFT_ADOPT_LACKS_IDEMPOTENCY
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: 45-80
Description: The adopt endpoint writes a resolved row but doesn't implement idempotency. If the same adopt request is sent twice (network retry), it could create duplicate resolved drift records.
Suggestion: Return the existing record if one already exists for the same route_id/resolved_at combination.

[HIGH] MISSING_RATE_LIMITING_ON_DRIFT_ENDPOINTS
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: general
Description: The new drift endpoints don't have rate limiting applied. Drift detection operations compare snapshots and can be expensive; without rate limiting they could be abused to cause resource exhaustion.
Suggestion: Apply the same rate-limiting middleware used by other protected endpoints.

[HIGH] SNAPSHOT_DIFF_REDACTION_MAY_BE_INCOMPLETE
File: core/crates/adapters/tests/snapshots_diff_redacts_secrets.rs
Lines: general
Description: The redaction test only tests a single "secret" pattern. If the implementation only redacts fields literally named "secret", it would miss common credential field names like "password", "token", "key", "credential".
Suggestion: Extend the redaction pattern list and add test cases for all common sensitive field name patterns.

[WARNING] TEST_INFRASTRUCTURE_APPSTATE_DUPLICATION
File: general
Lines: general
Description: AppState construction boilerplate is duplicated across 20+ test files. A shared `test_helpers::make_app()` would reduce maintenance burden.
Suggestion: Extract a shared test helper for constructing AppState.

[WARNING] OFFSET_BASED_PAGINATION
File: core/crates/adapters/tests/snapshots_list_pagination.rs
Lines: general
Description: Pagination appears to use offset-based cursors, which have known issues with concurrent inserts/deletes causing items to appear twice or be skipped.
Suggestion: Use cursor-based pagination (last seen ID/timestamp) for production use.

[WARNING] MISSING_CONCURRENT_DRIFT_DETECTION_TESTS
File: core/crates/adapters/tests/
Lines: general
Description: The drift tests cover basic CRUD operations but don't test concurrent scenarios: route deleted while drift detection runs, snapshot updated between drift computation and adoption.
Suggestion: Add integration tests for concurrent drift resolution scenarios.

[WARNING] INCONSISTENT_ERROR_RESPONSE_BODIES
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: general
Description: Some error responses return plain strings while others return JSON objects. Inconsistent error formats make client-side error handling harder.
Suggestion: Use the unified ApiError enum for all error responses.

[WARNING] AUDIT_CORRELATION_FILTER_LINEAR_SCAN
File: core/crates/adapters/tests/audit_list_correlation_filter.rs
Lines: general
Description: The correlation ID filter in the audit log likely performs a linear scan over all audit records. For production use with large audit logs, this needs an index on the correlation_id field.
Suggestion: Add a database index on the correlation_id column in the audit log table.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-16 — do not edit content above this line -->

See `docs/End_of_Phase_Reviews/Fixed/phase_9_fixed.md` for all fixed findings and `docs/End_of_Phase_Reviews/Unfixed/phase_9_unfixed.md` for deferred/wont-fix items. Key fixes: F002 (adopt 501), F005/F040 (bootstrap atomicity), F006 (snapshot ordering), F008/F009 (cookie/IP), F010 (rate limiter eviction), F015 (capability cache), F018 (timing/audit), F019 (SHA-256 doc), F023 (drift SQL), F028 (env-leak test), F037 (dead code).
