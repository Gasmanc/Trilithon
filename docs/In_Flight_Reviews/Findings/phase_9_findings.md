## Slice 9.1 — Axum HTTP Server Scaffold
**Status:** complete
**Date:** 2026-05-14
**Commit:** cd2e29e

### Implementation Summary
Stood up an axum-based HTTP server in the adapters crate. Defined the
`HttpServer` trait and `HttpServerError` enum in `core/crates/core/src/http.rs`.
Implemented `AxumServer` with `AppState`, `AxumServerConfig`, health handler,
and OpenAPI placeholder in `core/crates/adapters/src/http_axum.rs`. Registered
`GET /api/v1/health` (200 when ready, 503 while starting) and
`GET /api/v1/openapi.json` (static placeholder). Enforced H1 security
mitigation: remote binding rejected unless `allow_remote_binding = true`,
with a `tracing::warn!` emitted when the flag is set.

Wired the server into `run_with_shutdown` via `bind_and_spawn_http` helper in
`crates/cli/src/run.rs`.

### Simplify Findings
Items fixed inline during implementation:

- `json!` macro uses `unwrap` internally — replaced with manual
  `serde_json::Map` construction to satisfy `clippy::disallowed_methods`
- `.map(|ip| ip.is_loopback()).unwrap_or(false)` replaced with
  `.is_ok_and(|ip| ip.is_loopback())` (clippy::map_unwrap_or)
- `AxumServer::new` made `const` (clippy::missing_const_for_fn)
- `run_with_shutdown` exceeded 100-line limit after HTTP wiring — extracted
  `bind_and_spawn_http` helper (clippy::too_many_lines)
- `use trilithon_core::http::HttpServer as _;` moved to module level
  (clippy::items_after_statements)
- Doc comments: added backticks to `OpenAPI`, `BindFailed` references
  (clippy::doc_markdown)
- `tracing::warn!` custom target field removed — caused tracing-test env
  filter to miss the event
- tracing-test feature `no-env-filter` enabled so events from
  `trilithon_adapters` are captured in integration tests (default filter
  only captures events from the test crate itself)
- `raw_logs_contain` helper added to bypass scope-filtering in
  tracing-test — necessary because `span.enter()` guards do not
  persist across `.await` suspension points in async tests
- MutexGuard lifetime narrowed in `raw_logs_contain` to satisfy
  `clippy::significant_drop_tightening`

### Items Left Unfixed
None — all findings were fixed inline.
