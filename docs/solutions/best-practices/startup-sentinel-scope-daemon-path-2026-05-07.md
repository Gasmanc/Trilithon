---
track: bug
problem_type: correctness
root_cause: scope-creep
resolution_type: refactored
severity: high
title: "Scope startup diagnostic output to daemon/work paths only"
slug: startup-sentinel-scope-daemon-path
date: 2026-05-07
phase_id: "1"
generalizable: true
one_sentence_lesson: "Diagnostic startup sentinels belong on the daemon/work path only — emitting them unconditionally before arg parsing adds noise to every --help and --version invocation."
tags: [cli, ux, stderr, startup, sentinel, clap]
---

## Context

A daemon CLI emits a pre-tracing startup line to stderr so operators can confirm the process started even before the logging subscriber is installed. The line `"trilithon: starting (pre-tracing)"` was written at the very top of `main()`, before argument parsing, making it appear on every invocation including `--help`, `--version`, and fast-exit subcommands.

## What Happened

The pre-tracing write was positioned unconditionally before `Cli::try_parse()`. Integration tests for `version` and `help` had to account for this extra line. The line added noise to scripted invocations (`trilithon version 2>/dev/null`) and broke the "clean stdout/stderr" contract for informational subcommands. The fix moved the write into the `Command::Run` dispatch branch, so only the actual daemon run path emits it. A test asserting the sentinel's presence was updated to invoke the `run` subcommand.

## Lesson

> Diagnostic startup sentinels belong on the daemon/work path only — emitting them unconditionally before arg parsing adds noise to every --help and --version invocation.

## Applies When

- Adding any pre-tracing, pre-config, or "process is alive" stderr output
- Reviewing `main()` for unconditional stderr writes before argument dispatch
- Any logging or sentinel that is only meaningful to the running daemon (not to fast-exit commands)

## Does Not Apply When

- The sentinel is explicitly required on all invocations by spec (e.g. a compliance-required process-start audit log)
- The invocation is always the daemon run path (single-purpose binary with no subcommands)
