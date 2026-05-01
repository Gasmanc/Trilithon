# ADR-0001: Adopt Caddy as the only supported reverse proxy

## Status

Accepted — 2026-04-30.

## Context

Trilithon is a control plane for a reverse proxy. The first decision a control
plane must make is which proxy it controls. The candidate set in the
operations community is Nginx, HAProxy, Traefik, Envoy, and Caddy. The
project owner has selected Caddy and the binding prompt encodes that
selection as a non-negotiable constraint
(`docs/prompts/PROMPT-spec-generation.md` section 2, item 1).

The forces driving this decision are:

1. **Native automatic certificate management.** Caddy 2 ships an in-process
   ACME client that obtains, renews, and serves certificates without an
   external sidecar. Trilithon's V1 certificate-visibility feature (T1.9)
   reads directly from `GET /config/apps/tls/certificates` rather than
   stitching together state from Certbot, acme.sh, or cert-manager.
2. **JSON Admin API as a first-class control surface.** Caddy exposes a
   structured, schema-described configuration surface over HTTP. Nginx
   and HAProxy require text-template generation and process reload.
   Envoy's xDS surface is more powerful than Caddy's but presupposes a
   fleet, which is out of scope for V1. Traefik's API is read-mostly.
3. **Single binary, no external dependencies.** Caddy is one Go binary.
   Trilithon's two-container deployment (T2.8) and bare-metal systemd
   path (T2.7) both benefit from the absence of a Lua runtime, an
   OpenResty layer, or a sidecar control plane.
4. **Module discoverability.** Caddy's loaded-module list is queryable
   at runtime, which enables the capability probe (T1.11) and lets
   Trilithon gracefully degrade features that depend on optional modules
   (hazard H5).
5. **Permissive licence.** Caddy is Apache-2.0. The licence permits
   redistribution alongside Trilithon without copyleft entanglement.

The hazard register (section 7) records two Caddy-specific concerns this
decision must accept: hot-reload connection eviction (H4) and apply-time
TLS provisioning latency (H17). Both are inherent to Caddy's design and
cannot be engineered away by switching proxy.

## Decision

Trilithon SHALL support exactly one reverse proxy: Caddy, version 2.8 or
later. Trilithon SHALL NOT include code paths, configuration surfaces,
documentation, or marketing copy that suggest support for any other
proxy. Pull requests that introduce abstractions whose only purpose is
to leave room for a non-Caddy proxy SHALL be rejected. Where a
specification or design document refers to "the proxy," the referent
SHALL be Caddy.

The minimum supported Caddy version SHALL be 2.8. Trilithon SHALL record
the running Caddy version in every snapshot (T1.2) so that cross-version
restores can warn (hazard H9).

## Consequences

**Positive.**

- The control-plane domain model collapses around one well-documented
  configuration surface. The desired-state schema mirrors Caddy's JSON
  schema rather than a lowest-common-denominator abstraction.
- The capability probe (T1.11), the module-availability checks at
  validation time (hazard H5), and the policy presets (T2.2) can rely
  on a single, queryable module registry.
- Trilithon ships smaller. There is no plug-in matrix, no proxy
  detection, no proxy-specific dialect translation.

**Negative.**

- Users committed to Nginx or HAProxy cannot adopt Trilithon without
  switching proxy. Trilithon is not a migration tool.
- Caddy bugs, CVEs, and behavioural changes propagate directly to
  Trilithon users. Trilithon's release cadence SHALL track Caddy's
  security advisories.
- The product is bound to the Caddy project's continued maintenance.
  If Caddy's stewardship lapses, Trilithon has no fallback.

**Neutral.**

- Trilithon's typed mutation API (T1.6) is shaped by Caddy's domain
  model. The shape may not transfer to a hypothetical second proxy.
  This is accepted: the prompt forbids that hypothetical (constraint 1).
- Documentation language uses "Caddy" where a more abstract product
  might say "the proxy." This trades abstraction for clarity.

## Alternatives considered

**Nginx.** The most-deployed reverse proxy in the world. Configuration
is a custom directive language; reloads are signal-driven; certificate
management requires Certbot or a similar sidecar; there is no
first-class JSON admin API in mainline Nginx. Rejected because the
control-plane primitives Trilithon needs (typed JSON config, module
introspection, certificate state queries) would have to be synthesised
out of band, eliminating the engineering leverage that motivates
Trilithon.

**HAProxy.** A high-performance load balancer with a Runtime API and the
Data Plane API. The Data Plane API is structured and capable, but
HAProxy's certificate management, request matching, and middleware
ecosystem are weaker than Caddy's for the web-facing reverse-proxy
workload Trilithon targets. Rejected because the user's workload is
HTTP-with-automatic-TLS, which is Caddy's strength.

**Traefik.** A container-aware reverse proxy with a label-driven
configuration model and a read-mostly REST API. Strong Docker
discovery story, weaker control-plane surface than Caddy, and no
write-side admin API on par with Caddy's `POST /load`. Rejected
because Trilithon's mutation model (T1.6) requires a write-side admin
surface; Traefik would force file-mediated round-tripping.

**Envoy.** The control surface of choice for service meshes via xDS.
Powerful, formal, and built for fleet operation. Rejected because
Envoy presupposes a control plane already exists and is poorly
matched to a single-instance home-lab deployment. Trilithon's
single-instance V1 (and T3.1's eventual fleet model) will not need
xDS-grade machinery.

**Multi-proxy abstraction.** A pluggable backend that supports two or
more proxies through a common interface. Rejected because the prompt
forbids it (constraint 1) and because the abstraction tax (lowest
common denominator schema, two test matrices, two capability probes)
would dwarf the value delivered.

## References

- Binding prompt: `../prompts/PROMPT-spec-generation.md#2-non-negotiable-constraints`,
  item 1.
- ADR-0002 (Caddy JSON Admin API as source of truth).
- ADR-0010 (Two-container deployment with unmodified official Caddy).
- ADR-0013 (Capability probe gates optional Caddy features).
- Caddy documentation: "JSON Config Structure" and "API endpoints," for
  Caddy 2.8.
