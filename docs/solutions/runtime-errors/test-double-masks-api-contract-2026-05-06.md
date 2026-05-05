---
track: bug
problem_type: integration
root_cause: environment-mismatch
resolution_type: refactored
severity: high
title: "Test double that accepts any body masks incorrect API call semantics"
slug: test-double-masks-api-contract
date: 2026-05-06
phase_id: "3"
generalizable: true
one_sentence_lesson: "When a test double accepts any method/body, incorrect API call semantics can compile and pass tests while silently failing against a real server — verify API call shapes against the actual service documentation before writing the test double"
tags: [testing, test-double, api-contract, caddy, integration, mock]
---

## Context

Phase 3 shipped a `CaddyClient` trait with an in-memory test double used across sentinel, probe, and reconnect tests. The double recorded calls but accepted any HTTP method, any path, and any request body without validation. This is the normal starting point for a test double — you implement only the methods your test needs.

## What Happened

The `patch_config` implementation in `hyper_client.rs` sent an RFC6902 JSON Patch array to Caddy when Caddy's `PUT /config/[path]` endpoint expected the replacement value directly. Because the test double never inspected what it received, the wrong call semantics went undetected through every test in the suite. The bug was only surfaced by a multi-reviewer finding that cross-referenced Caddy's API documentation.

The fix was twofold: correct the production call (add `put_config` using `PUT`), and redesign the test doubles to record `puts` instead of `patches` — making it structurally impossible for the old wrong call to succeed even in tests.

## Lesson

> When a test double accepts any method/body, incorrect API call semantics can compile and pass tests while silently failing against a real server — verify API call shapes against the actual service documentation before writing the test double

## Applies When

- Writing a test double for an HTTP client that wraps a third-party service
- The real service has specific method/path/body shape requirements (not just REST conventions)
- You are adding a new operation to an existing HTTP client trait — check the real API docs before stubbing the double

## Does Not Apply When

- The test double is testing business logic that is independent of the wire format (e.g. retry logic, timeout handling)
- The real service has a spec-compliant REST API where standard HTTP semantics hold (then method checking in the double is noise)
