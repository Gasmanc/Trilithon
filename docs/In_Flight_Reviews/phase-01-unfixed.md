## W4/W8/gemini-8 — Signal test sleep(1s) race on CI
**Date:** 2026-05-01
**Status:** open
**Reason not fixed:** The 1-second sleep in `cli/tests/signals.rs:60` and `cli/tests/utc_timestamps.rs:59` is a fragile synchronisation point. The correct fix is a readiness-sentinel mechanism (e.g. a line on stderr that the test process waits for before sending the signal). This requires non-trivial refactoring of the signal integration tests and potentially the daemon startup sequence. Deferred to avoid scope creep in the fix pass; the race is rare in practice on local hardware.

## cc-qwen-2/gemini-4 — allow_remote not enforced
**Date:** 2026-05-01
**Status:** open
**Reason not fixed:** The `allow_remote` field is parsed and stored in `DaemonConfig` but is never checked against the actual bind address. Enforcement requires the HTTP server to be present (Phase 9). This is intentional scaffolding for Phase 1; the field will be validated at server startup in the relevant phase.

## codex-7/gemini-5 — `pub use trilithon_core as core` shadows std `core`
**Date:** 2026-05-01
**Status:** open
**Reason not fixed:** `adapters/src/lib.rs:7` re-exports the core crate under the alias `core`, which shadows the standard library's `core` crate name. This is debatable — within the adapters crate itself the shadowing has no practical effect since the std `core` is accessed through `std::` anyway. Renaming the alias (e.g. `trilithon_core`) would break callers. Deferred pending a decision on the public API shape in a later phase.

## gemini-4 / gemini-11 — boot() vestigial; config loaded and discarded in main
**Date:** 2026-05-01
**Status:** open
**Reason not fixed:** `boot()` in `adapters/src/lib.rs` is not called by `main.rs` and exists only as a scaffold placeholder. Similarly, `main.rs` loads config and discards it (the runtime will load it again). Both are intentional Phase 1 scaffolding; wiring will happen as the adapters and daemon loop grow in later phases.

## gemini-6 / cc-minimax-I1 — `'static` bound on EnvProvider unnecessary
**Date:** 2026-05-01
**Status:** open
**Reason not fixed:** `core/src/config/env.rs` has a `'static` bound on the `EnvProvider` trait that is broader than needed. Relaxing it to a lifetime-bounded version would require changes to all callsites and trait object usage. Low risk; deferred to a later cleanup pass.

## FIX-14 — MSRV mismatch between CLAUDE.md (1.80) and workspace (1.85)
**Date:** 2026-05-01
**Status:** open
**Reason not fixed:** `CLAUDE.md` states MSRV 1.80 but `core/Cargo.toml` correctly sets `rust-version = "1.85"` (Edition 2024 requires it). `CLAUDE.md` is partially managed by `new-project` and the MSRV section is in a regenerated block. The workspace is the source of truth; CLAUDE.md is documentation-only and will be corrected on the next `new-project --resync` run.

## cc-qwen-5 / cc-glm-13 — ffi/Cargo.toml unsafe_code = "allow" without explanation
**Date:** 2026-05-01
**Status:** open
**Reason not fixed:** `core/crates/ffi/Cargo.toml` allows unsafe code because the UniFFI-generated bindings require it. Adding a tracking comment inside `ffi/src/lib.rs` is correct but the FFI crate was not in scope for this fix pass (no reviewer flagged it as CRITICAL). Deferred to the FFI-specific review.

## cc-glm-16 — Removing compile-time env vars in config_show.rs test is a no-op
**Date:** 2026-05-01
**Status:** open
**Reason not fixed:** `cli/tests/config_show.rs:17-18` calls `.env_remove("TRILITHON_GIT_SHORT_HASH")` and `.env_remove("TRILITHON_RUSTC_VERSION")` which are build-time env vars, not runtime ones; removing them from the child process environment has no effect. The correct fix is to ensure those build vars never leak into the test env, or to document why the removes are present. Deferred as low-priority informational item.
