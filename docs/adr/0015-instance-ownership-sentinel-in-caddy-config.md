# ADR-0015: Mark Caddy ownership with a sentinel object and refuse to proceed without confirmation

## Status

Accepted — 2026-04-30.

## Context

Hazard H12 is concrete and previously seen: "A user with two
Trilithon installations on the same machine pointed at the same
Caddy is a real failure mode. The daemon MUST detect 'another
Trilithon is managing this Caddy' via a sentinel object in Caddy's
configuration (`@id: \"trilithon-owner\"`) and refuse to proceed
without explicit takeover confirmation."

The failure mode is not exotic. A user trialling Trilithon may
install a second copy on the same host (a containerised one
alongside a bare-metal one, a development build alongside a
production build, two sibling deployments aimed at the same Caddy
during migration). Without a sentinel, both daemons drift-detect,
both reconcile, and the running configuration thrashes between two
desired states whose owners do not know about each other. The
audit log on each side reports drift events that the other side
caused.

The mechanism the prompt names is a sentinel object in Caddy's
configuration, addressed by Caddy's `@id` mechanism. Caddy's JSON
admin API supports per-object identifiers (`@id`) that allow
direct addressing through `/id/<id>` paths. Trilithon writes a
single owner sentinel and reads it on startup to determine
ownership.

Forces:

1. **Detection must be cheap and reliable.** A `GET
   /id/trilithon-owner` against Caddy is a single HTTP request
   over the loopback or Unix socket admin channel. The result
   resolves the question.
2. **Detection must not race.** Two Trilithons starting
   simultaneously could both write the sentinel. The write
   must be conditional (compare-and-set semantics) to ensure
   exactly one wins.
3. **Takeover must be possible.** A user who has decommissioned
   one Trilithon and stood up another expects the new one to take
   over without requiring a Caddy reset. Takeover SHALL be an
   explicit, audited action, not a default.
4. **The sentinel must survive Caddy reloads.** The sentinel is
   part of desired state; every applied snapshot includes it.

## Decision

**Sentinel structure.** Trilithon SHALL maintain a sentinel object
in Caddy's configuration at a stable path. The sentinel SHALL carry
the literal Caddy `@id` value `trilithon-owner` so that it is
directly addressable through `GET /id/trilithon-owner`. The sentinel
SHALL include the following fields:

- `@id: "trilithon-owner"` — the Caddy identifier.
- `installation_id` — a UUID that identifies the specific Trilithon
  installation. Generated on first run and persisted in the
  daemon's data directory (alongside the master key location of
  ADR-0014, but not encrypted).
- `display_name` — a human-readable name for the installation
  (default: hostname plus the daemon's data-directory path,
  user-configurable through the web UI).
- `trilithon_version` — the version string of the Trilithon
  daemon that wrote the sentinel.
- `claimed_at_unix` — UTC Unix timestamp of the most recent
  ownership claim (seconds).

The sentinel's exact location in Caddy's configuration tree SHALL
be a Trilithon-managed object that does not affect Caddy's traffic
behaviour. The architecture document SHALL specify the path; this
ADR fixes the contract, not the location.

**Startup ownership check.** On every daemon startup, after the
capability probe (ADR-0013) succeeds, Trilithon SHALL fetch
`/id/trilithon-owner` from Caddy. The behaviour SHALL be:

- **Sentinel absent (404).** Caddy is unowned. Trilithon SHALL
  write the sentinel with its own `installation_id`, log the
  claim through the tracing subscriber, and write an audit row of
  type `instance_ownership_claimed`. Trilithon SHALL proceed to
  normal operation.
- **Sentinel present, `installation_id` matches.** Trilithon owns
  this Caddy. The daemon SHALL update `claimed_at_unix` and
  `trilithon_version` (a normal mutation) and proceed.
- **Sentinel present, `installation_id` does not match.**
  Another Trilithon installation owns this Caddy. The daemon
  SHALL refuse to proceed and SHALL surface a structured error
  through the web UI's startup state (T1.13) and through the
  daemon's tracing output. The error SHALL include: the other
  installation's `display_name` and `installation_id`, the
  `claimed_at_unix` timestamp, and a clear explanation of the
  three resolution paths below. The daemon SHALL NOT make any
  mutation, apply, drift detection, or capability-related write
  in this state.

**Resolution paths.**

1. **Identify and stop the other Trilithon.** The user SHALL stop
   the other daemon, after which the user MAY retry the current
   daemon's startup. The other daemon's audit log preserves the
   record of its tenure; the new daemon does not need to import
   it.
2. **Explicit takeover.** The user MAY click an explicit
   "Take ownership" action in the current daemon's UI (or invoke
   the equivalent `claim_ownership_force` operation through an
   authenticated channel). The action SHALL require typing the
   other installation's `display_name` as a confirmation, SHALL
   produce an audit row of type `instance_ownership_taken_over`
   recording the previous owner's metadata, and SHALL overwrite
   the sentinel.
3. **Stand down.** The user MAY shut down the current daemon and
   continue using the other.

**Audit.** Every ownership transition (claim, takeover, surrender)
SHALL produce an audit row. The audit row SHALL include the
sentinel's contents before and after, the actor, and the
correlation identifier.

**Sentinel preservation.** Every Trilithon-applied snapshot
(ADR-0009) SHALL include the current sentinel. A `POST /load`
that drops the sentinel by accident is a defect; integration tests
SHALL verify that round-tripping a snapshot through `POST /load`
preserves the sentinel.

**Out-of-band sentinel removal.** If a user removes the sentinel
manually (through `caddy admin` or a Caddyfile reload), the next
drift detection cycle (T1.4) SHALL detect the absence and surface
it as a drift event with a recommended action ("re-establish
ownership"). Trilithon SHALL NOT silently re-write the sentinel
without producing the drift event first.

## Consequences

**Positive.**

- The two-Trilithons-one-Caddy failure mode is detected at startup
  with a single HTTP request. Hazard H12 is addressed.
- Takeover is explicit and audited. A user who knowingly migrates
  has a clear path. A user who is in the failure mode by accident
  is told what is happening rather than experiencing thrashing
  reconciliations.
- The sentinel is part of desired state, which means the existing
  snapshot, drift, and audit machinery cover it without special-case
  code.

**Negative.**

- The sentinel adds one Caddy configuration object that exists for
  Trilithon's benefit, not for traffic-serving reasons. Users who
  read their Caddy configuration directly will see the object and
  may be confused. Documentation SHALL explain it.
- The takeover path is a foot-gun if abused (a user could click
  takeover without first stopping the other daemon, producing
  thrashing). The confirmation requirement (typing the other
  installation's name) is the safeguard; users who type past
  warnings have accepted the consequence.
- A bug that drops the sentinel from a snapshot would manifest as
  Trilithon "losing" Caddy ownership on next startup. Integration
  tests are non-optional here.

**Neutral.**

- Multi-instance fleet management (T3.1) extends the model: the
  central controller writes its `installation_id`, the edge agents
  see it. The sentinel structure already accommodates multi-instance
  semantics through the `installation_id` field.
- Open question: whether the sentinel includes a "preferred
  takeover policy" (auto-allow under conditions) for advanced
  users. Recorded as an open question in the architecture
  document, not in this ADR.

## Alternatives considered

**File-based lock on the daemon host.** Use a file lock at
`/var/lib/trilithon/owner.lock` to prevent two daemons on the
same host from running. Rejected because it does not detect the
case of two daemons on different hosts pointing at the same
remote Caddy and because filesystem locks are advisory and
unreliable across containers and bind mounts.

**Caddy admin endpoint authentication binding to one client.**
Configure Caddy to accept admin connections only from one
specific client. Rejected because constraint 11 forbids exposing
Caddy's admin endpoint to a non-loopback interface; the binding
shape is already loopback or Unix socket, where multiple clients
on the same machine can connect equally. Concretely, on Linux
deployments the canonical socket path is `/run/caddy/admin.sock`;
on macOS and Windows development setups the fallback is loopback
TCP `127.0.0.1:2019` with mutual TLS. The binary choice lives in
`config.toml` under `[caddy] admin_endpoint = "..."`.

**Periodic ownership heartbeat.** Have each Trilithon write its
ownership timestamp periodically and treat absence-of-recent-
heartbeat as the other side being gone. Rejected because the
heartbeat introduces races and because explicit takeover is a
better fit for a config plane than implicit timeout-driven
takeover.

**No detection; rely on user discipline.** Document the failure
mode and trust users not to do it. Rejected because hazard H12
names the failure mode as "real" and because the detection cost
is one HTTP request.

## References

- Binding prompt: `../prompts/PROMPT-spec-generation.md#7-edge-cases-and-known-hazards`,
  hazard H12; section 4 features T1.4, T1.13; section 6 feature
  T3.1.
- ADR-0001 (Caddy as the only supported reverse proxy).
- ADR-0002 (Caddy JSON Admin API as source of truth).
- ADR-0009 (Immutable content-addressed snapshots and audit log).
- ADR-0012 (Optimistic concurrency on monotonic config_version).
- ADR-0013 (Capability probe gates optional Caddy features).
- Caddy documentation: "API endpoints — `/id/<id>`," Caddy 2.8.
