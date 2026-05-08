---
id: scope:area::phase-2-codex-review-findings:legacy-uncategorized
category: scope
kind: process
location:
  area: phase-2-codex-review-findings
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

# Phase 2 — Codex Review Findings

**Reviewer:** codex
**Date:** 2026-05-07
**Diff range:** HEAD
**Phase:** 2

---

[HIGH] STDERR_READY_SIGNAL_CAN_BLOCK_AND_HANG_TEST
File: core/crates/cli/tests/utc_timestamps.rs
Lines: 70-79
Description: The stderr reader thread uses `mpsc::sync_channel(1)` and calls `ready_tx.send(())` for every JSON line. After the main thread performs a single `recv_timeout`, further sends can block once the one-slot buffer fills, which stops stderr draining and can deadlock/hang the test (and potentially the child process) under moderate log volume.
Suggestion: Signal readiness only once with a non-blocking path (for example, guard with a `sent_ready` flag and skip later sends), or use `try_send` and ignore `Full`, or switch to an unbounded `mpsc::channel()` for the readiness signal.
