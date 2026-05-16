# Phase 9 — Gemini Review Findings

**Reviewer:** gemini
**Date:** 2026-05-15T00:00:00Z
**Diff range:** fd5127f..HEAD
**Phase:** 9

---

[CRITICAL] HTTP_SERVER_MISWIRED_TO_STUBS
File: core/crates/cli/src/run.rs
Lines: 335-339
Description: The `bind_and_spawn_http` function uses `trilithon_adapters::http_axum::stubs::make_test_app_state` instead of constructing a real `AppState` with live database and store dependencies. This means all authenticated endpoints, mutations, and drift resolution operations will return errors or no-op results in the running daemon.
Suggestion: Replace the stub call with manual construction of `AppState` using the real `pool`, `storage_arc`, `caddy_client`, and other dependencies initialized in `run_with_shutdown`.

[CRITICAL] DRIFT_ADOPT_RESOLUTION_LOGIC_ERROR
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: 120-135
Description: The `adopt` handler re-applies the *current desired state* snapshot to Caddy via `state.applier.apply`. In a drift scenario, "Adopting" should mean accepting Caddy's diverged state as the new truth. The current implementation does the opposite — it overwrites Caddy's live state with the old desired state, making it functionally identical to "Reapply".
Suggestion: Implement `adopt` by fetching the running config from Caddy, converting it to a `DesiredState` and `Snapshot`, persisting that snapshot as the new desired state, and then marking the drift as resolved.

[HIGH] SNAPSHOT_JSON_LEAKS_PLAINTEXT_SECRETS
File: core/crates/adapters/src/http_axum/snapshots.rs
Lines: 147-151
Description: The `get_snapshot` handler returns the full `Snapshot` object, which includes the `desired_state_json` field containing plaintext secrets (TLS keys, API tokens). While `diff_snapshots` and `audit_routes` correctly redact these fields, `get_snapshot` allows any authenticated user to retrieve sensitive configuration in the clear.
Suggestion: Deserialise `desired_state_json` into a `DesiredState`, apply the `SecretsRedactor` to mask sensitive fields, and return the redacted JSON.

[HIGH] NON_ATOMIC_BOOTSTRAP_FLOW
File: core/crates/adapters/src/auth/bootstrap.rs
Lines: 104-142
Description: The bootstrap flow creates the admin user in the database before writing the `bootstrap-credentials.txt` file. If the daemon crashes or the disk is full between these steps, the account is created with a lost random password. On subsequent starts, the daemon skips bootstrap because users exist, leaving the admin permanently locked out.
Suggestion: Use a transaction for user creation and write the credentials file before committing, or write the file to a temporary location and only commit the database change if the file I/O succeeds.

[WARNING] MISSING_SESSION_TOUCH_THROTTLING
File: core/crates/adapters/src/auth/sessions.rs
Lines: 124-148
Description: The `touch` method, called by `auth_layer` on every authenticated request, performs a SELECT and an UPDATE on the `sessions` table. Without throttling, this creates unnecessary write load and contention on the SQLite database, especially for high-frequency API clients.
Suggestion: Only update `last_seen_at` in the database if the current timestamp is significantly newer (e.g., >60 seconds) than the cached value.

[WARNING] BRITTLE_PATH_CLASSIFICATION
File: core/crates/adapters/src/http_axum/auth_middleware.rs
Lines: 40-47
Description: The `classify` function uses exact string literal matches on `request.uri().path()`. This is brittle and will fail for paths with trailing slashes (e.g., `/api/v1/health/`) or if the router's path normalization differs from the middleware's manual check.
Suggestion: Normalize the path (strip trailing slashes) before matching, or use Axum's internal routing state to identify Public/MustChangePassword routes.

[WARNING] RATE_LIMITER_MEMORY_EXHAUSTION
File: core/crates/adapters/src/auth/rate_limit.rs
Lines: general
Description: The `LoginRateLimiter` uses a `DashMap` to track failures by IP but lacks any mechanism to expire or garbage-collect entries. An attacker rotating IP addresses could slowly exhaust the daemon's memory by creating thousands of stale `BucketState` entries.
Suggestion: Implement a time-to-live (TTL) for buckets or cap the `DashMap` size with an LRU eviction policy.

[SUGGESTION] HARDCODED_CONFIG_VERSION_IN_LOGIN_RESPONSE
File: core/crates/adapters/src/http_axum/auth_routes.rs
Lines: 185
Description: The `LoginResponse` returns a hardcoded `config_version: 0`. Returning the actual current `config_version` from storage would be more consistent with the documentation and more useful for clients needing to synchronize their local state.
Suggestion: Fetch the latest `config_version` from `state.storage` during the login flow and include it in the response.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-16 — do not edit content above this line -->

See `docs/End_of_Phase_Reviews/Fixed/phase_9_fixed.md` for all fixed findings and `docs/End_of_Phase_Reviews/Unfixed/phase_9_unfixed.md` for deferred/wont-fix items. Key fixes: F002 (adopt 501), F005/F040 (bootstrap atomicity), F006 (snapshot ordering), F008/F009 (cookie/IP), F010 (rate limiter eviction), F015 (capability cache), F018 (timing/audit), F019 (SHA-256 doc), F023 (drift SQL), F028 (env-leak test), F037 (dead code).
