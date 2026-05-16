# Phase 9 — GLM Review Findings

**Reviewer:** glm
**Date:** 2026-05-15
**Diff range:** fd5127f..HEAD
**Phase:** 9

---

[CRITICAL] Production HTTP server uses noop stub stores
File: core/crates/cli/src/run.rs
Lines: 345-350
Description: bind_and_spawn_http constructs AppState via stubs::make_test_app_state, wiring NoopStorage, NoopSessionStore, NoopUserStore, and NoopApplier. Every API handler will fail or return stub data in production — real stores are never wired into the HTTP server.
Suggestion: Replace make_test_app_state with a real AppState constructor receiving the already-created SqlitePool, Arc<dyn Storage>, Arc<dyn SessionStore>, Arc<dyn UserStore>, Arc<DriftDetector>, etc. from run_with_shutdown.

[CRITICAL] Bootstrap race condition — duplicate admin accounts possible
File: core/crates/adapters/src/auth/bootstrap.rs
Lines: 88-100
Description: bootstrap_if_empty checks user_count > 0 then separately calls create_user and set_must_change_pw. Two concurrent daemon processes during rolling restart can both observe user_count == 0 and create duplicate admin accounts. A crash between create_user and set_must_change_pw leaves the admin without must_change_pw.
Suggestion: Wrap check-and-create in a SQLite transaction, or combine flag set into the INSERT statement by adding must_change_pw as a parameter to create_user.

[HIGH] Adopt drift handler re-applies desired state instead of capturing running state
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: 113-167
Description: adopt semantically means "accept the running state as new desired state." Handler instead loads latest_desired_state() and re-applies it — identical to reapply semantics.
Suggestion: Fetch the running config from Caddy (get_running_config), build a snapshot from it, insert as new desired state, then apply.

[HIGH] Internal error details leaked to API clients
File: core/crates/adapters/src/http_axum/auth_routes.rs
Lines: general
Description: Every map_err(|e| ApiError::Internal(e.to_string())) sends raw error messages (including database details, table names, column names) in the JSON response body.
Suggestion: Log the full error server-side and return a generic message. Correlate with a request id for debugging.

[HIGH] Rate limiter map grows unbounded — no eviction of stale IP buckets
File: core/crates/adapters/src/auth/rate_limit.rs
Lines: general
Description: LoginRateLimiter::buckets is a DashMap<IpAddr, BucketState> with no eviction or TTL. Every failed login from a unique IP permanently consumes memory.
Suggestion: Add periodic sweep (every 60s via DashMap::retain) removing entries where next_allowed_at_unix is far in the past, or cap map size.

[HIGH] Mutations handler uses empty CapabilitySet — no validation against real Caddy capabilities
File: core/crates/adapters/src/http_axum/mutations.rs
Lines: 281-285
Description: build_snapshot constructs a CapabilitySet with empty modules and zeroed fields. Mutations requiring Caddy modules (e.g. rate limiting) will always pass validation.
Suggestion: Read from state.capability_cache.snapshot() to get real probed capabilities.

[WARNING] Timing side-channel allows username enumeration
File: core/crates/adapters/src/http_axum/auth_routes.rs
Lines: 153-170
Description: When user is not found, handler returns 401 without calling verify_password. When user exists but password is wrong, verify_password runs Argon2id (~0.5s). Attacker can distinguish "user exists" (slow) from "user not found" (fast).
Suggestion: Always call verify_password with a dummy hash when user is not found so both paths take the same time.

[WARNING] Session cookie Secure flag never set even with remote binding enabled
File: core/crates/adapters/src/http_axum/auth_routes.rs
Lines: 107-113
Description: set_cookie_header hardcodes secure: false. When allow_remote_binding = true cookies are still sent without Secure flag.
Suggestion: Pass secure: state.is_remote_binding or derive from bind address.

[WARNING] Login response hardcodes config_version: 0
File: core/crates/adapters/src/http_axum/auth_routes.rs
Lines: 209-213
Description: LoginResponse always sets config_version: 0. Clients relying on this to seed expected_version will always send 0, causing 409 conflicts on non-empty systems.
Suggestion: Query state.storage.current_config_version() or read from latest snapshot.

[WARNING] Drift current handler hardcodes redaction_sites: 0
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: 92-93
Description: DriftCurrentResponse.redaction_sites is always 0. Clients cannot distinguish "no secrets in diff" from "redaction metadata not tracked."
Suggestion: Store redaction_sites in the drift event row or compute at detection time.

[WARNING] Near-identical code in adopt and reapply drift handlers
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: 100-267
Description: adopt and reapply share ~60 lines of identical logic. Only ResolutionKind differs.
Suggestion: Extract shared resolve_drift_with_apply(state, event_id, session, kind) helper.

[WARNING] audit_routes casts since/until from i64 without validating since <= until
File: core/crates/adapters/src/http_axum/audit_routes.rs
Lines: 126-127
Description: No validation that since <= until when both provided. Negative values not rejected.
Suggestion: Validate since <= until and reject negative values, returning 400.

[SUGGESTION] Capabilities handler does linear scan on Vec for feature detection
File: core/crates/adapters/src/http_axum/capabilities.rs
Lines: 62-63
Description: modules.contains(&"http.handlers.rate_limit".to_owned()) creates a String allocation and does a linear scan.
Suggestion: Check has_rate_limit and has_waf on the BTreeSet before converting to Vec.

[SUGGESTION] No password complexity requirements beyond length >= 12
File: core/crates/adapters/src/http_axum/auth_routes.rs
Lines: 314-320
Description: change_password only validates length >= 12 and differs from old password. No character diversity check.
Suggestion: Consider adding minimum diversity check or entropy estimation.

[SUGGESTION] build_snapshot_from_desired is dead code
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: 341-400
Description: Annotated #[allow(dead_code)] with "retained for future use" comment. Violates no-dead-code convention.
Suggestion: Remove it. Reintroduce when adopt needs it.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-16 — do not edit content above this line -->

See `docs/End_of_Phase_Reviews/Fixed/phase_9_fixed.md` for all fixed findings and `docs/End_of_Phase_Reviews/Unfixed/phase_9_unfixed.md` for deferred/wont-fix items. Key fixes: F002 (adopt 501), F005/F040 (bootstrap atomicity), F006 (snapshot ordering), F008/F009 (cookie/IP), F010 (rate limiter eviction), F015 (capability cache), F018 (timing/audit), F019 (SHA-256 doc), F023 (drift SQL), F028 (env-leak test), F037 (dead code).
