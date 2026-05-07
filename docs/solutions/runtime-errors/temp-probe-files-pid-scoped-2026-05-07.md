---
track: bug
problem_type: race-condition
root_cause: concurrency-gap
resolution_type: guard-added
severity: high
title: "Scope temporary probe files to PID to prevent concurrent-process races"
slug: temp-probe-files-pid-scoped
date: 2026-05-07
phase_id: "1"
generalizable: true
one_sentence_lesson: "Temporary probe files used for writability checks must include the PID (or a UUID) to prevent race conditions when multiple processes test the same directory concurrently."
tags: [rust, filesystem, concurrency, probe, tempfile, pid]
---

## Context

A config loader checks that the `data_dir` is writable by creating and removing a sentinel file before the daemon starts. The sentinel was named `.trilithon-write-probe` — a fixed name.

## What Happened

If two Trilithon processes were started simultaneously against the same `data_dir` (misconfiguration), both would attempt to create, check, and remove `.trilithon-write-probe`. One could remove the file while the other was still using it, producing a false `DataDirNotWritable` error or a stale probe file. The fix appended the process ID to the filename: `.trilithon-write-probe.<pid>`. This guarantees each process operates on its own uniquely-named file with no cross-process collision. A more robust alternative is `tempfile::NamedTempFile` (auto-cleans on drop), which was noted as the preferred long-term approach.

## Lesson

> Temporary probe files used for writability checks must include the PID (or a UUID) to prevent race conditions when multiple processes test the same directory concurrently.

## Applies When

- Creating any sentinel or probe file in a shared directory
- Writability checks, lock files, or health-check artifacts with fixed names
- Tests that create files in shared temp directories under parallel execution

## Does Not Apply When

- The file is intentionally a named lock (where exclusivity via a fixed name is the point)
- Only a single process can ever access the directory (enforced by another mechanism)
