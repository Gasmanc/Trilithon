---
track: bug
problem_type: security
root_cause: wrong-assumption
resolution_type: guard-added
severity: high
title: "Rate limiters behind a proxy must key on X-Forwarded-For, not TCP peer"
slug: rate-limit-ip-behind-proxy-x-forwarded-for
date: 2026-05-16
phase_id: "9"
generalizable: true
one_sentence_lesson: "Rate limiters and IP-keyed logic must read X-Forwarded-For behind trusted proxies or all traffic collapses to one shared bucket"
tags: [security, rate-limiting, proxy, http]
---

## Context

The login rate limiter keyed on `ConnectInfo<SocketAddr>.ip()` — the direct TCP peer. Behind a reverse proxy (Caddy), all login requests arrive from `127.0.0.1`, collapsing every per-IP bucket into one shared bucket for all clients.

## What Happened

Five failed logins from any single IP would lock out all users on the server. The fix added a `trusted_proxy: bool` field to `AppState` and extracted a `resolve_client_ip` helper that reads the outermost value of `X-Forwarded-For` when the flag is set, falling back to the TCP peer otherwise. The flag defaults to `false` to avoid trusting spoofed headers on direct-internet deployments.

## Lesson

> Rate limiters and IP-keyed logic must read X-Forwarded-For behind trusted proxies or all traffic collapses to one shared bucket

## Applies When

- The server runs behind a reverse proxy (Caddy, nginx, load balancer)
- Any per-IP enforcement (rate limits, geo-blocks, allow-lists) is in play
- `ConnectInfo<SocketAddr>` is the source of the client IP

## Does Not Apply When

- The server is directly internet-facing (no proxy) — in that case `X-Forwarded-For` must NOT be trusted as it is trivially spoofable
- IP is only used for logging, not enforcement
