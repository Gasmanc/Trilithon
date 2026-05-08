---
id: duplicate:area::phase-1-glm-review-findings:legacy-uncategorized
category: duplicate
kind: process
location:
  area: phase-1-glm-review-findings
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

# Phase 1 — GLM Review Findings

**Reviewer:** glm
**Date:** 2026-05-06T13:08:00Z
**Diff range:** 3734e02^..be773df -- core/ web/
**Phase:** 1

---

[HIGH] Build script rerun-if-changed paths resolve to non-existent files
File: core/crates/cli/build.rs
Lines: 4-5
Description: `cargo:rerun-if-changed=.git/HEAD` and `.git/refs/heads/` are relative to the package directory (`core/crates/cli/`), not the repository root. They resolve to `core/crates/cli/.git/HEAD` which doesn't exist, so Cargo never re-runs the build script on git HEAD changes. The embedded `TRILITHON_GIT_SHORT_HASH` will be stale after commits that don't touch files under `core/crates/cli/`.
Suggestion: Use `std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.git/HEAD")` or emit the absolute path. Alternatively, compute the git root at build time: `std::process::Command::new("git").args(["rev-parse", "--git-dir"]).output()` and use that path.

---

[WARNING] UtcSecondsLayer and TsWriter timestamp are decoupled
File: core/crates/cli/src/observability.rs
Lines: 100-110 (UtcSecondsLayer), 168-175 (TsWriterGuard)
Description: In the registry chain `.with(fmt_layer).with(UtcSecondsLayer)`, `on_event` is called left-to-right: fmt layer writes and flushes first, then `UtcSecondsLayer::on_event` sets `LAST_TS`. Since `TsWriterGuard::flush()` runs during the fmt layer's `on_event`, `LAST_TS` is always `None`, and `get_or_now_unix_ts()` always falls back to `now_utc()`. The injected `ts_unix_seconds` in JSON output is correct (independently generated), but `UtcSecondsLayer` never contributes to it. The two mechanisms appear coupled but aren't.
Suggestion: Either move `UtcSecondsLayer` before the fmt layer in the registry chain so `LAST_TS` is set before the writer flushes, or remove the `LAST_TS` indirection and have `TsWriterGuard` call `OffsetDateTime::now_utc().unix_timestamp()` directly, documenting that `UtcSecondsLayer` is test-only.

---

[WARNING] Config loaded and immediately discarded in run_daemon
File: core/crates/cli/src/main.rs
Lines: 58-63
Description: `run_daemon` calls `load_config` for validation, then drops the result. `run_with_shutdown()` receives no config. If later slices need the config inside the daemon loop, it will be loaded a second time. The double-load is wasteful and introduces a TOCTOU gap between validation and use.
Suggestion: Pass the loaded `DaemonConfig` into `run_with_shutdown()` (or store it in a shared handle) so validation and use are a single operation. Acceptable as-is for Phase 1 since `daemon_loop` is a no-op.

---

[SUGGESTION] Test fixture uses hardcoded /tmp path shared across test processes
File: core/crates/adapters/tests/fixtures/minimal.toml
Lines: 8
Description: `data_dir = "/tmp/trilithon-test-data"` is a fixed path. `load_config` creates this directory and writes a probe file. Parallel test runs (e.g. `cargo test` with multiple targets) will race on directory creation and probe-file writes. The `data_dir_not_writable` test correctly uses `tempfile::tempdir()`, but `happy_path_minimal` does not.
Suggestion: Have `happy_path_minimal` (and any test using `minimal.toml`) generate a temp directory and override `data_dir` via an env var like `TRILITHON_STORAGE__DATA_DIR`, or create a fixture generator that substitutes a temp path.

---

[SUGGESTION] vitest.config.ts duplicates react plugin from vite.config.ts
File: web/vitest.config.ts
Lines: 1-11
Description: Both `vite.config.ts` and `vitest.config.ts` load `@vitejs/plugin-react`. Vitest already discovers and merges `vite.config.ts` by default (unless `vitest.config.ts` fully replaces it). Having both means the React plugin runs twice during tests.
Suggestion: Either remove the react plugin from `vitest.config.ts` and let Vitest inherit it from `vite.config.ts`, or remove `vite.config.ts`'s plugin and have `vitest.config.ts` be the single source. If the split is intentional (Vitest ignores `vite.config.ts`), add a comment explaining why.

---
## Resolution Log
<!-- appended by review-remediate on 2026-05-07 — do not edit content above this line -->

| # | Finding title | Status | Fix commit | PR | Resolved date | Notes |
|---|--------------|--------|------------|----|---------------|-------|
| 1 | Build script rerun-if-changed paths resolve to non-existent files | ✅ Fixed | `06c9c22` | — | 2026-05-07 | F005 |
| 2 | UtcSecondsLayer and TsWriter timestamp are decoupled | ✅ Fixed | `263af98` | — | 2026-05-07 | F021 |
| 3 | Config loaded and immediately discarded in run_daemon | 🚫 Won't Fix | — | — | 2026-05-07 | False positive — config IS passed to run_with_shutdown (F014) |
| 4 | Test fixture uses hardcoded /tmp path shared across test processes | ✅ Fixed | `1bc3923` | — | 2026-05-07 | F010 |
| 5 | vitest.config.ts duplicates react plugin from vite.config.ts | ✅ Fixed | `c646fb8` | — | 2026-05-07 | F048 |
