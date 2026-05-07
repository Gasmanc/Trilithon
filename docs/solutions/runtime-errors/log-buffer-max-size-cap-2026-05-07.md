---
track: bug
problem_type: performance
root_cause: missing-error-handling
resolution_type: guard-added
severity: medium
title: "Cap in-memory log line buffers to prevent unbounded memory growth"
slug: log-buffer-max-size-cap
date: 2026-05-07
phase_id: "1"
generalizable: true
one_sentence_lesson: "In-memory log-line buffers that accumulate bytes before flush must have an explicit upper bound; unbounded growth will OOM a process if a single event has a very large message field."
tags: [rust, logging, tracing, buffer, memory, observability, oom]
---

## Context

A custom `tracing-subscriber` writer accumulated all bytes written to it in an in-memory `Vec<u8>` buffer before flushing to stderr on event completion. This allowed the writer to prepend a `ts_unix_seconds` field to the final JSON line.

## What Happened

The buffer had no maximum size. A tracing event with an abnormally large field (e.g. a span attribute containing a large serialized struct, or a deeply nested error chain) would grow the buffer without bound before the flush happened. In high-log-volume scenarios with large events, multiple concurrent buffers could exhaust heap memory. The fix added a `MAX_BUF: usize = 64 * 1024` constant; bytes beyond that threshold were silently truncated. The truncation was logged via a `tracing::warn!` on the first overflow per buffer.

## Lesson

> In-memory log-line buffers that accumulate bytes before flush must have an explicit upper bound; unbounded growth will OOM a process if a single event has a very large message field.

## Applies When

- Implementing a custom `tracing_subscriber::fmt::MakeWriter` or similar per-event buffer
- Any pattern that collects bytes in memory before flushing (e.g. batching log events)
- Writing custom structured log formatters

## Does Not Apply When

- The buffer is backed by a ring buffer or bounded channel with a fixed maximum size already enforced by the data structure
- The writer is guaranteed to never be called with events above a known size limit
