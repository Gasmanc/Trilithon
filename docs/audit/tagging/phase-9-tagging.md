# Phase 9 — Tagging Analysis
**Generated:** 2026-05-13
**Model:** opus (extended thinking)
**Documents read:**
- CLAUDE.md (208 lines)
- docs/architecture/architecture.md (1124 lines)
- docs/architecture/trait-signatures.md (734 lines)
- docs/planning/PRD.md (952 lines)
- docs/phases/phase-09-http-api.md (239 lines)
- docs/adr/0009-immutable-content-addressed-snapshots-and-audit-log.md
- docs/adr/0011-loopback-only-by-default-with-explicit-opt-in-for-remote-access.md
- docs/adr/0012-optimistic-concurrency-on-monotonic-config-version.md
- docs/architecture/contracts.md (17 lines, empty registry)
- docs/architecture/contracts-invariants.md (44 lines)
- docs/architecture/seams.md (137 lines; 5 active seams: applier-caddy-admin, applier-audit-writer, snapshots-config-version-cas, apply-lock-coordination, apply-audit-notes-format)
- docs/architecture/contract-roots.toml (37 lines; roots limited to `trilithon_core::reconciler::*`)
- docs/todo/phase-09-http-api.md (1159 lines)
**Slices analysed:** 11

---

## Proposed Tags

### 9.1: `axum` server scaffold, loopback bind, `/api/v1/health`, OpenAPI surface
**Proposed tag:** [cross-cutting]
**Reasoning:** Touches `core/crates/adapters/src/http_axum/{mod.rs,health.rs,openapi.rs}`, modifies `core/crates/adapters/Cargo.toml`, AND modifies `core/crates/cli/src/main.rs` — explicitly crosses the adapters ↔ cli (entry) layer boundary. Introduces the `HttpServer` trait impl per trait-signatures.md §10 and the shared `AppState` struct that every subsequent slice consumes. Establishes the `http.request.received` / `http.request.completed` / `daemon.started` tracing convention that every other slice in this phase emits. References ADR-0011 plus PRD T1.13 plus Hazard H1.
**Affected seams:** PROPOSED: http-server-entry-wiring
**Planned contract additions:** none (HttpServer trait already specified in trait-signatures §10; AppState lives in adapters and is not contract-rooted per contract-roots.toml)
**Confidence:** high

### 9.2: Argon2id password hashing and `users` adapter
**Proposed tag:** [standard]
**Reasoning:** All files under `core/crates/adapters/src/auth/` (passwords.rs, users.rs) plus a Cargo.toml dependency bump (argon2, password-hash). Introduces the `UserStore` trait — but it lives entirely inside the adapters crate, is not referenced cross-layer, and no other slice modifies it. No I/O cross-layer concerns: hashing is pure, persistence is via existing `Storage`. Emits no new audit kinds (9.5 emits the auth audit rows downstream).
**Affected seams:** none
**Planned contract additions:** none (UserStore + password helpers live in `trilithon_adapters::auth::*`, which is not a contract root)
**Confidence:** high

### 9.3: Sessions table writer, cookie codec, login rate limiter
**Proposed tag:** [standard]
**Reasoning:** Confined to `core/crates/adapters/src/auth/{sessions.rs,rate_limit.rs}`. Introduces the `SessionStore` trait and the `LoginRateLimiter` type, both adapter-local. No layer crossing; uses `RandomBytes` and `Storage` already established in earlier phases. No new shared tracing convention or audit kinds.
**Affected seams:** none
**Planned contract additions:** none (SessionStore + LoginRateLimiter live in adapters; not contract-rooted)
**Confidence:** high

### 9.4: Bootstrap-account flow with `bootstrap-credentials.txt` mode 0600
**Proposed tag:** [cross-cutting]
**Reasoning:** Files span `core/crates/adapters/src/auth/bootstrap.rs` AND `core/crates/cli/src/main.rs` — crosses the adapters ↔ cli layer boundary. Introduces a brand-new audit kind `auth.bootstrap-credentials-created` (architecture §6.6 extension) that other phases/dashboards consume. Carries Hazard H13 mitigation invariants (password not in env/args/logs, file mode 0600, must_change_pw flag) that downstream slices (9.5, 9.6) structurally depend on. Adds a `bootstrap = true` extension to the shared `daemon.started` tracing event introduced in 9.1.
**Affected seams:** PROPOSED: bootstrap-credentials-flow
**Planned contract additions:** none (BootstrapOutcome + bootstrap_if_empty live in adapters)
**Confidence:** high

### 9.5: Auth endpoints: login, logout, change-password
**Proposed tag:** [standard]
**Reasoning:** Adds `core/crates/adapters/src/http_axum/auth_routes.rs` and wires routes in `http_axum/mod.rs` — one crate, one module group, three handlers. Uses traits and types established by 9.2/9.3 and the AppState convention from 9.1; does not introduce new traits or modify shared ones. Emits audit kinds `auth.login-succeeded`, `auth.login-failed`, `auth.logout`, `auth.session-revoked`, all already enumerated in architecture §6.6 — no new vocabulary. References one ADR/PRD pair (PRD T1.14, architecture §6.6, §11) — under the 3+ threshold.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high

### 9.6: Authentication middleware (sessions and tokens)
**Proposed tag:** [cross-cutting]
**Reasoning:** Although the file footprint is small (`core/crates/adapters/src/http_axum/auth_middleware.rs`), this slice introduces the `AuthContext` enum and `AuthenticatedSession` axum extractor that EVERY downstream handler in slices 9.7, 9.8, 9.9, 9.10 takes as a parameter. It also establishes the `Public` / `MustChangePassword` / `Protected` path-classification convention plus the 401/403 envelope shape that other slices must conform to. This is precisely the "introduces a convention other slices must follow" criterion. Cross-references the `tokens` table from architecture §6.4 alongside `sessions`, fusing two auth backends behind one trait surface.
**Affected seams:** PROPOSED: http-auth-context-extraction
**Planned contract additions:** none (AuthContext, AuthenticatedSession live in adapters)
**Confidence:** high

### 9.7: `POST /api/v1/mutations` with `expected_version` envelope
**Proposed tag:** [standard]
**Reasoning:** Single file (`core/crates/adapters/src/http_axum/mutations.rs`), single crate. Consumes the already-existing `Applier` trait (Phase 7) and `core::validate::validate_mutation` (existing). Introduces one new audit kind variant `mutation.rejected.missing-expected-version` but it's an extension of the established `mutation.rejected` family in §6.6 used only by this handler. References ADR-0012 plus PRD T1.6/T1.8 plus architecture §6.6/§7.1 — at the edge of the 3+ threshold, but the references are descriptive (consuming existing contracts), not introducing new shared structure.
**Affected seams:** applier-caddy-admin, applier-audit-writer, snapshots-config-version-cas (this slice is the HTTP entrypoint exercising these existing seams; no new boundary)
**Planned contract additions:** none
**Confidence:** medium
**If low confidence, why:** Borderline cross-cutting because three Phase 7 seams converge here; tagged standard because the slice consumes those contracts without modifying them and changes are confined to one file.

### 9.8: Snapshot and route read endpoints
**Proposed tag:** [cross-cutting]
**Reasoning:** Modifies `core/crates/adapters/src/http_axum/snapshots.rs` AND `core/crates/core/src/storage.rs` — explicitly crosses the adapters ↔ core layer boundary. The phase doc flags this directly: "extend `Storage` with `list_snapshots(SnapshotSelector, limit)` ... if not present; flag the trait extension below." The `Storage` trait is the most-shared trait in the codebase (implemented by `storage_sqlite`, consumed by every reconciler, audit-writer, and HTTP handler), so modifying it is the textbook cross-cutting case. Adds a new method that downstream slices and phases (drift, audit, future UI) will use.
**Affected seams:** PROPOSED: storage-snapshot-listing
**Planned contract additions:** `trilithon_core::storage::Storage::list_snapshots` — note: `Storage` is not currently in contract-roots.toml; if this trait is to be tracked, contract-roots.toml needs an entry. Flagging for `/phase-merge-review` to decide.
**Confidence:** high

### 9.9: `GET /api/v1/audit` with paginated filters
**Proposed tag:** [standard]
**Reasoning:** Single file (`core/crates/adapters/src/http_axum/audit_routes.rs`), single crate. Consumes the existing `Storage::tail_audit_log` from Phase 6 (slice 6.6) without modifying it. Uses `AuditEvent::from_str` and `AuditSelector` already established. No new audit kinds, no new shared conventions, no trait changes.
**Affected seams:** none
**Planned contract additions:** none
**Confidence:** high

### 9.10: Drift endpoints: current, adopt, reapply, defer
**Proposed tag:** [standard]
**Reasoning:** Single file (`core/crates/adapters/src/http_axum/drift_routes.rs`), single crate. Consumes Phase 8's existing `DriftDetector::mark_resolved` and `Storage::latest_drift_event` and reuses the mutation pipeline from slice 9.7. No new traits, no layer crossing, no new audit kinds (uses the already-defined `config.drift-resolved`).
**Affected seams:** none (consumes Phase 8 internals; no new boundary)
**Planned contract additions:** none
**Confidence:** high

### 9.11: `GET /api/v1/capabilities` plus error envelope and OpenAPI publication
**Proposed tag:** [cross-cutting]
**Reasoning:** Formalises the `ApiError` enum and `IntoResponse` mapping that every handler in 9.5–9.10 returns — the error envelope is by definition a convention other slices must follow. Adds `core/crates/adapters/src/http_axum/error.rs`, `capabilities.rs`, and the `openapi.rs` `utoipa::OpenApi` derive root that references EVERY handler in the phase (paths list in the slice spec enumerates 14 handlers across seven other modules). The OpenAPI document is also the source of truth for the Phase 11 typed client — explicit downstream-phase structural dependency. References PRD T1.11/T1.13 plus architecture §6.13/§11 plus all earlier slices.
**Affected seams:** PROPOSED: api-error-envelope, PROPOSED: openapi-document-publication
**Planned contract additions:** none (ApiError and ApiDoc live in adapters; not contract-rooted)
**Confidence:** high

---

## Summary
- 5 trivial: 0
- standard: 6 (9.2, 9.3, 9.5, 9.7, 9.9, 9.10)
- cross-cutting: 5 (9.1, 9.4, 9.6, 9.8, 9.11)
- low-confidence: 0 (one medium: 9.7)

## Notes

- **Phase 9 is structurally an HTTP-surface phase**, so the proportion of cross-cutting slices is high. The cross-cutting slices each establish a convention (server scaffold + tracing, bootstrap audit kind, auth extractor, Storage extension, error envelope/OpenAPI) that the rest of the phase or downstream phases consume.
- **No new seams are currently in `seams.md`** that cover the HTTP boundary. Five proposed seams were identified across slices 9.1, 9.4, 9.6, 9.8, 9.11; these should be drafted into `seams-proposed.md` for ratification at `/phase-merge-review`. Suggested IDs:
  - `http-server-entry-wiring` (9.1) — cli wires `HttpServer` impl + AppState
  - `bootstrap-credentials-flow` (9.4) — cli + adapters cooperate on first-run file mode 0600 and H13 invariants
  - `http-auth-context-extraction` (9.6) — middleware → handler extractor convention; sessions + tokens fused
  - `storage-snapshot-listing` (9.8) — `Storage::list_snapshots` extension to a core trait
  - `api-error-envelope` and `openapi-document-publication` (9.11) — uniform error shape; published OpenAPI as Phase 11 client source
- **Contract registry impact:** `docs/architecture/contracts.md` is currently empty and `contract-roots.toml` only roots `trilithon_core::reconciler::*`. The only candidate addition this phase is `trilithon_core::storage::Storage::list_snapshots` (slice 9.8). Decision on whether to root `Storage` lies with `/phase-merge-review`; flagged in the slice entry.
- **Hazard H13** is a strong constraint on slice 9.4 — the four "MUST NOT" clauses (password not in args/env/logs, file mode 0600) make that slice non-negotiably cross-cutting because the invariants are testable from multiple slices and must not regress.
- **No slices in Phase 9 are [trivial]** — every slice either touches a shared trait, crosses a layer, introduces a convention, or emits an audit kind. This is expected for a phase that stands up a whole HTTP surface from scratch.

---

## User Decision
**Date:** 2026-05-13
**Decision:** accepted

### Modifications (if any)
None.

### Notes from user
None.
