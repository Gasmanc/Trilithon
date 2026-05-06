---
track: bug
problem_type: security
root_cause: insufficient-validation
resolution_type: validation-added
severity: high
title: "Redirect URLs must be scheme-validated at the mutation boundary"
slug: redirect-url-scheme-allowlist-mutation-boundary
date: 2026-05-06
phase_id: "4"
generalizable: true
one_sentence_lesson: "Redirect URL validation must reject non-http/https schemes at the mutation boundary — accepting javascript:, data:, or file: schemes enables open redirect to arbitrary protocol handlers"
tags: [rust, security, redirect, url-validation, open-redirect, owasp]
---

## Context

Phase 4's `Route` model includes a `RedirectRule { to: String, status: u16 }`. The `CreateRoute`, `UpdateRoute`, and `ImportFromCaddyfile` pre-conditions did not inspect the `redirects` field at all — any string was accepted and stored into `DesiredState`.

## What Happened

An attacker who can submit mutations could set `redirects.to` to `javascript:alert(1)`, `data:text/html,...`, `//attacker.example`, or any protocol-relative URL. Once stored, the Caddy apply layer would emit this as a redirect response header, turning the proxy into an open redirector. The fix added `check_redirect_url()` in `validate.rs` that parses the URL with `url::Url::parse` and rejects any scheme other than `http` or `https`. It is called from `CreateRoute`, `UpdateRoute`, and `ImportFromCaddyfile` pre-conditions.

## Lesson

> Redirect URL validation must reject non-http/https schemes at the mutation boundary — accepting javascript:, data:, or file: schemes enables open redirect to arbitrary protocol handlers

## Applies When

- A mutation stores a URL that will later be used as a redirect target by a proxy or web server
- The URL is user-supplied or comes from an external data source (Caddyfile import, API payload)
- The storage layer does not validate URL schemes before persisting

## Does Not Apply When

- The redirect URL is constructed internally by the server with no user influence over the scheme
- The system only redirects within the same origin (scheme + host are fixed by the server)
