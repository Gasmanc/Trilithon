# Phase 9 — Codex Review Findings

**Reviewer:** codex
**Date:** 2026-05-15T00:00:00Z
**Diff range:** fd5127f..HEAD
**Phase:** 9

---

[CRITICAL] HTTP_SERVER_USES_TEST_STUB_STATE
File: core/crates/cli/src/run.rs
Lines: 333-347
Description: The production daemon builds the HTTP server with `stubs::make_test_app_state`, so it never uses the bootstrapped users, real session store, real storage, real applier, drift detector, or populated capability cache. Login will hit `NoopUserStore`, mutations hit `NoopApplier`, and read endpoints use `NoopStorage`.
Suggestion: Construct `AppState` from the real SQLite pool/storage, real `SqliteUserStore`, real `SqliteSessionStore`, real `CaddyApplier`, shared drift detector, and the capability cache populated by `setup_caddy`.

[HIGH] BOOTSTRAP_CAN_LOCK_OUT_ADMIN
File: core/crates/adapters/src/auth/bootstrap.rs
Lines: 114-137
Description: The bootstrap user is created before `bootstrap-credentials.txt` is created. If the credentials file already exists, permissions fail, or the directory becomes unwritable, the function returns an error after creating an admin account with an unknown random password. The next startup skips bootstrap because a user now exists.
Suggestion: Create the credentials file atomically before committing the user, or wrap user creation and file write in rollback-safe logic. At minimum, preflight the target path and remove the created user if credential persistence fails.

[HIGH] DRIFT_RESOLUTION_REAPPLIES_WRONG_STATE
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: 141-259
Description: `adopt` loads the latest desired snapshot and applies it back to Caddy, making it effectively the same as `reapply` instead of adopting the running state. Both paths also call `apply` with an existing snapshot whose `config_version` equals `expected_version`, but `CaddyApplier` expects a new snapshot at `expected_version + 1`, so the real CAS path can fail after Caddy has already been loaded.
Suggestion: For adopt, fetch/convert the running config into a new desired-state snapshot. For reapply, either add a dedicated reload path that does not advance CAS or create a new versioned snapshot and call apply with the current applied version.

[HIGH] DISABLED_USERS_KEEP_SESSION_ACCESS
File: core/crates/adapters/src/http_axum/auth_middleware.rs
Lines: 156-168
Description: The middleware accepts any existing session as long as the user row exists. It does not reject users with `disabled_at` set, so disabling an account only blocks new logins while existing sessions continue to authorize protected endpoints.
Suggestion: Reject sessions for disabled users in the middleware, and preferably revoke that session or all sessions for the user when disabled state is detected.

[HIGH] ENVELOPE_EXPECTED_VERSION_CAN_BE_BYPASSED
File: core/crates/adapters/src/http_axum/mutations.rs
Lines: 178-284
Description: `build_snapshot` ignores the envelope `expected_version`; the pure mutation apply validates only the version embedded in the mutation body. The handler then inserts the snapshot before the applier checks the envelope version. A client can send a current body version with a stale envelope version, receive a conflict, but still persist a new latest desired snapshot.
Suggestion: Use the envelope version as the authoritative concurrency guard before snapshot insertion. Reject mismatches with any body-level version, and only publish snapshots through the successful apply/CAS path or store rejected attempts separately.

[WARNING] LOGIN_RETURNS_STALE_CONFIG_VERSION
File: core/crates/adapters/src/http_axum/auth_routes.rs
Lines: 291-297
Description: `LoginResponse.config_version` is hardcoded to `0`, so clients logging in after any successful apply receive a stale version and will submit invalid `expected_version` values.
Suggestion: Read the current applied config version from storage and return that value in the login response.

[WARNING] API_ERROR_ENVELOPE_IS_NOT_UNIFIED_AND_LEAKS_INTERNALS
File: core/crates/adapters/src/http_axum/auth_routes.rs
Lines: 90-133
Description: `ApiError` responses are inconsistent: some use `code`, others use `error`, and `Internal` returns raw internal error strings to clients. This violates the Phase 9.11 unified error envelope and can expose database or backend details.
Suggestion: Replace ad hoc bodies with a single `{ "code": "...", "detail": ... }` envelope, map internal errors to a stable `internal` code, and log sensitive details server-side only.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-16 — do not edit content above this line -->

See `docs/End_of_Phase_Reviews/Fixed/phase_9_fixed.md` for all fixed findings and `docs/End_of_Phase_Reviews/Unfixed/phase_9_unfixed.md` for deferred/wont-fix items. Key fixes: F002 (adopt 501), F005/F040 (bootstrap atomicity), F006 (snapshot ordering), F008/F009 (cookie/IP), F010 (rate limiter eviction), F015 (capability cache), F018 (timing/audit), F019 (SHA-256 doc), F023 (drift SQL), F028 (env-leak test), F037 (dead code).
