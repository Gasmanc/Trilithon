---
track: bug
problem_type: security
root_cause: insufficient-validation
resolution_type: validation-added
severity: high
title: "User-supplied server-side request URLs must reject loopback and RFC 1918 addresses"
slug: ssrf-user-url-reject-private-addresses
date: 2026-05-06
phase_id: "4"
generalizable: true
one_sentence_lesson: "Any user-supplied URL that triggers a server-side HTTP request (ask URL, webhook, etc.) must be validated to reject loopback and private-range addresses — otherwise it is an SSRF vector"
tags: [rust, security, ssrf, url-validation, loopback, rfc1918, owasp]
---

## Context

Phase 4's `TlsConfig.on_demand_ask_url` is an optional URL that Caddy queries before issuing a certificate for a hostname in on-demand TLS mode. The `SetTlsConfig` pre-conditions did not inspect `patch.on_demand_ask_url`. Any string was accepted into `DesiredState`.

## What Happened

A compromised client or operator could set `on_demand_ask_url` to `http://169.254.169.254/latest/meta-data/` (cloud IMDS), `http://127.0.0.1:8080/admin`, or any RFC 1918 / loopback address. Once applied, Caddy would query that URL for every on-demand TLS certificate request, functioning as an SSRF proxy. An always-200 response at that internal URL would also allow certificate issuance for arbitrary domains. The fix added `check_on_demand_ask_url()` requiring `https` scheme and rejecting loopback (`127.0.0.0/8`, `::1`) and RFC 1918 ranges (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`) via `is_loopback_or_private()`.

## Lesson

> Any user-supplied URL that triggers a server-side HTTP request (ask URL, webhook, etc.) must be validated to reject loopback and private-range addresses — otherwise it is an SSRF vector

## Applies When

- A configuration field accepts a URL that the server will fetch on behalf of an action (webhook URL, health-check URL, ACME ask URL, OAuth callback)
- The URL is persisted and later used by a server-side HTTP client
- The service runs in a cloud or internal environment with accessible metadata or internal APIs at private addresses

## Does Not Apply When

- The URL is only ever displayed to the user and never fetched server-side
- The server that makes the request runs in an isolated network with no internal services reachable at private addresses
