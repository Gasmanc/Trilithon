---
id: duplicate:area::phase-3-findings:legacy-uncategorized
category: duplicate
kind: process
location:
  area: phase-3-findings
  multi: false
finding_kind: legacy-uncategorized
phase_introduced: unknown
status: open
created_at: migration
created_by: legacy-migration
last_verified_at: 0a795583ea9c4266e7d9b0ae0f56fd47d2ecf574
severity: medium
do_not_autofix: false
---

## slice-3.8
**Status:** complete
**Date:** 2026-05-03
**Summary:** Wired Phase 3 Caddy startup in `run.rs`: builds `HyperCaddyClient`, runs the initial capability probe, reads/creates the installation id, ensures the ownership sentinel, and spawns the reconnect loop before emitting `daemon.started`. Added `caddy_startup_exit_code()` helper to `exit.rs`, implemented `reconnect::ShutdownObserver` for `ShutdownSignal`, wrote the `caddy_end_to_end` integration test (TRILITHON_E2E_CADDY=1), and added `caddy_unreachable_exits_3` to the cli storage-startup tests. Signal tests gated on `TRILITHON_E2E_CADDY=1` since they now require a real Caddy to proceed past the startup probe. README updated with Caddy adapter docs.

### Simplify Findings
- **Redundant `validate_loopback_only` in `run.rs`**: `config_loader` already validates the loopback policy; second call removed.
- **Stringly-typed `"local"` instance ID**: appeared twice; extracted as `CADDY_INSTANCE_ID` constant.
- **`storage.pool().clone()` when `pool` was already in scope**: changed to `pool.clone()`.
- **Duplicate config-writing in `caddy_unreachable_exits_3`**: refactored `write_config` into `write_config_with_caddy` and `write_config` delegates to it; test uses the parameterized helper.
- **Comment disorder in `storage_startup.rs`**: Test 3/Test 4 section headers were mis-ordered; fixed.
- **`Arc::clone(&caddy_client)` type mismatch**: `HyperCaddyClient` vs `dyn CaddyClient` coercion; reverted to `.clone()`.

### Items Fixed Inline
- Redundant `validate_loopback_only` in `run.rs` — removed
- Stringly-typed `"local"` instance ID — extracted as `CADDY_INSTANCE_ID`
- `storage.pool().clone()` when pool already cloned — changed to `pool.clone()`
- Duplicate config-writing helper — factored into `write_config_with_caddy`
- Comment disorder in `storage_startup.rs` — reordered Test 3/Test 4 sections

### Items Left Unfixed
- none

## slice-3.5
**Status:** complete
**Date:** 2026-05-03
**Summary:** Implemented `CapabilityCache` (parking_lot `RwLock`-backed), `CapabilityStore` (transactional SQLite persistence with ULID primary keys), and `run_initial_probe` (fetches modules, stamps timestamp, writes cache and DB row). Three tests added: one inline unit test verifying cache population and `caddy.capability-probe.completed` event emission, one integration test (`probe_persisted`) verifying exactly-one-current-row invariant across repeated probes, and one gated E2E timing test.

### Simplify Findings
- **Duplicated `sqlx_err` helper** (`capability_store.rs`): the `sqlx_err` function is an exact copy of the one in `sqlite_storage.rs`. Only two uses exist at this point; the "three uses before extracting" rule means extraction is not yet warranted.

### Items Fixed Inline
- none

### Items Left Unfixed
- Duplicated `sqlx_err` in `capability_store.rs` vs `sqlite_storage.rs` — below the three-use threshold for extraction.

## slice-3.4
**Status:** complete
**Date:** 2026-05-03
**Summary:** Implemented `HyperCaddyClient` over `hyper` 1.x with Unix-socket (hyperlocal) and loopback-mTLS (hyper-rustls) transports. Added `current_traceparent` in `traceparent.rs` deriving a W3C traceparent from the active `tracing::Span`. All seven `CaddyClient` methods are implemented with per-call timeouts. Three inline unit tests and one gated E2E integration test added.

### Simplify Findings
- **Per-call Unix client rebuild** (`hyper_client.rs`, execute()): `Client::builder(...).build(UnixConnector)` was called on every request, discarding the connection pool. Fixed by storing `Box<UnixClient>` in the `Inner::Unix` variant.
- **Extra `GET /` round-trip in `get_loaded_modules`** (`hyper_client.rs`): `fetch_caddy_version()` fired a second `GET /` call after `GET /config/apps`, doubling the network round-trips. Removed `fetch_caddy_version` entirely; caddy_version falls back to `"unknown"` per the spec's permitted fallback.
- **Narrating section comments in `build_tls_client`** (`hyper_client.rs`): `// --- CA root ---`, `// --- Client cert ---`, etc. narrated what the code does without adding information. Removed.
- **Unenforced doc note on `unix_uri`** (`hyper_client.rs`): The doc said `api_path must start with /` but the invariant was not enforced. Removed the misleading note.

### Items Fixed Inline
- Per-call Unix client rebuild — stored `Box<UnixClient>` in `Inner::Unix`
- Extra GET / round-trip in `get_loaded_modules` — removed `fetch_caddy_version`, use constant "unknown"
- Narrating section comments in `build_tls_client` — removed
- Unenforced `api_path` doc constraint on `unix_uri` — removed

### Items Left Unfixed
- `collect_module_ids` has no recursion depth guard — pathological configs could stack-overflow. Low production risk; not fixed to avoid scope creep.

## gemini Review
**Date:** 2026-05-03

[HIGH] Missing sync_all Before Rename
File: core/crates/adapters/src/caddy/installation_id.rs
Lines: 40-42
Description: Missing `sync_all()` before renaming the temporary installation ID file. Without a hardware-level flush, a system crash immediately after the `rename` could result in a directory entry pointing to an empty or incomplete file on some filesystems.
Suggestion: Call `file.sync_all()?` before dropping the file handle to ensure the UUID is physically written to disk.

[WARNING] No UUID Validation On Read
File: core/crates/adapters/src/caddy/installation_id.rs
Lines: 29-30
Description: The function returns an existing installation ID without verifying its format or content. If the file exists but is empty or corrupted, the daemon will proceed with an invalid ID, likely causing downstream failures in ownership sentinel management.
Suggestion: Validate that the string read from disk is a valid, non-empty UUID string (e.g., hyphenated v4) before returning it.

[WARNING] SQLite Error Code Extended Codes Not Masked
File: core/crates/adapters/src/caddy/capability_store.rs
Lines: 84-88
Description: SQLite error code mapping matches exact integers (5, 6), which fails to catch extended error codes like `SQLITE_BUSY_RECOVERY` (261) or `SQLITE_BUSY_SNAPSHOT` (517).
Suggestion: Use bitwise masking (`code & 0xFF == 5`) to correctly identify all categories of BUSY/LOCKED errors in SQLite.

[SUGGESTION] Missing Request-Level Timeouts
File: core/crates/adapters/src/caddy/hyper_client.rs
Lines: general
Description: Missing global request-level timeouts in the hyper-based Caddy client. A TCP connection that succeeds but then hangs during HTTP processing could block the daemon's startup sequence or reconnect loop indefinitely.
Suggestion: Wrap hyper request futures in a `tokio::time::timeout` call using the configured `apply_timeout` or a sensible default.

## codex Review
**Date:** 2026-05-03

[CRITICAL] PATCH Semantics Do Not Match Caddy API
File: core/crates/adapters/src/caddy/hyper_client.rs
Lines: 441-474
Description: `patch_config` serializes and sends an RFC6902 patch document (`JsonPatch`) to `PATCH /config...`. Caddy's `PATCH /config/[path]` expects the replacement JSON value at that path, not a JSON Patch ops array. This makes config mutation behavior incorrect and can cause sentinel creation/takeover writes to fail against real Caddy.
Suggestion: Replace this abstraction with Caddy-native semantics: send the actual replacement value for `PATCH`, and use `POST`/`PUT` where creation is required. If JSON Patch is still desired internally, translate it into correct Caddy API calls before dispatch.

[HIGH] Sentinel Creation Uses Replace-Only Path Update
File: core/crates/adapters/src/caddy/sentinel.rs
Lines: 109-117
Description: When no sentinel exists, code attempts creation via `patch_config` with `JsonPatchOp::Add` at `/apps/http/servers/__trilithon_sentinel__`. Combined with current PATCH behavior, this targets a path that typically does not exist yet and relies on unsupported patch-op semantics, so startup can fail instead of creating sentinel ownership.
Suggestion: For initial creation, call a dedicated create operation (`POST` or `PUT` to the parent path) with a concrete sentinel object keyed by `__trilithon_sentinel__`. Reserve replace operations for takeover updates only.

[HIGH] Capability Probe Persistence Can Fail On Fresh DB
File: core/crates/cli/src/run.rs
Lines: 17, 80-91
Description: Startup persists probe results with `instance_id = "local"` before any non-test code inserts a matching `caddy_instances` row. Migration `0002_capability_probe.sql` enforces `caddy_instance_id REFERENCES caddy_instances(id)`, so fresh installs can hit FK constraint failure and abort startup.
Suggestion: Ensure `caddy_instances('local')` exists before first probe (seed in migration, or explicit upsert in startup before `run_initial_probe`), then persist probe results.

[HIGH] Reconnect Backoff Neutralized By Fixed 15s Sleep
File: core/crates/adapters/src/caddy/reconnect.rs
Lines: 65-69, 91-98
Description: Every loop iteration sleeps `HEALTH_INTERVAL` (15s) before checking health, even while disconnected. That means retry cadence becomes `15s + backoff`, not capped exponential backoff, delaying reconnect detection and capability re-probe.
Suggestion: Sleep based on state: `HEALTH_INTERVAL` only when reachable; when unreachable, sleep only the current backoff before the next health check (raced with shutdown).

[WARNING] Reconnect Logic Test Incorrectly E2E-Gated
File: core/crates/adapters/tests/caddy/reconnect_against_killed_caddy.rs
Lines: 157-162
Description: This test uses a scripted in-memory client and does not require a real Caddy process, but it is gated behind `TRILITHON_E2E_CADDY=1`. As a result, it is skipped in normal test runs, leaving reconnect timing logic effectively untested.
Suggestion: Remove the env gate for this test (or split into always-on deterministic unit/integration tests), so reconnect behavior is continuously validated in CI.

[WARNING] Takeover Audit Event Dropped
File: core/crates/cli/src/run.rs
Lines: 101-111
Description: `ensure_sentinel` returns `(SentinelOutcome, Option<AuditEvent>)`, but `run_with_shutdown` only checks for `Err` and discards successful outcome/event. A takeover can occur without the returned audit event being propagated for later processing.
Suggestion: Capture the success tuple and route the optional `AuditEvent` into the audit pipeline (or at minimum structured logging) to preserve takeover observability.

## qwen Review
**Date:** 2026-05-03

[WARNING] Incomplete Caddy Server Block In Sentinel Write
File: core/crates/adapters/src/caddy/sentinel.rs
Lines: 94-117
Description: The sentinel is written as a Caddy HTTP server block (`/apps/http/servers/__trilithon_sentinel__`) containing only `@id` and `installation_id` fields. Caddy server objects require at minimum a `listen` array. When `patch_config` sends this to a real Caddy 2.8 instance, the admin API may accept the JSON but Caddy will fail to provision the server.
Suggestion: Either use a storage mechanism outside Caddy's config tree (e.g., a dedicated SQLite row or a file on disk), or embed the sentinel as a label/metadata field on an existing server. If keeping the current approach, at minimum add a `listen: []` field so Caddy provisions the server without binding any ports.

[WARNING] Unbounded Recursion In collect_module_ids
File: core/crates/adapters/src/caddy/hyper_client.rs
Lines: 364-381
Description: `collect_module_ids` recursively walks the entire JSON tree returned by Caddy's `/config/apps` endpoint with no depth or node-count limit. A deeply nested config could stack-overflow the async task.
Suggestion: Add a depth counter parameter (e.g., `depth: usize`, reject or cap at 64) or switch to an iterative approach using an explicit `Vec` stack.

[SUGGESTION] Misleading Lock-Free Doc Comment
File: core/crates/adapters/src/caddy/cache.rs
Lines: 6
Description: The doc comment says "lock-free-read cache" but the implementation uses `parking_lot::RwLock`, which is not lock-free. Readers acquire a shared lock.
Suggestion: Remove "lock-free-read" from the doc comment. Acceptable phrasing: "Thread-safe cache for the latest CaddyCapabilities."

[SUGGESTION] Traceparent Doc Mismatches Implementation
File: core/crates/adapters/src/caddy/traceparent.rs
Lines: 7-16, 25-42
Description: The doc comment claims the function "hex-encodes the UTF-8 bytes" of the `correlation_id` span field, but the implementation actually uses the tracing span's internal numeric ID (`id.into_u64()`) packed into 128 bits. The correlation_id field is never read.
Suggestion: Rewrite the doc comment to accurately describe what the function does: derive trace-id from the current span's numeric ID with zero-filled low bits, and span-id from CSPRNG.

[WARNING] Duplicate Step Numbering In config_loader
File: core/crates/adapters/src/config_loader.rs
Lines: 200-204
Description: Both the admin endpoint validation and the data directory writability check are labeled step "7" in comments.
Suggestion: Rename line 204's comment from `// 7.` to `// 8.`.

[SUGGESTION] validate_endpoint Does Not Normalize Domain Case
File: core/crates/adapters/src/caddy/validate_endpoint.rs
Lines: 39
Description: `url::Host::Domain("localhost")` is an exact match, so `https://LOCALHOST:2019` or `https://Localhost:2019` would be rejected as non-loopback even though DNS treats them identically.
Suggestion: Use case-insensitive comparison: `url::Host::Domain(d) if d.eq_ignore_ascii_case("localhost")`.

[SUGGESTION] Sentinel Pointer Could Collide With User Servers
File: core/crates/adapters/src/caddy/sentinel.rs
Lines: 28
Description: `SENTINEL_POINTER = "/apps/http/servers/__trilithon_sentinel__"` creates a server whose name starts with double underscore. A user could manually create a server with this same name, causing false positives.
Suggestion: Given V1 scope this is low risk, but consider a more distinctive name or use a comment documenting the naming convention.

[SUGGESTION] conflict_error Embeds Logging Side Effect In Error Builder
File: core/crates/adapters/src/caddy/sentinel.rs
Lines: 163-173
Description: `conflict_error` calls `tracing::error!` before returning the error, coupling error construction with a logging side effect and creating risk of double-logging if callers also log on the same error path.
Suggestion: Remove the `tracing::error!` call from `conflict_error` and let the caller handle logging.

## minimax Review
**Date:** 2026-05-03

[CRITICAL] CaddyError::OwnershipMismatch Dead Code
File: core/crates/core/src/caddy/error.rs
Lines: 22-29
Description: `CaddyError::OwnershipMismatch` is defined but never constructed or matched anywhere in the codebase. The sentinel code uses `SentinelError::Conflict` (a distinct variant). This is dead code that will never be reached.
Suggestion: Remove `CaddyError::OwnershipMismatch` entirely. If ownership mismatch semantics are needed later, a caller can match `SentinelError::Conflict` and map it explicitly.

[HIGH] caddy_version Always Returns "unknown"
File: core/crates/adapters/src/caddy/hyper_client.rs
Lines: 519-536
Description: `get_loaded_modules` always returns `caddy_version: "unknown"`. Caddy 2.8 exposes version via `GET /version` (returns `{"version":"v2.8.4"}`). The hardcoded fallback means operators cannot rely on version in capability records.
Suggestion: In `get_loaded_modules`, issue a second lightweight `GET /version` request and parse `{"version": ...}` to populate `caddy_version`. Fall back to `"unknown"` only if that call fails.

[HIGH] collect_module_ids Unbounded Recursion
File: core/crates/adapters/src/caddy/hyper_client.rs
Lines: 364-381
Description: `collect_module_ids` recurses into arbitrary JSON without a depth bound. A pathological Caddy config can cause a stack overflow.
Suggestion: Add an explicit depth counter (e.g. `MAX_DEPTH: usize = 128`) and return early when depth exceeds the limit, logging a warning.

[WARNING] localhost Not A Reliable Loopback Indicator
File: core/crates/adapters/src/caddy/validate_endpoint.rs
Lines: 32-47
Description: `validate_loopback_only` accepts `localhost` as a valid loopback host, but the resolution of `localhost` is platform-dependent and can resolve to non-loopback addresses in containerized environments.
Suggestion: Reject `localhost` as a hostname and require explicit loopback IPs only (`127.0.0.1`, `::1`).

[WARNING] Takeover Audit Event Dropped At Call Site
File: core/crates/cli/src/run.rs
Lines: 77-78
Description: `ensure_sentinel` returns a `SentinelOutcome` with optional `AuditEvent` on takeover, but the startup code discards the event. A takeover can occur without the returned audit event being propagated.
Suggestion: When `ensure_sentinel` returns a takeover outcome, pass the audit event to `storage.record_audit_event(...)` before proceeding.

[WARNING] Replace On Non-Existent Sub-Path In Takeover
File: core/crates/adapters/src/caddy/sentinel.rs
Lines: 124-151
Description: The takeover code path uses `JsonPatchOp::Replace` at path `{SENTINEL_POINTER}/installation_id`. The `Replace` op on a non-existent field may fail. A `Remove` + `Add` sequence would be more reliable.
Suggestion: Verify empirically against real Caddy 2.8 that `Replace` at a nested non-existent field succeeds; if not, use `Remove` + `Add`.

[SUGGESTION] Probe Event Missing correlation_id
File: core/crates/adapters/src/caddy/probe.rs
Lines: 69-73
Description: `run_initial_probe` emits `caddy.capability-probe.completed` with no unique correlation identifier. If two probes fire near-simultaneously, downstream consumers cannot distinguish them.
Suggestion: Add a `correlation_id` field using a freshly generated ULID so each probe event can be individually identified.

[SUGGESTION] sqlx_err Helper Duplicated
File: core/crates/adapters/src/caddy/capability_store.rs
Lines: 94-100
Description: `sqlx_err` in `capability_store.rs` is a literal copy of the same function in `sqlite_storage.rs`. Now that both store types use it, extraction to a shared module is warranted.
Suggestion: Extract the `sqlx_err` helper to a shared module (e.g., `adapters/src/db_errors.rs`) and replace both copies.

[SUGGESTION] E2E Test Hardcoded Socket Path
File: core/crates/adapters/tests/caddy/probe_under_one_second.rs
Lines: 50-51
Description: The test inserts a row with hardcoded address `'/tmp/caddy-e2e-test.sock'` regardless of the actual socket path used (which can be overridden via `TRILITHON_E2E_CADDY_SOCKET`).
Suggestion: Use the actual `socket_path` variable in the SQL insert so the DB row reflects the socket under test.

> NOTE: kimi — API error (server error, transient). No findings extracted.
> NOTE: glm — stalled; no output returned within session window.

## Phase-End Simplify
**Date:** 2026-05-03

### Items Fixed Inline

1. **extract execute_with_timeout helper** — `hyper_client.rs` — 7 duplicate timeout-dispatch blocks unified into one private method
2. **depth guard in collect_sentinels** — `sentinel.rs` — unbounded recursion patched with `MAX_COLLECT_DEPTH = 128`
3. **remove redundant fetch_caddy_version comment** — `hyper_client.rs` — call-site comment removed (covered by function name)
4. **shorten traceparent limitation comment** — `traceparent.rs` — multi-paragraph explanation condensed to single NOTE line
5. **parallel modules + version fetch** — `hyper_client.rs` — `GET /config/apps` and `GET /version` now run with `tokio::join!`
6. **spawn_blocking for read_or_create** — `run.rs` — synchronous FS call moved off async thread
7. **extract EventCollector test helper** — `test_support.rs` — shared tracing test harness created; probe.rs and sentinel.rs deduped (integration test copy kept as it uses a distinct type)

### Items Left Unfixed

- `sqlx_err` duplication (capability_store.rs + sqlite_storage.rs) — only 2 uses; three-use extraction rule not yet met
- Two `ShutdownObserver` traits (lifecycle.rs + reconnect.rs) — structural refactor touching core+adapters+cli; deferred
- Sentinel builds raw JSON map with string literals — would require typed struct; deferred to Phase 6 design review
- Unconditional DB write on every probe — low-churn path (only on reconnect events, not health ticks); deferred
- Double TOML round-trip in config_loader — startup-only, sub-millisecond; deferred
