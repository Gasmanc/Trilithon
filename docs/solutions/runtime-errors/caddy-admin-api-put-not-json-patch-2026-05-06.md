---
track: bug
problem_type: api-contract
root_cause: api-misuse
resolution_type: refactored
severity: critical
title: "Caddy admin API PUT expects replacement value, not RFC6902 JSON Patch"
slug: caddy-admin-api-put-not-json-patch
date: 2026-05-06
phase_id: "3"
generalizable: true
one_sentence_lesson: "Caddy's admin API PUT /config/[path] expects the replacement value directly — not an RFC6902 JSON Patch ops array — so any mutation of live Caddy config must use PUT with the replacement value, not PATCH with a patch document"
tags: [caddy, http-api, json-patch, api-contract, put, patch]
---

## Context

Phase 3 built a `CaddyClient` trait and `hyper_client.rs` implementation for Caddy admin API interaction. The `patch_config` method serialised a `JsonPatch` ops array (`[{"op": "add", "path": "...", "value": ...}]`) and sent it to `PATCH /config/[path]`. Caddy's admin API uses a custom protocol where `PUT /config/[path]` replaces the value at that path directly — the body is the replacement value, not an ops array.

## What Happened

The sentinel creation and takeover code called `patch_config(path, [JsonPatchOp::Add {...}])`, sending a JSON array to Caddy instead of the sentinel object itself. Because the in-memory test double accepted any request body without inspecting it, every test passed. Against a real Caddy 2.8 process the call would either fail (if the path didn't exist) or write a JSON array where a server config object was expected, corrupting the live config.

The fix added a `put_config(path, value)` method to `CaddyClient` that issues `PUT /config/[path]` with the replacement value as the body. All sentinel write operations were migrated to `put_config`.

## Lesson

> Caddy's admin API PUT /config/[path] expects the replacement value directly — not an RFC6902 JSON Patch ops array — so any mutation of live Caddy config must use PUT with the replacement value, not PATCH with a patch document

## Applies When

- Writing any code that mutates the Caddy admin API config tree (`/config/...`)
- Adding new config write operations to `CaddyClient` — always check Caddy's own API docs, not RFC6902
- Reviewing client code that sends arrays to a `PATCH` or `PUT` config endpoint of a third-party service

## Does Not Apply When

- Reading config (`GET /config/...`) — shape is determined by what Caddy returns, not by the client
- Using the `load_config` endpoint (`POST /load`) which takes a full config object, not a path-targeted mutation
