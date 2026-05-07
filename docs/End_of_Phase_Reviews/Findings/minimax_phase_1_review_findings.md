# Phase 1 — Minimax Review Findings

**Reviewer:** minimax
**Date:** 2026-05-06T13:03:00Z
**Diff range:** 3734e02^..be773df -- core/ web/
**Phase:** 1

---

[SEVERITY] WARNING
File: core/crates/cli/src/run.rs
Lines: 12-14
Description: `daemon_loop` returns `Ok(())` unconditionally, making the `anyhow::Result<ExitCode>` return type misleading. If the daemon body grows in later phases and starts returning errors, the error is silently discarded by `run_with_shutdown`.
Suggestion: Propagate the actual result from `daemon_loop` or assert unreachable/panic if it cannot fail in Phase 1.

---

[SEVERITY] WARNING
File: core/crates/cli/tests/signals.rs
Lines: 79-80
Description: `std::thread::sleep(std::time::Duration::from_secs(1))` gives the daemon 1 second to initialize before sending the signal. Under heavy system load, the process may not have installed its signal handlers yet, causing the test to fail or send the signal to an unprepared process.
Suggestion: Implement a health-ready notification mechanism (e.g., write to a pipe or file) that the test waits for instead of a fixed sleep.

---

[SEVERITY] WARNING
File: core/crates/cli/src/shutdown.rs
Lines: 120
Description: `ShutdownController::trigger` uses `let _ = self.tx.send(true)` which silently ignores any send error. If all receivers have been dropped (which shouldn't happen while the controller lives), the shutdown signal is silently lost.
Suggestion: Log the send result or propagate the error if shutdown notification failure is truly recoverable.

---

[SEVERITY] WARNING
File: core/crates/cli/tests/shutdown.rs
Lines: 53-62
Description: `trigger_observable` test spawns a task and calls `controller.trigger()` immediately after. Without a timeout on the task join handle, if the task is not scheduled within the timeout window (e.g., under heavy load), the test will hang indefinitely rather than fail with a clear message.
Suggestion: Add a timeout wrapper around `handle.await` similar to other tests in this file.

---

[SEVERITY] SUGGESTION
File: core/crates/adapters/src/config_loader.rs
Lines: 182
Description: The write probe file is named `.trilithon-write-probe` — a fixed filename in the data directory. If two Trilithon processes share the same data directory (misconfiguration), they will conflict on the probe file.
Suggestion: Use a random or process-unique filename (e.g., `tempfile` crate) for the probe file.

---

[SEVERITY] SUGGESTION
File: core/crates/adapters/src/config_loader.rs
Lines: 226-227
Description: `coerce_value` falls back to TOML parsing for any unrecognized TOML variant (Array, Table, Datetime). This could silently produce unexpected results if a future TOML type is added to the config schema but not handled in this function.
Suggestion: Match all variants explicitly and return a proper error for unhandled types rather than falling back to ambiguous TOML parsing.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-07 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | daemon_loop returns Ok(()) unconditionally — misleading return type | 🚫 Won't Fix | — | — | 2026-05-07 | False positive — returning () is correct for Phase 1 stub (F033) |
| 2 | Signal test uses fixed sleep instead of readiness probe | ✅ Fixed | `263af98` | — | 2026-05-07 | F015 |
| 3 | ShutdownController::trigger silently ignores send errors | ✅ Fixed | `263af98` | — | 2026-05-07 | F025 |
| 4 | trigger_observable test has no timeout on handle.await | 🚫 Won't Fix | — | — | 2026-05-07 | False positive — test already wraps in tokio::time::timeout(100ms) (F034) |
| 5 | Write probe file uses fixed filename — concurrent processes collide | ✅ Fixed | `263af98` | — | 2026-05-07 | F018 |
| 6 | coerce_value falls back to TOML string parsing for unhandled variants | ✅ Fixed | `c646fb8` | — | 2026-05-07 | F054 |
