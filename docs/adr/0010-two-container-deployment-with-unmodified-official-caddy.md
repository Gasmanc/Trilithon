# ADR-0010: Deploy as two containers with the official Caddy image left unmodified

## Status

Accepted — 2026-04-30.

## Context

Tier 2 feature T2.8 specifies an official `docker-compose.yml` that
runs two containers: the unmodified official Caddy image and a
Trilithon daemon image. The two share a volume for the Unix admin
socket and use a separate volume for Trilithon's SQLite store. The
binding prompt's constraint 3 forbids modifying Caddy: "Trilithon
ships alongside the official Caddy binary or container image.
Trilithon does not fork, patch, or rebuild Caddy." Constraint 11
forbids exposing Caddy's admin endpoint to a non-loopback interface.
Hazard H11 names the Docker socket as a privileged trust boundary
that must remain in the daemon container.

The forces that drove the prompt to this shape:

1. **Update independence.** The Caddy project releases on its own
   cadence. A Trilithon user who needs a Caddy security patch
   should be able to pull `caddy:2.x.y` and restart the Caddy
   container without rebuilding Trilithon. A bundled image would
   couple the two release cycles.
2. **Trust separation.** The Docker socket is a root-equivalent
   capability (hazard H11). Caddy serves untrusted HTTP traffic;
   it has no business holding root-equivalent capability over the
   host. Keeping the socket in the Trilithon daemon container
   (which never serves user-facing HTTP) confines the blast radius.
3. **Admin endpoint posture.** Caddy's admin endpoint is
   unauthenticated by default (hazard H1). The right binding is a
   Unix domain socket on a shared volume between the two containers
   or a loopback bind inside a private network. A two-container
   deployment expresses this naturally; a single-container
   deployment would force the admin endpoint into the same
   container as user-facing HTTP.
4. **Image hygiene.** Trilithon's daemon image is a multi-stage
   Rust build, distroless or scratch-based, under 50 MB (T2.8
   acceptance). Caddy's official image is what Caddy ships. Mixing
   them produces a larger surface and a confused supply chain.

## Decision

Trilithon's official Docker deployment SHALL be a two-container
`docker-compose.yml`:

**Container A — Caddy.** SHALL use an unmodified official Caddy image
(`caddy:<version>` from Docker Hub or the Caddy project's release
artefacts). The image SHALL NOT be rebuilt, repacked, or modified.
The image SHALL NOT have the Docker socket mounted. The image SHALL
expose Caddy's HTTP and HTTPS listeners on the host's public
interfaces as the user requires. The image's admin endpoint SHALL
NOT bind to a non-loopback interface; it SHALL be served on a Unix
domain socket on a shared volume between Container A and Container B.
The canonical socket path is `/run/caddy/admin.sock`. On macOS or
Windows development setups (which lack a usable Unix-socket bind-mount
across the Docker VM boundary in some configurations), the fallback
is loopback TCP `127.0.0.1:2019` with mutual TLS, scoped to the
private inter-container network. Both transports are acceptable; the
binary choice lives in `config.toml` under
`[caddy] admin_endpoint = "..."`.

**Container B — Trilithon daemon.** SHALL use Trilithon's distroless
or scratch-based Rust image, under 50 MB, multi-stage built. The
image SHALL have the Docker socket mounted read-write at
`/var/run/docker.sock` for the discovery loop (T2.1, ADR-0007). The
image SHALL emit a stark first-run warning explaining the trust grant
(hazard H11). The image SHALL bind its web UI (T1.13) to
`127.0.0.1:7878` on the host by default and SHALL bind the typed tool
gateway to the same loopback interface (ADR-0008, ADR-0011).

**Shared volumes.**

- An admin-socket volume SHALL be mounted into both containers,
  containing the Unix domain socket Caddy listens on for admin
  traffic. Permission on the socket file SHALL be set such that
  only the Trilithon daemon's process user can connect.
- A Trilithon data volume SHALL be mounted into Container B only,
  holding the SQLite database, the secrets-vault metadata, and
  the configuration file. Permission SHALL be `0600` on sensitive
  files. Container A SHALL NOT see this volume.
- A Caddy data volume SHALL be mounted into Container A only, for
  Caddy's automatic certificate storage. Container B SHALL NOT
  see this volume; it queries certificate state through the admin
  API (T1.9), not by reading Caddy's on-disk store.

**Networking.** The two containers SHALL share a private Docker
network. The Caddy container SHALL be reachable from Container B
over this network and via the admin socket. The Trilithon web UI
port SHALL bind to `127.0.0.1` on the host by default; opt-in for
remote access is governed by ADR-0011.

**Update procedure.** Updating Caddy SHALL be `docker compose pull
caddy && docker compose up -d caddy`. Updating Trilithon SHALL be
`docker compose pull trilithon && docker compose up -d trilithon`.
Neither operation SHALL require rebuilding the other image.

**Bare-metal parallel.** The bare-metal systemd path (T2.7) SHALL
preserve the same trust separation: Caddy runs as the `caddy` user
managed by Caddy's official packaging, Trilithon runs as the
`trilithon` user, the two communicate over a Unix socket at a
documented path under `/run`, the Docker-socket access (if any) is
the Trilithon user's concern only.

## Consequences

**Positive.**

- A Caddy CVE is a `docker compose pull caddy` away, independent
  of Trilithon's release cadence.
- The Docker socket's blast radius is confined to Container B.
  An attacker who compromises Caddy through user-facing HTTP does
  not gain root-equivalent capability over the host through the
  same compromise.
- The Caddy admin endpoint is on a Unix domain socket shared only
  between the two containers, addressing hazard H1 by construction.
- The official Caddy image is the same image Caddy users without
  Trilithon use. Trilithon does not introduce a parallel,
  unverifiable "Trilithon-Caddy" supply chain.

**Negative.**

- Two-container deployment is more conceptually demanding than a
  single-container "all in one" image would be. The setup
  documentation must guide users through it. The 30-second
  `docker compose up` acceptance criterion (T2.8) constrains the
  cognitive load to one command.
- Volume permissioning is a real source of misconfiguration. The
  `docker-compose.yml` SHALL set permissions explicitly and the
  daemon SHALL verify them on startup, refusing to proceed if
  the admin socket is world-writable.
- Caddy's automatic certificate storage in Container A's data
  volume must be backed up separately from Trilithon's data
  volume. T2.12's backup feature SHALL document this.

**Neutral.**

- Users with strong opinions about a single-container image MAY
  build their own. Trilithon SHALL NOT publish such an image as
  official. Third-party single-container images SHALL NOT be
  endorsed without an ADR.
- The Trilithon daemon image's choice of base (distroless versus
  scratch) is a deployment-engineering concern recorded in the
  architecture document, not in this ADR.

## Alternatives considered

**Single combined image.** Build Trilithon and Caddy together into
one container. Rejected because constraint 3 forbids modifying
Caddy, because the Docker-socket trust grant would extend to Caddy's
HTTP-serving process (hazard H11), and because Caddy and Trilithon
release cadences would couple.

**Sidecar in the Caddy container.** Run Trilithon as an additional
process inside Caddy's container. Rejected because Caddy's official
image runs one process; introducing supervision (s6, dumb-init) into
the official image is a modification, and running a Trilithon process
in a non-official image creates the supply-chain problem the official
image was meant to solve.

**Three containers (Caddy, Trilithon, optional Watchtower-style
auto-updater).** Add an auto-updater. Rejected for V1 because
auto-updating a control plane is a sensitive act that wants explicit
configuration and an audit trail. Out of scope for V1.

**Kubernetes Helm chart as the primary deployment path.** Rejected
because the V1 target audience is single-host deployments
(home-lab, small-business). Helm is appropriate for fleet operation
(T3.1) and is a future-tier concern.

## References

- Binding prompt: `../prompts/PROMPT-spec-generation.md#2-non-negotiable-constraints`,
  items 3, 11; section 5 features T2.7, T2.8; section 7 hazards H1,
  H11.
- ADR-0001 (Caddy as the only supported reverse proxy).
- ADR-0007 (Proposal-based Docker discovery).
- ADR-0011 (Loopback-only by default with explicit opt-in for remote
  access).
- Caddy documentation: official Docker image reference, Caddy 2.8.
