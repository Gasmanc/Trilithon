# Phase 1 — Unfixed Findings

**Run date:** 2026-05-07T00:00:00Z
**Total unfixed:** 11 (6 won't fix · 5 advisories won't fix · 0 deferred · 0 conflicts)

| ID | Severity | Consensus | Title | File | Status | Reason |
|----|----------|-----------|-------|------|--------|--------|
| F014 | WARNING | MAJORITY | DaemonConfig not passed into run_with_shutdown (stale reference) | `core/crates/cli/src/main.rs` | 🚫 Won't Fix | False positive — config IS passed to run_with_shutdown in current code |
| F016 | WARNING | MAJORITY | coerce_value None arm is unreachable dead code | `core/crates/adapters/src/config_loader.rs` | 🚫 Won't Fix | False positive — None arm IS reachable for Serde-defaulted fields absent from TOML |
| F024 | WARNING | MAJORITY | Timestamp captured in get_or_now_unix_ts is racy under async scheduling | `core/crates/cli/src/observability.rs` | 🚫 Won't Fix | Addressed by F021 — timestamp now captured at make_writer() time |
| F031 | WARNING | SINGLE | ConfigError::InternalSerialise variant referenced in docs but missing | `core/crates/adapters/src/config_loader.rs` | 🚫 Won't Fix | False positive — variant does not exist in the codebase |
| F033 | WARNING | SINGLE | daemon_loop signature should return Result not () | `core/crates/cli/src/run.rs` | 🚫 Won't Fix | False positive — current code already returns (), which is correct for Phase 1 |
| F034 | WARNING | SINGLE | trigger_observable test has no timeout, can hang CI | `core/crates/cli/src/shutdown.rs` | 🚫 Won't Fix | False positive — test already uses tokio::time::timeout(100ms) |
| F055 | SUGGESTION | SINGLE | Advisory: version counter overflow risk (learnings_match pattern) | general | 🚫 Won't Fix | Advisory only — no Phase 1 code; apply when version counters are introduced |
| F056 | SUGGESTION | SINGLE | Advisory: SQLite transaction rollback on early exit (learnings_match pattern) | general | 🚫 Won't Fix | Advisory only — no Phase 1 code; apply when SQLite transactions are introduced |
| F057 | SUGGESTION | SINGLE | Advisory: schema version column at model creation (learnings_match pattern) | general | 🚫 Won't Fix | Advisory only — no Phase 1 code; apply when DB schema is introduced |
| F058 | SUGGESTION | SINGLE | Advisory: CIDR validation at mutation boundary (learnings_match pattern) | general | 🚫 Won't Fix | Advisory only — no Phase 1 code; apply when CIDR config fields are introduced |
| F059 | SUGGESTION | SINGLE | Advisory: Caddy admin API uses PUT not JSON Patch (learnings_match pattern) | general | 🚫 Won't Fix | Advisory only — no Phase 1 code; apply when Caddy integration is implemented |
