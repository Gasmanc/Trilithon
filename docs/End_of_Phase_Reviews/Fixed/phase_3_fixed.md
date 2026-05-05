# Phase 3 — Fixed Findings

**Run date:** 2026-05-05
**Total fixed:** 11

| ID | Severity | Title | File | Commit | PR | Date |
|----|----------|-------|------|--------|----|------|
| F001 | CRITICAL | PATCH Semantics Do Not Match Caddy API | `core/crates/adapters/src/caddy/hyper_client.rs` | `189e3d0` | — | 2026-05-05 |
| F002 | HIGH | Sentinel Creation Uses Replace-Only Path Update | `core/crates/adapters/src/caddy/sentinel.rs` | `189e3d0` | — | 2026-05-05 |
| F003 | WARNING | Reconnect Logic Test Incorrectly E2E-Gated | `core/crates/adapters/tests/caddy/reconnect_against_killed_caddy.rs` | `f606ab3` | — | 2026-05-05 |
| F004 | WARNING | localhost Not A Reliable Loopback Indicator | `core/crates/adapters/src/caddy/validate_endpoint.rs` | `b74bee3` | — | 2026-05-05 |
| F005 | WARNING | Replace On Non-Existent Sub-Path In Takeover | `core/crates/adapters/src/caddy/sentinel.rs` | `189e3d0` | — | 2026-05-05 |
| F006 | WARNING | sqlx_err Helper Duplicated | `core/crates/adapters/src/db_errors.rs` | `c3ed136` | — | 2026-05-05 |
| F007 | WARNING | Two ShutdownObserver Traits | `core/crates/core/src/lifecycle.rs` | `9e18cf5` | — | 2026-05-05 |
| F008 | SUGGESTION | conflict_error Embeds Logging Side Effect In Error Builder | `core/crates/adapters/src/caddy/sentinel.rs` | `2931cbe` | — | 2026-05-05 |
| F009 | SUGGESTION | Sentinel Pointer Could Collide With User Servers | `core/crates/adapters/src/caddy/sentinel.rs` | `d734da4` | — | 2026-05-05 |
| F011 | SUGGESTION | Unconditional DB Write On Every Probe | `core/crates/adapters/src/caddy/probe.rs` | `24a7eb6` | — | 2026-05-05 |
| F012 | SUGGESTION | Double TOML Round-Trip In config_loader | `core/crates/adapters/src/config_loader.rs` | `9ee9506` | — | 2026-05-05 |
