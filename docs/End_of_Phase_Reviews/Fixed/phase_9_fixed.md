# Phase 9 — Fixed Findings

**Run date:** 2026-05-16T00:00:00Z
**Total fixed:** 13

| ID | Severity | Title | File | Commit | PR | Date |
|----|----------|-------|------|--------|----|------|
| F002 | CRITICAL | Drift adopt semantics inverted | `src/http_axum/drift_routes.rs` | `2386697` | — | 2026-05-16 |
| F005 | HIGH | Bootstrap credentials file not atomic | `src/auth/bootstrap.rs` | `cf1359c` | — | 2026-05-16 |
| F006 | HIGH | Snapshot inserted before confirmed apply | `src/http_axum/mutations.rs` | `2386697` | — | 2026-05-16 |
| F008 | HIGH | Session cookie Secure flag hardcoded false | `src/http_axum/auth_routes.rs` | `2386697` | — | 2026-05-16 |
| F009 | HIGH | Client IP not resolved from X-Forwarded-For | `src/http_axum/auth_routes.rs` | `2386697` | — | 2026-05-16 |
| F010 | HIGH | Rate limiter map unbounded growth | `src/auth/rate_limit.rs` | `2386697` | — | 2026-05-16 |
| F015 | HIGH | Mutation handler uses empty CapabilitySet | `src/http_axum/mutations.rs` | `2386697` | — | 2026-05-16 |
| F018 | HIGH | Username enumeration via timing/audit actor | `src/http_axum/auth_routes.rs` | `2386697` | — | 2026-05-16 |
| F019 | WARNING | SHA-256 token hash missing safety doc | `src/http_axum/auth_middleware.rs` | `2386697` | — | 2026-05-16 |
| F023 | WARNING | resolve_drift_event matches already-resolved rows | `src/sqlite_storage.rs` | `2386697` | — | 2026-05-16 |
| F028 | WARNING | bootstrap_password_not_in_env test missing | `tests/bootstrap_password_not_in_env.rs` | `cf1359c` | — | 2026-05-16 |
| F037 | SUGGESTION | Dead code build_snapshot_from_desired | `src/http_axum/drift_routes.rs` | `2386697` | — | 2026-05-16 |
| F040 | SUGGESTION | Non-Unix bootstrap uses File::create not create_new | `src/auth/bootstrap.rs` | `cf1359c` | — | 2026-05-16 |
