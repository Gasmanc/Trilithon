---
track: bug
problem_type: security
root_cause: hardcoded-value
resolution_type: config-driven
severity: high
title: "Security-sensitive HTTP flags must be config-driven, not hardcoded"
slug: security-flags-from-config-not-hardcoded
date: 2026-05-16
phase_id: "9"
generalizable: true
one_sentence_lesson: "Security-sensitive HTTP flags (Secure on cookies, HSTS, etc.) must be driven by runtime config, not hardcoded to the insecure default"
tags: [http, cookies, security, config]
---

## Context

`set_cookie_header` hardcoded `build_cookie(..., false)` — the `Secure` flag was always absent. Any deployment terminating TLS at a proxy forwarded plain HTTP to the daemon, making session cookies transmissible over plain HTTP without the Secure flag.

## What Happened

The fix added `secure_cookies: bool` to `AppState`, wired from `config.server.allow_remote`. When `allow_remote = true` the server is internet-accessible and must set `Secure`; when false (loopback-only) the flag is unnecessary. The flag flows through to `build_cookie` at runtime.

## Lesson

> Security-sensitive HTTP flags (Secure on cookies, HSTS, etc.) must be driven by runtime config, not hardcoded to the insecure default

## Applies When

- HTTP security headers or cookie attributes depend on whether TLS is present
- The binary may be deployed in multiple modes (loopback-only vs. internet-facing)
- The flag is a no-op in development but critical in production

## Does Not Apply When

- The service always runs behind TLS (cert is loaded at startup and there is no plain-HTTP mode)
- The flag is always safe to set regardless of deployment context
