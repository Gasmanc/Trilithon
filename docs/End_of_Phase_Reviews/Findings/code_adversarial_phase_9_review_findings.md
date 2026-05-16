# Phase 9 — Code Adversarial Review Findings

**Reviewer:** code_adversarial
**Date:** 2026-05-15
**Diff range:** fd5127f..HEAD
**Phase:** 9

---

[HIGH] SNAPSHOT INSERTED BEFORE APPLY; ORPHANED ROW ON CONFLICT
File: core/crates/adapters/src/http_axum/mutations.rs
Lines: 187-193
Description: `storage.insert_snapshot(snapshot.clone())` is called unconditionally before `applier.apply()`. If the applier returns `OptimisticConflict`, `LockContested`, or `Failed`, the snapshot row is already committed to storage and is never rolled back. `latest_desired_state()` will return this phantom snapshot on the next request, causing subsequent mutations to compute diffs from a state Caddy has never seen.
Suggestion: Either wrap snapshot insertion + apply in a single transaction, or move `insert_snapshot` to after a successful `ApplyOutcome::Succeeded`.

[HIGH] DRIFT ADOPT RE-APPLIES DESIRED STATE INSTEAD OF RUNNING STATE
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: 141-175
Description: The `adopt` endpoint is documented as "accept running state as desired state," but its implementation loads `latest_desired_state()` and re-applies that. It does not read the running state captured in the drift event. The result is that adopt is semantically identical to reapply — an operator choosing adopt to accept a manually changed Caddy config will silently get reapply behaviour.
Suggestion: The adopt path must reconstruct a DesiredState from the running state stored in the drift event row. At minimum, the endpoint should return 501 until this is implemented, rather than silently doing a reapply.

[HIGH] RATE LIMITER KEYED BY FORWARDED IP IS BYPASSABLE; ALSO NEVER EVICTS
File: core/crates/adapters/src/auth/rate_limit.rs
Lines: general
Description: Two issues: (1) ConnectInfo<SocketAddr> gives the direct TCP peer. Behind a reverse proxy (Caddy), all requests arrive from 127.0.0.1 — entire per-IP rate-limiter collapses to a single shared bucket. (2) The DashMap grows without bound — no LRU eviction, no TTL cleanup, no entry cap.
Suggestion: Extract client IP from X-Forwarded-For/X-Real-IP when a trusted-proxy flag is set. Add background eviction task or size cap for stale rate-limit buckets.

[HIGH] BEARER TOKEN AUTHENTICATION BYPASSES must_change_pw ENFORCEMENT
File: core/crates/adapters/src/http_axum/auth_middleware.rs
Lines: 192-202
Description: The `must_change_pw` enforcement block only fires when AuthContext is Session. Token-authenticated callers can reach all Protected endpoints regardless of the must_change_pw flag on the associated user.
Suggestion: Decide whether tokens are subject to must_change_pw. If yes, store user_id on the token row and check the flag. If tokens are explicitly exempt, add a comment documenting that exemption.

[WARNING] CONCURRENT DRIFT RESOLUTION: TOCTOU BETWEEN latest_unresolved AND mark_resolved
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: 127-136, 233-241, 336-344
Description: Two concurrent requests for the same event_id can both pass the id check and both call mark_resolved, writing two audit rows for the same event.
Suggestion: Add idempotency check in mark_resolved that uses a conditional UPDATE WHERE resolved_at IS NULL; zero affected rows means a concurrent resolution won.

[WARNING] DRIFT defer WRITES NO AUDIT ROW FOR THE DEFERRING USER
File: core/crates/adapters/src/http_axum/drift_routes.rs
Lines: 331-358
Description: adopt and reapply write supplemental audit rows crediting the acting session. defer does not — no AuditAppend is written with the actor. An operator deferring drift leaves no audit trail identifying who made the decision.
Suggestion: Write an explicit AuditAppend with the resolving actor before returning 204.

[WARNING] SESSION touch DOES NOT VERIFY expires_at AT MIDDLEWARE LAYER
File: core/crates/adapters/src/http_axum/auth_middleware.rs
Lines: 150-153
Description: The middleware treats touch() returning Some as valid, relying entirely on the SQL predicate. No secondary expiry check is performed on the returned Session struct's expires_at field. Clock step backward or a latent SQL bug admits expired sessions.
Suggestion: After touch() returns Some(session), assert session.expires_at > now_unix at the middleware level as defense-in-depth.

[WARNING] LoginResponse.config_version HARDCODED TO ZERO
File: core/crates/adapters/src/http_axum/auth_routes.rs
Lines: 292-298
Description: Login success response always returns config_version: 0. A client that uses config_version from login to seed expected_version for mutations will always send 0, guaranteeing 409 conflicts on any non-empty system.
Suggestion: Populate config_version from state.storage.latest_desired_state().await during the login handler.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-16 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Snapshot inserted before apply; orphaned row on conflict | ✅ Fixed | `2386697` | — | 2026-05-16 | F006 |
| 2 | Drift adopt re-applies desired state instead of running state | ✅ Fixed | `2386697` | — | 2026-05-16 | F002: returns 501 until get_running_config is wired |
| 3 | Rate limiter keyed by forwarded IP; also never evicts | ✅ Fixed | `2386697` | — | 2026-05-16 | F009 (X-Forwarded-For) + F010 (eviction) |
| 4 | Bearer token auth bypasses must_change_pw | 🔕 Superseded | — | — | — | Excluded — no aggregate finding; enforcement is in session middleware |
