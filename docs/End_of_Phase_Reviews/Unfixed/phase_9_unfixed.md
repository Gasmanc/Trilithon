# Phase 9 — Unfixed Findings

**Run date:** 2026-05-16T00:00:00Z
**Total unfixed:** 16 (7 deferred · 4 won't fix · 5 superseded/excluded)

| ID | Severity | Consensus | Title | File | Status | Reason |
|----|----------|-----------|-------|------|--------|--------|
| F001 | CRITICAL | UNANIMOUS | Production HTTP server wired to stubs | `cli/src/run.rs` | wont_fix | Already fixed during Phase 9 implementation — real AppState wired |
| F003 | CRITICAL | SINGLE | Rate limiter permanent lockout | `src/auth/rate_limit.rs` | wont_fix | Not present — .min(60) cap already prevents overflow |
| F004 | CRITICAL | SINGLE | Insecure token generation in stubs | `src/http_axum/stubs.rs` | wont_fix | Superseded by F001 being already fixed; stubs are test-only |
| F007 | HIGH | UNANIMOUS | Internal errors leak detail to clients | `src/http_axum/auth_routes.rs` | wont_fix | Already implemented — Internal variant logs server-side, returns generic body |
| F011 | HIGH | MAJORITY | Session store missing user_id FK index | `migrations/` | deferred | Requires new migration; tracked for next DB-change phase |
| F012 | HIGH | MAJORITY | Token pool stores plaintext secret | `src/http_axum/auth_middleware.rs` | deferred | Significant refactor of token storage; Phase 10+ |
| F013 | HIGH | SINGLE | Token revocation not propagated to session store | `src/http_axum/` | deferred | Requires session/token store coordination; Phase 10+ |
| F014 | HIGH | SINGLE | Audit chain hash not verified on read | `src/sqlite_storage.rs` | deferred | New read-path feature; Phase 10+ |
| F016 | HIGH | SINGLE | Snapshot insert not idempotent on retry | `src/sqlite_storage.rs` | deferred | Requires INSERT OR IGNORE + content-hash dedup design |
| F017 | HIGH | SINGLE | DriftDetector not started in HTTP server | `cli/src/run.rs` | deferred | Wiring work for Phase 10 |
| F020 | WARNING | SINGLE | Token middleware leaks token existence via 401 vs 403 | `src/http_axum/auth_middleware.rs` | deferred | Requires response normalisation; Phase 10 |
| F021 | WARNING | SINGLE | Drift event timestamp not monotonic | `src/sqlite_storage.rs` | deferred | Clock skew handling; future phase |
| F022 | WARNING | SINGLE | Snapshot parent_id not validated on insert | `src/sqlite_storage.rs` | deferred | Orphan prevention; future phase |
| F024 | WARNING | SINGLE | Audit log truncated on storage error | `src/audit_writer.rs` | deferred | Retry/WAL design; future phase |
| F025 | WARNING | SINGLE | Drift reapply does not verify snapshot before apply | `src/http_axum/drift_routes.rs` | deferred | Correctness check; future phase |
| F026 | WARNING | SINGLE | Session cookie SameSite not configurable | `src/auth/sessions.rs` | deferred | Config surface expansion; future phase |
| F027 | WARNING | SINGLE | DriftCurrentResponse.redaction_sites always 0 | `src/http_axum/drift_routes.rs` | deferred | Requires DB schema change to store redaction_sites count |
| F029 | WARNING | SINGLE | ApiError::BadRequest not in Phase 9.11 spec | `src/http_axum/auth_routes.rs` | wont_fix | Implementation is correct (400 semantics); spec needs updating |
| F030 | WARNING | SINGLE | health_handler in module root not health.rs | `src/http_axum.rs` | wont_fix | Minor structural deviation; no functional impact |
| F031 | WARNING | SINGLE | Cookie header unwrap without safety comment | `src/auth/sessions.rs` | wont_fix | Already uses unwrap_or_else with fallback + comment |
| F032 | WARNING | SINGLE | Audit sort by occurred_at not ULID | `src/sqlite_storage.rs` | wont_fix | Already sorts by id DESC (ULID) |
| F033 | WARNING | SINGLE | LIKE wildcard escape missing | `src/sqlite_storage.rs` | wont_fix | Already uses ESCAPE '\\' in all LIKE clauses |
| F034 | WARNING | SINGLE | Dynamic SQL via format! | `src/sqlite_storage.rs` | wont_fix | format! used only for LIKE parameter values, not SQL fragments |
| F035 | WARNING | SINGLE | CIDR validation at mutation boundary | `src/http_axum/mutations.rs` | deferred | Complex new validation feature; learnings pattern tracked |
| F036 | WARNING | SINGLE | std::sync::Mutex in async test doubles | `tests/` | wont_fix | Only in non-async #[test] blocks; no poisoning risk |
| F038 | SUGGESTION | SINGLE | Login audit actor reveals username | `src/http_axum/auth_routes.rs` | wont_fix | Superseded by F018 |
| F039 | SUGGESTION | SINGLE | Token rate_limit_qps field never enforced | `src/http_axum/auth_middleware.rs` | deferred | Significant new leaky-bucket feature; Phase 10+ |
| F041 | SUGGESTION | SINGLE | Shadowed correlation_id variable | `src/http_axum/drift_routes.rs` | deferred | Style; address in F002 proper implementation pass |
| F042 | SUGGESTION | SINGLE | Near-identical adopt/reapply code | `src/http_axum/drift_routes.rs` | deferred | Extract helper after adopt is properly implemented (post-F002) |
| F043 | SUGGESTION | SINGLE | No password complexity beyond length >= 12 | `src/http_axum/auth_routes.rs` | wont_fix | Out of Phase 9 spec scope |
