---
track: knowledge
problem_type: best-practice
title: "Use a dedicated non-zero exit code for runtime task panics"
slug: task-panic-distinct-exit-code
date: 2026-05-07
phase_id: "1"
generalizable: true
one_sentence_lesson: "Task panics are a distinct operational failure mode from clean shutdown — exit with a dedicated non-zero code (not 0) so monitors and process managers can distinguish a panic restart from a normal stop."
tags: [rust, exit-code, panic, tokio, process-manager, observability, signals]
---

## Context

A Tokio daemon uses a `JoinSet` to drain background tasks on shutdown. When the join set collects a `JoinError` (task panicked), the daemon needs to communicate this to the OS and any process manager that supervises it.

## What Happened

The initial implementation returned `ExitCode::StartupPreconditionFailure` (exit 3) on task panic — the same code used for "Caddy unreachable" or "storage directory not writable". This made runtime panics indistinguishable from pre-startup failures in logs and alerting. The fix introduced `ExitCode::RuntimePanic = 70` (matching the `sysexits.h` spirit: 70 = software error). The drain loop now distinguishes a panicked task from a clean `Ok(())` return and maps accordingly.

## Lesson

> Task panics are a distinct operational failure mode from clean shutdown — exit with a dedicated non-zero code (not 0) so monitors and process managers can distinguish a panic restart from a normal stop.

## Applies When

- Designing exit code tables for daemon processes
- Mapping `JoinError`, panic payloads, or `catch_unwind` results to exit codes
- Writing systemd unit files or process-manager configs that act on exit codes differently (e.g. restart-on-failure vs restart-always)

## Does Not Apply When

- The binary is a short-lived CLI tool where process managers don't supervise it
- All non-zero exits trigger the same restart behavior regardless of cause
