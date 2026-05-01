# ADR-0013: Gate optional Caddy features behind a capability probe

## Status

Accepted — 2026-04-30.

## Context

Caddy's plug-in architecture means that a running Caddy is one of many
possible Caddies. Stock Caddy does not include `caddy-ratelimit`,
Coraza (web application firewall), `layer4`, `forward_auth` plug-ins,
or several other modules Trilithon's policy presets (T2.2) and Tier 3
sketches (T3.2 WAF, T3.3 rate limiting, T3.4 forward-auth, T3.5 layer
4) reference. A user who installed Caddy from their distribution's
package manager has different capability than a user who built Caddy
with `xcaddy` against a curated module set.

Trilithon must do three things at once:

1. Detect what the running Caddy can do.
2. Present features that depend on missing modules as unavailable
   rather than failing at apply time.
3. Allow features that depend on present modules to work without
   per-feature configuration ceremony.

The binding prompt formalises this. T1.11 (Caddy capability probe)
specifies the mechanism. Hazard H5 names the failure mode that the
probe prevents: "Configuration that references a Caddy module not
loaded by the running Caddy will fail at apply. The capability probe
MUST reject such configuration at desired-state validation, not at
apply." T2.2 acceptance: "Presets that depend on optional Caddy
modules are marked as such and degrade gracefully (the route
applies, with the unavailable feature omitted and a warning
surfaced)."

Forces:

1. **Apply-time failure is the worst time to fail.** A user clicking
   "approve" on a route should not encounter "this Caddy does not
   have rate limiting." The validation step must catch the mismatch
   first.
2. **Module presence is queryable.** Caddy exposes `GET /config/apps`
   and module lists through its admin API, which the daemon already
   reaches over loopback or Unix socket.
3. **Probe results change.** A user can upgrade Caddy or swap to an
   `xcaddy`-built binary without restarting Trilithon. The probe
   must be revalidated on Caddy reconnect.
4. **Graceful degradation is preferable to refusal.** A policy preset
   (T2.2) that includes a rate-limit slot SHALL still apply on a
   stock Caddy, with the rate-limit slot omitted and a warning
   surfaced; refusing the entire preset would leave the user unable
   to use the rest of the feature.

## Decision

**The probe.** On daemon startup and on every successful (re)connection
to Caddy's admin endpoint, Trilithon SHALL execute a capability probe
that records the loaded module identifiers reported by Caddy. The
probe SHALL persist its result in memory and SHALL invalidate the
cache on disconnect. The probe result SHALL include at minimum: the
loaded HTTP handler modules, the loaded matcher modules, the loaded
TLS modules, the loaded reverse-proxy upstream modules, and the
loaded rate-limit module if any.

**Capability descriptors.** Trilithon's `crates/core` SHALL define a
typed `CaddyCapability` enumeration covering each capability the
desired-state model can reference. The enumeration SHALL include at
minimum:

- `rate_limit_enforced` (presence of `caddy-ratelimit` or equivalent).
- `web_application_firewall` (presence of Coraza or equivalent).
- `forward_auth` (presence of `forward_auth` HTTP handler).
- `layer4_proxy` (presence of `layer4` global app).
- `bot_challenge` (presence of a Turnstile, hCaptcha, or equivalent
  module).

The enumeration SHALL be extensible; adding a new capability SHALL
NOT require schema migration of the snapshot store (ADR-0009).

**Validation gating (hazard H5).** Trilithon's mutation validator
SHALL consult the capability set before accepting a mutation. A
mutation that references a missing capability SHALL be rejected at
validation, with a typed error that names the missing capability and
the configuration object that requires it. The error message SHALL
include a pointer to documentation explaining how to obtain a Caddy
build with the capability.

**Policy preset behaviour (T2.2).** A policy preset MAY declare
optional capability slots. When attached to a route on a Caddy that
lacks a capability, the preset SHALL apply with the dependent slot
omitted, and Trilithon SHALL surface a warning (not an error) on the
route's view recording the omission. The preset's other slots
SHALL apply normally. The audit log SHALL record the warning so
that a future Caddy upgrade can be paired with a "promote" action
that re-attaches the preset with the now-available capability.

**UI surface (T1.11 acceptance).** Features that require a
currently-missing capability SHALL be marked "unavailable on this
Caddy build" in the web UI and SHALL include a link to documentation
explaining how to enable them. Features that require a
currently-present capability SHALL appear normally.

**Probe revalidation.** The probe SHALL re-run on Caddy reconnect.
A revalidation that adds capabilities SHALL emit an audit row of type
`capability_added`. A revalidation that removes capabilities SHALL
emit an audit row of type `capability_removed` and SHALL re-validate
the current desired state; if the state now references a removed
capability, Trilithon SHALL surface the situation as a typed error
and SHALL NOT auto-disable the affected configuration object.

**Restore behaviour (hazard H9).** A backup or import (T2.12) whose
snapshots reference capabilities the current Caddy lacks SHALL be
flagged at restore preflight. The user MAY override per-condition
with an audited acknowledgement (consistent with T1.3 rollback
preflight semantics).

## Consequences

**Positive.**

- Apply-time failures due to missing modules are eliminated by
  construction. Hazard H5 is addressed at the validation boundary,
  which is where the user's intent is still recoverable.
- Stock-Caddy users see a coherent product: features that need
  enhanced builds are visibly unavailable, not buggy.
- Enhanced-Caddy users see those features without configuration
  ceremony; the probe surfaces them automatically.
- Caddy upgrades and downgrades are handled gracefully. The probe
  re-runs, capabilities update, and the audit log records the
  change.

**Negative.**

- The probe runs at startup and on every reconnect, which adds
  latency to those events. Caddy's `GET /config/apps` is fast, but
  the probe is non-zero work.
- Capability descriptors must be maintained as Caddy adds modules.
  Trilithon's release cadence SHALL include a checklist item to
  evaluate new Caddy modules for descriptor coverage.
- A misbehaving Caddy that returns inconsistent module lists across
  reconnects could thrash capability state. The architecture
  document SHALL specify a debounce on capability change events.

**Neutral.**

- Tier 3 features (T3.2 WAF, T3.3 rate limiting, T3.4 forward-auth,
  T3.5 layer 4) are gated by the same descriptors. The V1 probe
  surface is therefore the V2/V3 surface; no architecture change
  is required when those features land.
- The mapping from Caddy module identifier to Trilithon capability
  is a translation table maintained in `crates/core`. The table
  is part of the product's surface and SHALL be reviewed in code
  review like any other mapping.

## Alternatives considered

**No probe; trust apply-time errors.** Skip the probe and let
apply failures inform the user. Rejected because hazard H5
explicitly forbids this and because apply-time failure happens
after the user has approved the change, leaving them with a
broken intent and a confusing error.

**Probe at startup only, no reconnect re-probe.** Skip
revalidation. Rejected because Caddy upgrades and binary swaps
are real (a user upgrading from stock Caddy to `xcaddy` with
rate-limit support is a typical case) and requiring a Trilithon
restart to notice would be poor UX.

**Per-feature probe instead of unified probe.** Each feature
runs its own check on use. Rejected because it duplicates work
and produces inconsistent capability state across the product.

**Manual capability declaration in configuration.** Have the user
declare which Caddy modules they have. Rejected because the
information is already available from Caddy and a user-declared
manifest invites stale and incorrect declarations that drift from
reality.

## References

- Binding prompt: `../prompts/PROMPT-spec-generation.md#4-tier-1`,
  feature T1.11; section 5 feature T2.2; section 6 features T3.2,
  T3.3, T3.4, T3.5; section 7 hazards H5, H9.
- ADR-0001 (Caddy as the only supported reverse proxy).
- ADR-0002 (Caddy JSON Admin API as source of truth).
- ADR-0009 (Immutable content-addressed snapshots and audit log).
- Caddy documentation: "API endpoints — `/config/`," Caddy 2.8.
