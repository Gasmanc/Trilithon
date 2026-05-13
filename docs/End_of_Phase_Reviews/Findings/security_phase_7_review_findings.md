# Phase 7 — Security Review Findings

**Reviewer:** security
**Date:** 2026-05-10
**Diff range:** ddda146..HEAD
**Phase:** 7

---

[WARNING] SHELL SUBPROCESS FOR PROCESS LIVENESS CHECK
File: `core/crates/adapters/src/storage_sqlite/locks.rs`
Lines: 144–152
Description: `process_alive` spawns `/usr/bin/kill -0 <pid>` as a child process via `std::process::Command::new("kill")`. The `kill` binary is resolved from `PATH`, not from a fixed path, so a malicious `PATH` entry could substitute a different `kill` binary. An attacker who can manipulate the environment substitutes a `kill` binary that always exits 0, causing stale lock detection to falsely declare all holder pids dead, allowing lock bypass.
Suggestion: Replace the `Command::new("kill")` invocation with `nix::sys::signal::kill(Pid::from_raw(pid), None)` — a direct POSIX syscall without PATH lookup. Promote `nix` from `[dev-dependencies]` to `[dependencies]`.

[WARNING] UNIX SOCKET PATH AND DOCKER CONTAINER ID PASSED UNVALIDATED TO CADDY CONFIG
File: `core/crates/core/src/reconciler/render.rs`
Lines: 357–359
Description: `UpstreamDestination::UnixSocket { path }` and `UpstreamDestination::DockerContainer { container_id, port }` are inserted into the rendered Caddy JSON without any sanitisation. A value containing `../../` style segments in a unix socket path would pass through directly to Caddy's transport layer.
Suggestion: Add a validation function for `UpstreamDestination::UnixSocket` that rejects paths containing `..`, null bytes, or non-absolute paths. For `DockerContainer`, validate that `container_id` matches `[a-zA-Z0-9_.-]{1,128}`.

[WARNING] `nix` DEPENDENCY ONLY IN `[dev-dependencies]` — NOT AVAILABLE IN PRODUCTION CODE
File: `core/crates/adapters/Cargo.toml`
Lines: 39
Description: `nix` is declared under `[dev-dependencies]` only. The production code in `locks.rs` uses `std::process::Command::new("kill")` for the liveness probe precisely because `nix` is unavailable at compile time in non-test builds.
Suggestion: Move `nix` to `[dependencies]` with appropriate feature flags (`signal`, `unistd`) and update `process_alive` to use `nix::sys::signal::kill` directly.

[SUGGESTION] `correlation_id` SILENTLY REPLACED WITH A FRESH ULID ON PARSE FAILURE
File: `core/crates/adapters/src/applier_caddy.rs`
Lines: 424–427
Description: `snapshot.correlation_id.parse::<Ulid>().unwrap_or_else(|_| Ulid::new())` silently generates a new correlation ID when the stored value cannot be parsed. A corrupt or truncated `correlation_id` will cause audit rows to carry a different ID than the rows written by the mutation pipeline, breaking audit trail correlation without any log warning.
Suggestion: Log a `tracing::warn!` when the fallback fires, including the raw `snapshot.correlation_id` value and the `snapshot_id`.

[SUGGESTION] PRESET BODY EMBEDDED INTO CADDY HANDLER WITHOUT SCHEMA VALIDATION
File: `core/crates/core/src/reconciler/render.rs`
Lines: 327–330
Description: `serde_json::from_str::<Value>(&preset.body_json)` deserialises the preset body to an arbitrary `serde_json::Value` and embeds it directly as `"policy"` in the Caddy handler object without structural validation against an allowlist.
Suggestion: Either validate preset body JSON against a restricted key allowlist before storing in `state.presets`, or enumerate the body's top-level keys during the capability check.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-13 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Invalid preset JSON silently discarded | ✅ Fixed | `569b149` | — | 2026-05-13 | |
| 2 | UNIX socket and Docker container_id unvalidated | ✅ Fixed | `569b149` | — | 2026-05-13 | |
| 3 | correlation_id silently replaced with fresh ULID | ✅ Fixed | `569b149` | — | 2026-05-13 | warn log added |
| 4 | process_alive shells out to PATH kill | 🚫 Won't Fix | — | — | — | Already fixed in Phase 7 implementation |
| 5 | Preset body without structural allowlist validation | ⏭️ Deferred | — | — | — | Phase 12+ scope; silent discard fixed by F016 |
