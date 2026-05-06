---
track: bug
problem_type: api-contract
root_cause: insufficient-validation
resolution_type: validation-added
severity: high
title: "CIDR strings must be validated at the mutation boundary, not at apply time"
slug: cidr-validate-at-mutation-boundary
date: 2026-05-06
phase_id: "4"
generalizable: true
one_sentence_lesson: "CIDR strings accepted at the API boundary must be validated at mutation time — invalid notation stored in DesiredState cannot be caught by Caddy until config push fails at apply time"
tags: [rust, validation, cidr, ip, matcher, mutation-boundary]
---

## Context

Phase 4's `CidrMatcher(pub String)` newtype accepted any string. `MatcherSet.remote` is a list of `CidrMatcher` entries specifying IP ranges that a route should match. The `CreateRoute` and `UpdateRoute` pre-conditions did not inspect `MatcherSet.remote` entries.

## What Happened

An invalid CIDR string (e.g. `"not-a-cidr"`, `"256.1.2.3/24"`, `"10.0.0.0/33"`) was accepted into `DesiredState` and stored. The error was only discovered when Caddy attempted to apply the configuration — producing an opaque Caddy-level error rather than a clear rejection message at mutation time. The fix added `check_matchers_valid()` in `validate.rs` that calls `parse_cidr()` for each `CidrMatcher`. `parse_cidr()` splits on `/`, parses the address with `std::net::IpAddr`, and validates the prefix length (`0..=32` for IPv4, `0..=128` for IPv6) — without adding new dependencies.

## Lesson

> CIDR strings accepted at the API boundary must be validated at mutation time — invalid notation stored in DesiredState cannot be caught by Caddy until config push fails at apply time

## Applies When

- A model field wraps a string that represents structured network data (CIDR, IP address, hostname, port)
- The field is accepted from an external mutation payload and stored in desired state
- Validation of the field is deferred to an apply layer (Caddy, Nginx, iptables) rather than performed at intake

## Does Not Apply When

- The field is typed as a concrete type that cannot represent invalid values (e.g. `std::net::Ipv4Addr`, `ipnet::IpNet`)
- Validation is performed by the struct constructor and the type cannot be constructed with an invalid value
