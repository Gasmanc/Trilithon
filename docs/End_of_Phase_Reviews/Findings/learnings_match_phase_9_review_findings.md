# Phase 9 — Learnings Match Review Findings

**Reviewer:** learnings_match
**Date:** 2026-05-15
**Diff range:** fd5127f..HEAD
**Phase:** 9

---

[WARNING] Known pattern: ulid-sort-key-stable-ordering
File: core/crates/adapters/src/http_axum/audit_routes.rs
Lines: general
Description: Audit list endpoint may sort by occurred_at rather than ULID id column — rows within the same second will have non-deterministic ordering and pagination cursor will be unstable.
Suggestion: Review docs/solutions/best-practices/ulid-sort-key-stable-ordering-2026-05-09.md — sort by ULID id column, not occurred_at, to get stable ordering with millisecond tie-breaking.

[WARNING] Known pattern: sqlite-like-wildcard-escape
File: core/crates/adapters/src/http_axum/audit_routes.rs, core/crates/adapters/src/http_axum/routes.rs
Lines: general
Description: User-controlled filter strings (event, hostname_filter) passed into LIKE clauses without escaping %, _, and \ let callers inject wildcards that match unintended rows.
Suggestion: Review docs/solutions/security-issues/sqlite-like-wildcard-escape-2026-05-08.md — escape user input and specify ESCAPE clause in all LIKE queries.

[WARNING] Known pattern: sqlite-static-sql-coalesce-optional-filters
File: general
Lines: general
Description: Optional filter parameters on audit/route/snapshot list endpoints are an injection surface if built with format!(). Dynamic SQL construction should be replaced with static queries.
Suggestion: Review docs/solutions/security-issues/sqlite-static-sql-coalesce-optional-filters-2026-05-05.md — use ? IS NULL OR col = ? pattern instead of format!()-constructed SQL.

[WARNING] Known pattern: cidr-validate-at-mutation-boundary
File: core/crates/adapters/src/http_axum/mutations.rs
Lines: general
Description: The mutation endpoint accepts user-supplied route config. Structured field types (IP ranges, hostnames) must be validated at the HTTP handler boundary, not deferred to apply time.
Suggestion: Review docs/solutions/security-issues/cidr-validate-at-mutation-boundary-2026-05-06.md — validate at mutation boundary.

[WARNING] Known pattern: tokio-mutex-in-async-test-doubles
File: core/crates/adapters/tests/
Lines: general
Description: Async test doubles using std::sync::Mutex will poison the lock on panic and cascade-fail subsequent tests.
Suggestion: Review docs/solutions/best-practices/tokio-mutex-in-async-test-doubles-2026-05-08.md — use tokio::sync::Mutex in all async test doubles.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-16 — do not edit content above this line -->

See `docs/End_of_Phase_Reviews/Fixed/phase_9_fixed.md` for all fixed findings and `docs/End_of_Phase_Reviews/Unfixed/phase_9_unfixed.md` for deferred/wont-fix items. Key fixes: F002 (adopt 501), F005/F040 (bootstrap atomicity), F006 (snapshot ordering), F008/F009 (cookie/IP), F010 (rate limiter eviction), F015 (capability cache), F018 (timing/audit), F019 (SHA-256 doc), F023 (drift SQL), F028 (env-leak test), F037 (dead code).
