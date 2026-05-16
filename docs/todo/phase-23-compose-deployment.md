# Phase 23 — Two-container Docker Compose Deployment — Implementation Slices

> Phase reference: [../phases/phase-23-compose-deployment.md](../phases/phase-23-compose-deployment.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference (`docs/phases/phase-23-compose-deployment.md`).
- Architecture §3 (system context), §11 (security posture), §12.1 (tracing vocabulary), §14 (upgrade and migration).
- Trait signatures: `core::storage::Storage` (audit row append).
- ADRs: ADR-0010 (two-container deployment with unmodified official Caddy), ADR-0011 (loopback-only by default), ADR-0014 (secrets at rest — keychain interaction inside the container).
- PRD: T2.8 (Docker Compose deployment path).
- Hazards: H1 (Caddy admin endpoint exposure), H11 (Docker socket trust boundary), H13 (bootstrap credential leak).

## Slice plan summary

| # | Title | Primary files | Effort (ideal-eng-hours) | Depends on |
|---|-------|---------------|--------------------------|------------|
| 23.1 | Multi-stage Trilithon Dockerfile and `healthcheck` subcommand | `core/Dockerfile`, `core/crates/cli/src/commands/healthcheck.rs` | 6 | — |
| 23.2 | Base `docker-compose.yml` (default profile) | `deploy/compose/docker-compose.yml`, `deploy/compose/trilithon.env.example` | 6 | 23.1 |
| 23.3 | Opt-in Docker-discovery overlay and socket-trust enforcement test | `deploy/compose/docker-compose.discovery.yml`, `deploy/compose/test/lint-no-socket.sh` | 5 | 23.2 |
| 23.4 | First-run Docker socket trust-grant warning emission | `core/crates/cli/src/startup.rs`, `core/crates/core/src/audit.rs` | 5 | 23.1, 23.3 |
| 23.5 | GHCR publish workflow with multi-arch build, cosign signing, and SBOM | `.github/workflows/docker-publish.yml`, `deploy/compose/README.md` (signature-verification heading only) | 8 | 23.1 |
| 23.6 | Compose smoke-test script and 30-second bootstrap timing gate | `deploy/compose/test/smoke.sh`, `deploy/compose/test/verify-caddy-digest.sh` | 6 | 23.2, 23.5 |
| 23.7 | Upgrade-from-prior smoke test and `deploy/compose/UPGRADING.md` rationale paragraph | `deploy/compose/test/upgrade-from-prior.sh`, `deploy/compose/UPGRADING.md` | 5 | 23.5, 23.6 |
| 23.8 | Operator and end-user documentation (compose README and install page) | `deploy/compose/README.md`, `docs/install/compose.md` | 4 | 23.6, 23.7 |
| 23.9 | Wire deployment lints into `just check` and image-size budget into CI | `justfile`, `.github/workflows/docker-publish.yml` | 3 | 23.3, 23.5 |

---

## Slice 23.1 [standard] — Multi-stage Trilithon Dockerfile and `healthcheck` subcommand

### Goal

Trilithon ships a Docker image built by a multi-stage Rust build on a distroless base under 50 MiB, with a `trilithon healthcheck` subcommand that the Compose healthcheck stanza calls. After this slice, `docker build -t trilithon:dev -f core/Dockerfile .` produces a runnable image and `docker run --rm trilithon:dev --version` prints the version.

### Entry conditions

- The Trilithon binary builds cleanly in the workspace under `cargo build --release --workspace --bin trilithon-cli`.
- A working Docker daemon is available on the developer machine.
- The `core/crates/cli` clap derive surface exists (Phase 1).

### Files to create or modify

- `core/Dockerfile` — multi-stage Dockerfile, builder + distroless runtime.
- `core/crates/cli/src/commands/healthcheck.rs` — the `healthcheck` subcommand.
- `core/crates/cli/src/main.rs` — wire the subcommand into the clap derive enum.
- `core/crates/cli/Cargo.toml` — add `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "blocking"] }` if not already present (the healthcheck uses a blocking client so no Tokio runtime is required).

### Signatures and shapes

`core/Dockerfile` (verbatim):

```dockerfile
# syntax=docker/dockerfile:1.7
# Stage 1 — builder
FROM rust:1.80-slim-bookworm AS builder
ENV CARGO_PROFILE_RELEASE_LTO=thin \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
    CARGO_TERM_COLOR=never
RUN apt-get update \
 && apt-get install --yes --no-install-recommends \
        pkg-config \
        libssl-dev \
        clang \
        mold \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY core/ ./core/
WORKDIR /build/core
RUN cargo build --release --workspace --bin trilithon-cli
RUN strip --strip-all target/release/trilithon-cli

# Stage 2 — runtime (distroless)
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /build/core/target/release/trilithon-cli /usr/local/bin/trilithon
USER nonroot:nonroot
WORKDIR /var/lib/trilithon
EXPOSE 7878
ENTRYPOINT ["/usr/local/bin/trilithon"]
CMD ["serve"]
```

`core/crates/cli/src/commands/healthcheck.rs`:

```rust
//! `trilithon healthcheck` subcommand.
//!
//! Used by the Compose healthcheck stanza and by the systemd unit's
//! readiness probe. Exits zero on `200 OK` from the loopback health
//! endpoint, non-zero otherwise.

use std::process::ExitCode;
use std::time::Duration;

use clap::Args;

#[derive(Debug, Args)]
pub struct HealthcheckArgs {
    /// Override the default loopback health URL.
    #[arg(long, default_value = "http://127.0.0.1:7878/api/v1/health")]
    pub url: String,
    /// Connection plus read timeout, in seconds.
    #[arg(long, default_value_t = 5)]
    pub timeout_seconds: u64,
}

/// Run the healthcheck. Returns `ExitCode::SUCCESS` on `200 OK`,
/// `ExitCode::FAILURE` on any other outcome (transport error, non-2xx
/// response, timeout).
pub fn run(args: HealthcheckArgs) -> ExitCode {
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(args.timeout_seconds))
        .connect_timeout(Duration::from_secs(args.timeout_seconds))
        .build()
    {
        Ok(client) => client,
        Err(_) => return ExitCode::FAILURE,
    };
    match client.get(&args.url).send() {
        Ok(response) if response.status().as_u16() == 200 => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}
```

Wiring fragment for `core/crates/cli/src/main.rs`:

```rust
#[derive(Debug, clap::Subcommand)]
enum Command {
    Serve(serve::ServeArgs),
    Healthcheck(healthcheck::HealthcheckArgs),
    // ... other subcommands ...
}
```

### Algorithm

1. Parse CLI arguments via clap.
2. Construct a `reqwest::blocking::Client` with the supplied timeout for both connect and read.
3. Issue a `GET` against the supplied URL.
4. Return `ExitCode::SUCCESS` iff the response status is exactly `200`.
5. On any error (transport failure, non-200 status, builder error), return `ExitCode::FAILURE`.

### Tests

- `healthcheck::tests::returns_zero_on_200` — start a `wiremock` server returning `200 OK`, invoke `run`, assert the returned `ExitCode` equals `ExitCode::SUCCESS`.
- `healthcheck::tests::returns_nonzero_on_500` — start a `wiremock` server returning `500`, invoke `run`, assert `ExitCode::FAILURE`.
- `healthcheck::tests::returns_nonzero_on_connection_refused` — point `run` at `http://127.0.0.1:1` (a closed port), assert `ExitCode::FAILURE`.

### Acceptance command

```
docker build -t trilithon:dev -f core/Dockerfile . \
  && docker run --rm trilithon:dev --version \
  && cargo test -p trilithon-cli healthcheck
```

### Exit conditions

- `core/Dockerfile` exists and builds without error on a clean workspace.
- The resulting image runs `trilithon --version` successfully.
- The three named healthcheck tests pass.
- The `Healthcheck` variant is wired into the top-level clap subcommand enum.

### Audit kinds emitted

None. The healthcheck subcommand is read-only and does not write audit rows.

### Tracing events emitted

None. Per architecture §12.1, the healthcheck binary process MUST NOT emit tracing events from the daemon vocabulary; it is a separate, short-lived process that only inspects the daemon's HTTP surface.

### Cross-references

- ADR-0010 (two-container deployment).
- ADR-0011 (loopback-only by default).
- PRD T2.8.
- Architecture §3 (system context — daemon-to-Caddy boundary), §11 (security posture).

---

## Slice 23.2 [trivial] — Base `docker-compose.yml` (default profile)

### Goal

Author the canonical Compose topology that runs the unmodified upstream Caddy image alongside the Trilithon image, with the four named volumes, capability drops, read-only root filesystems, healthchecks, and loopback-bound web UI port. After this slice, `docker compose -f deploy/compose/docker-compose.yml config` validates without error and the stack boots into a healthy state.

### Entry conditions

- Slice 23.1 complete (the Trilithon image and the `healthcheck` subcommand exist).
- The upstream `caddy:2.8-alpine` digest has been pinned in `deploy/compose/caddy-version.txt` (a one-line text file containing the SHA-256 digest of the chosen Caddy image, looked up via `docker manifest inspect`).

### Files to create or modify

- `deploy/compose/docker-compose.yml` — the canonical two-service topology.
- `deploy/compose/trilithon.env.example` — documented environment variables, no secret values.
- `deploy/compose/caddy-version.txt` — single-line file containing the pinned upstream Caddy digest.

### Signatures and shapes

`deploy/compose/docker-compose.yml` (verbatim, with `<digest>` substituted at file-creation time from `caddy-version.txt`):

```yaml
name: trilithon

services:
  caddy:
    image: caddy:2.8-alpine@sha256:<digest>
    restart: unless-stopped
    networks:
      - trilithon_internal
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - caddy_data:/data
      - caddy_config:/config
      - caddy_admin_socket:/run/caddy
    cap_add:
      - NET_BIND_SERVICE
    cap_drop:
      - ALL
    read_only: true
    tmpfs:
      - /tmp
    command: ["caddy", "run", "--config", "/config/caddy.json", "--resume"]
    healthcheck:
      test: ["CMD-SHELL", "wget --quiet --tries=1 --spider http://127.0.0.1:2019/config/ || exit 1"]
      interval: 10s
      timeout: 3s
      retries: 3
      start_period: 30s

  trilithon:
    image: ghcr.io/gasmanc/trilithon:latest
    restart: unless-stopped
    depends_on:
      caddy:
        condition: service_healthy
    networks:
      - trilithon_internal
    ports:
      - "127.0.0.1:7878:7878"
    volumes:
      - trilithon_data:/var/lib/trilithon
      - caddy_admin_socket:/run/caddy
    env_file:
      - ./trilithon.env
    cap_drop:
      - ALL
    read_only: true
    tmpfs:
      - /tmp
    healthcheck:
      test: ["CMD", "/usr/local/bin/trilithon", "healthcheck"]
      interval: 10s
      timeout: 3s
      retries: 3
      start_period: 60s

networks:
  trilithon_internal:
    driver: bridge
    internal: false

volumes:
  caddy_data:
  caddy_config:
  caddy_admin_socket:
  trilithon_data:
```

`deploy/compose/trilithon.env.example` (verbatim):

```
# Trilithon Compose environment overrides.
# Copy this file to trilithon.env and edit. None of these variables
# carry secret values in this template.

# Address the daemon binds for the web UI. Loopback by default
# (ADR-0011). Binding to 0.0.0.0 requires an authenticated session and
# is logged at startup.
TRILITHON_BIND=127.0.0.1:7878

# Persistent data directory inside the container. The compose volume
# trilithon_data is mounted here.
TRILITHON_DATA_DIR=/var/lib/trilithon

# Log level. One of: error, warn, info, debug, trace.
TRILITHON_LOG_LEVEL=info

# Path to the bootstrap-credentials file produced on first run
# (mode 0600, read by the operator on first login).
TRILITHON_BOOTSTRAP_TOKEN_PATH=/var/lib/trilithon/bootstrap.token
```

### Algorithm

1. Resolve the upstream Caddy image digest by running `docker manifest inspect caddy:2.8-alpine` once; record the digest in `deploy/compose/caddy-version.txt`.
2. Substitute the digest into `docker-compose.yml` at the `<digest>` placeholder.
3. Run `docker compose -f deploy/compose/docker-compose.yml config` to validate.
4. Run `docker compose -f deploy/compose/docker-compose.yml up -d`; assert that both services become healthy within 60 seconds.

### Tests

- `deploy/compose/test/test_compose_validates.sh` — runs `docker compose config` and asserts exit zero. Invoked by `just check`.
- `deploy/compose/test/test_default_profile_no_socket.sh` — boots the stack, runs `docker compose exec caddy stat /var/run/docker.sock` and `docker compose exec trilithon stat /var/run/docker.sock`; both MUST return non-zero exit (no such file).
- `deploy/compose/test/verify-caddy-digest.sh` — resolves the upstream digest and asserts byte-equality with `caddy-version.txt`.

### Acceptance command

```
docker compose -f deploy/compose/docker-compose.yml config \
  && bash deploy/compose/test/test_default_profile_no_socket.sh
```

### Exit conditions

- `deploy/compose/docker-compose.yml` exists with the four volumes, two services, single network, capability drops, read-only roots, tmpfs, healthchecks, and the loopback-only Trilithon port.
- `deploy/compose/trilithon.env.example` documents `TRILITHON_BIND`, `TRILITHON_DATA_DIR`, `TRILITHON_LOG_LEVEL`, `TRILITHON_BOOTSTRAP_TOKEN_PATH`, with no secret default values.
- `docker compose config` validates without error.
- The default-profile no-socket test passes.

### Audit kinds emitted

None. The Compose file is a deployment artefact; audit rows are emitted by the daemon at runtime per slice 23.4.

### Tracing events emitted

`daemon.started` (architecture §12.1) — emitted by the daemon when it reaches readiness inside the container. The Compose file does not emit tracing events directly.

### Cross-references

- ADR-0010, ADR-0011.
- PRD T2.8.
- Architecture §3 (system context), §11 (security posture).
- Hazards: H1, H11.

---

## Slice 23.3 [trivial] — Opt-in Docker-discovery overlay and socket-trust enforcement test

### Goal

Provide a Compose overlay that mounts `/var/run/docker.sock` read-only into the `trilithon` service only, activated under the `docker-discovery` profile, and a CI-runnable lint script that proves the Caddy service never receives the socket under any composition. After this slice, `docker compose --profile docker-discovery up` mounts the socket exclusively into Trilithon, and `bash deploy/compose/test/lint-no-socket.sh` exits zero.

### Entry conditions

- Slice 23.2 complete.
- `yq` (the Go implementation, version 4.x) is available on developer machines and CI runners.

### Files to create or modify

- `deploy/compose/docker-compose.discovery.yml` — overlay file mounting the Docker socket into `trilithon` only.
- `deploy/compose/test/lint-no-socket.sh` — bash script that parses both compose files and asserts the `caddy` service has zero socket mounts.

### Signatures and shapes

`deploy/compose/docker-compose.discovery.yml` (verbatim):

```yaml
# Opt-in Docker discovery overlay.
#
# Activate with:
#   docker compose --profile docker-discovery up
# or:
#   docker compose -f docker-compose.yml -f docker-compose.discovery.yml up
#
# This overlay mounts /var/run/docker.sock read-only into the trilithon
# service ONLY. The caddy service MUST NOT receive the mount under any
# composition (hazard H11). The lint script
# deploy/compose/test/lint-no-socket.sh enforces this invariant.

services:
  trilithon:
    profiles:
      - docker-discovery
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
```

`deploy/compose/test/lint-no-socket.sh` (verbatim):

```bash
#!/usr/bin/env bash
# Asserts the `caddy` service in deploy/compose/*.yml has zero entries
# in `volumes` matching /var/run/docker.sock or /run/docker.sock.
# Hazard H11 mitigation: the Docker socket is never mounted into Caddy.

set -euo pipefail

cd "$(dirname "$0")/.."

files=("docker-compose.yml" "docker-compose.discovery.yml")

for file in "${files[@]}"; do
    if [[ ! -f "${file}" ]]; then
        echo "lint-no-socket: ${file} not found" >&2
        exit 1
    fi
    matches="$(yq -r \
        '.services.caddy.volumes[]? | select(test("docker\\.sock"))' \
        "${file}" | wc -l | tr -d '[:space:]')"
    if [[ "${matches}" != "0" ]]; then
        echo "lint-no-socket: ${file} mounts the Docker socket into caddy" >&2
        echo "  hazard H11 violation; refusing to proceed" >&2
        exit 1
    fi
done

echo "lint-no-socket: ok (caddy never mounts the Docker socket)"
```

### Algorithm

1. For each compose file in `deploy/compose/*.yml`, parse it with `yq`.
2. Extract the `caddy` service's `volumes` list.
3. For each entry, test whether the entry string matches the regex `docker\.sock`.
4. If any match is found, print a hazard-H11 violation message and exit 1.
5. Otherwise exit 0.

### Tests

- `deploy/compose/test/test_lint_passes_on_canonical.sh` — runs `lint-no-socket.sh` against the canonical files and asserts exit 0.
- `deploy/compose/test/test_lint_fails_on_violation.sh` — copies `docker-compose.yml` to a temporary file, injects `- /var/run/docker.sock:/var/run/docker.sock:ro` into the `caddy.volumes` list, runs the lint, asserts exit 1.
- `deploy/compose/test/test_discovery_socket_mounted_in_trilithon.sh` — boots the stack with the discovery profile, runs `docker compose --profile docker-discovery exec trilithon stat /var/run/docker.sock` (asserts exit 0), and `docker compose --profile docker-discovery exec caddy stat /var/run/docker.sock` (asserts non-zero).

### Acceptance command

```
bash deploy/compose/test/lint-no-socket.sh \
  && bash deploy/compose/test/test_lint_fails_on_violation.sh
```

### Exit conditions

- `deploy/compose/docker-compose.discovery.yml` exists and mounts the Docker socket into the `trilithon` service only, scoped to the `docker-discovery` profile.
- `deploy/compose/test/lint-no-socket.sh` exists, is `chmod +x`, and exits 0 on canonical files and 1 on a synthetic violation.
- The discovery-profile mount test passes.

### Audit kinds emitted

`docker.socket-trust-grant` (architecture §6.6) — emitted by the daemon at startup when the socket is detected. The Compose overlay itself emits no audit rows.

### Tracing events emitted

None at the Compose layer. Slice 23.4 emits the corresponding tracing event.

### Cross-references

- ADR-0010.
- PRD T2.8 (mitigates H11).
- Architecture §3 (Docker socket boundary), §11 (security posture).
- Hazards: H11.

---

## Slice 23.4 [cross-cutting] — First-run Docker socket trust-grant warning emission

### Goal

When the Trilithon daemon starts inside a container that has `/var/run/docker.sock` mounted, it MUST emit the trust-grant warning block to stdout, stderr, and the audit log with kind `docker.socket-trust-grant`. After this slice, an integration test running the binary inside a socket-mounted container asserts the warning appears in all three sinks exactly once per process lifetime.

### Entry conditions

- Slice 23.1 complete (the Trilithon image exists).
- Slice 23.3 complete (the discovery overlay exists for integration testing).
- Phase 6 audit log writer (`adapters::audit_log_store::append`) is available.

### Files to create or modify

- `core/crates/cli/src/startup.rs` — startup-time hook that detects the socket and emits the warning.
- `core/crates/core/src/audit.rs` — `AuditEvent::DockerSocketTrustGrant` already exists (confirmed in source). Do not re-add it; only verify the `kind` wire string is `"docker.socket-trust-grant"` per architecture §6.6.
- `core/crates/cli/tests/docker_socket_trust_grant.rs` — integration test asserting the three-sink emission.

### Signatures and shapes

```rust
// core/crates/cli/src/startup.rs
//! Startup-time side effects that run before the daemon enters its
//! main loop. The trust-grant warning lives here because it MUST run
//! exactly once per process and MUST run before any user action.

use std::io::Write;
use std::path::Path;

use crate::audit::record_docker_socket_trust_grant;

/// The literal trust-grant block printed to stdout and stderr.
/// MUST be emitted verbatim per the phased plan.
pub const DOCKER_SOCKET_TRUST_GRANT_BLOCK: &str = "\
=== Docker socket trust grant ===
Trilithon has detected /var/run/docker.sock mounted into this container.
This grants Trilithon effective root on the host machine.
This is a deliberate trust grant. To revoke it, restart without the
docker-discovery profile.
=== End Docker socket trust grant ===
";

/// Detects the Docker socket and emits the trust-grant warning.
/// Returns `true` if the warning was emitted.
pub fn emit_docker_socket_trust_grant_if_present(
    socket_path: &Path,
    audit_writer: &dyn crate::audit::AuditWriter,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<bool, std::io::Error> {
    if !socket_is_writable(socket_path) {
        return Ok(false);
    }
    write!(stdout, "{}", DOCKER_SOCKET_TRUST_GRANT_BLOCK)?;
    write!(stderr, "{}", DOCKER_SOCKET_TRUST_GRANT_BLOCK)?;
    stdout.flush()?;
    stderr.flush()?;
    record_docker_socket_trust_grant(audit_writer);
    Ok(true)
}

fn socket_is_writable(path: &Path) -> bool {
    use std::fs::OpenOptions;
    OpenOptions::new().write(true).open(path).is_ok()
}
```

Audit-event mapping (per architecture §6.6 mapping table):

```rust
// core/crates/core/src/audit.rs (relevant variant)
#[derive(Debug, Clone)]
pub enum AuditEvent {
    // ... other variants ...
    DockerSocketTrustGrant {
        socket_path: String,
        container_id: Option<String>,
    },
}

impl std::fmt::Display for AuditEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // ...
            AuditEvent::DockerSocketTrustGrant { .. } => {
                f.write_str("docker.socket-trust-grant")
            }
        }
    }
}
```

### Algorithm

1. At process startup, after configuration is loaded but before the HTTP listener binds, call `emit_docker_socket_trust_grant_if_present` with `Path::new("/var/run/docker.sock")`.
2. The function attempts a write-mode `open(2)` on the socket path. If `open` succeeds, the socket is mounted and writable; if `open` fails with any error, the socket is treated as absent.
3. On detection, write the verbatim block to stdout and stderr (in that order); flush both.
4. Append one audit row with `kind = "docker.socket-trust-grant"`, `actor_kind = "system"`, `outcome = "ok"`, `notes` JSON `{ "socket_path": "/var/run/docker.sock" }`.
5. Record that the warning has been emitted in process-local state so it does not repeat on capability re-probe.

### Tests

- `cli::startup::tests::warning_absent_when_socket_absent` — call `emit_...` with a path that does not exist; assert no writes to stdout/stderr and no audit row.
- `cli::startup::tests::warning_emitted_when_socket_present` — create a Unix socket in a tempdir; pass its path; assert verbatim block in both writers and exactly one audit row with kind `docker.socket-trust-grant`.
- `core/crates/cli/tests/docker_socket_trust_grant.rs` — full integration: spawn the binary inside a Docker container under the `docker-discovery` profile; capture stdout, stderr, and read the audit log row from SQLite; assert all three contain the warning.

### Acceptance command

```
cargo test -p trilithon-cli docker_socket_trust_grant
```

### Exit conditions

- `emit_docker_socket_trust_grant_if_present` is called from the daemon's startup path before the HTTP listener binds.
- The verbatim block from `DOCKER_SOCKET_TRUST_GRANT_BLOCK` matches the phased-plan text byte-for-byte.
- The integration test asserts the warning in stdout, stderr, and the audit log.
- The audit row's `kind` is `docker.socket-trust-grant`.

### Audit kinds emitted

- `docker.socket-trust-grant` (architecture §6.6).

### Tracing events emitted

- `daemon.started` (architecture §12.1) — emitted after the trust-grant warning, by the existing daemon startup code.

### Cross-references

- ADR-0010.
- PRD T2.8 (mitigates H11).
- Architecture §6.6 (audit kinds), §11 (security posture).
- Hazards: H11.

---

## Slice 23.5 [trivial] — GHCR publish workflow with multi-arch build, cosign signing, and SBOM

### Goal

A GitHub Actions workflow at `.github/workflows/docker-publish.yml` builds `linux/amd64` and `linux/arm64` images, pushes them to `ghcr.io/gasmanc/trilithon`, signs each digest with cosign keyless OIDC, attaches an SPDX SBOM via Syft as a registry attestation, and enforces the 50 MiB image-size budget. After this slice, a tag push of the form `v*` produces a multi-arch signed image visible in GHCR.

### Entry conditions

- Slice 23.1 complete (the Dockerfile is buildable in CI).
- The GitHub repository has a secret named `GHCR_TOKEN` with `write:packages` scope.
- `cosign` and `syft` are installable on `ubuntu-24.04` GitHub-hosted runners.

### Files to create or modify

- `.github/workflows/docker-publish.yml` — the publish workflow.
- `deploy/compose/README.md` — add only the "Verifying image signatures" heading and verification command (full README in slice 23.8).

### Signatures and shapes

`.github/workflows/docker-publish.yml` (verbatim):

```yaml
name: docker-publish

on:
  push:
    tags:
      - 'v*'
  workflow_dispatch:

permissions:
  contents: read
  id-token: write
  packages: write

jobs:
  build-and-publish:
    runs-on: ubuntu-24.04
    env:
      REGISTRY: ghcr.io
      IMAGE: ghcr.io/gasmanc/trilithon
      MAX_IMAGE_BYTES: 52428800
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Set up QEMU
        uses: docker/setup-qemu-action@v3

      - name: Set up Buildx
        uses: docker/setup-buildx-action@v3

      - name: Log in to GHCR
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GHCR_TOKEN }}

      - name: Compute tags
        id: tags
        run: |
          set -euo pipefail
          ref="${GITHUB_REF##*/}"
          if [[ "${ref}" == v* ]]; then
            ver="${ref#v}"
            major="${ver%%.*}"
            rest="${ver#*.}"
            minor="${rest%%.*}"
            tags="${IMAGE}:${ref},${IMAGE}:v${major}.${minor},${IMAGE}:v${major},${IMAGE}:latest"
          else
            tags="${IMAGE}:edge"
          fi
          echo "tags=${tags}" >> "${GITHUB_OUTPUT}"

      - name: Build and push (multi-arch)
        id: build
        uses: docker/build-push-action@v6
        with:
          context: .
          file: core/Dockerfile
          platforms: linux/amd64,linux/arm64
          push: true
          tags: ${{ steps.tags.outputs.tags }}
          provenance: true
          sbom: false

      - name: Enforce 50 MiB image-size budget
        run: |
          set -euo pipefail
          docker pull --platform linux/amd64 "${IMAGE}@${{ steps.build.outputs.digest }}"
          size=$(docker image inspect --format='{{.Size}}' "${IMAGE}@${{ steps.build.outputs.digest }}")
          echo "Image size: ${size} bytes (budget ${MAX_IMAGE_BYTES})"
          if (( size > MAX_IMAGE_BYTES )); then
            echo "ERROR: image exceeds 50 MiB budget" >&2
            exit 1
          fi

      - name: Install cosign
        uses: sigstore/cosign-installer@v3
        with:
          cosign-release: v2.4.0

      - name: Sign image (keyless OIDC)
        env:
          COSIGN_EXPERIMENTAL: '1'
        run: |
          cosign sign --yes "${IMAGE}@${{ steps.build.outputs.digest }}"

      - name: Install Syft
        uses: anchore/sbom-action/download-syft@v0

      - name: Generate SPDX SBOM
        run: |
          syft "${IMAGE}@${{ steps.build.outputs.digest }}" \
            -o spdx-json=trilithon.spdx.json

      - name: Attach SBOM attestation
        env:
          COSIGN_EXPERIMENTAL: '1'
        run: |
          cosign attest --yes \
            --predicate trilithon.spdx.json \
            --type spdxjson \
            "${IMAGE}@${{ steps.build.outputs.digest }}"
```

Signature-verification snippet for `deploy/compose/README.md` (heading and command only at this slice):

```markdown
## Verifying image signatures

Trilithon's published images at `ghcr.io/gasmanc/trilithon` are signed
with Sigstore cosign keyless OIDC against the GitHub Actions identity
issuer. Verify a specific tag with:

    cosign verify ghcr.io/gasmanc/trilithon:<tag> \
      --certificate-identity-regexp '^https://github\.com/gasmanc/Trilithon/\.github/workflows/docker-publish\.yml@refs/tags/v.*$' \
      --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

### Algorithm

1. The workflow triggers on `push` for tags matching `v*` and on manual `workflow_dispatch`.
2. Compute the tag set: `vX.Y.Z`, `vX.Y`, `vX`, `latest` for tag pushes; `edge` for manual dispatch.
3. Build the multi-arch image via Buildx with `linux/amd64` and `linux/arm64`.
4. Pull the published `linux/amd64` digest, run `docker image inspect --format='{{.Size}}'`, fail if the result exceeds `52428800` bytes (50 MiB).
5. Install cosign, sign the digest keyless via OIDC.
6. Install Syft, generate an SPDX-JSON SBOM, attach it as a `spdxjson` attestation via cosign.

### Tests

- The workflow itself is a CI artefact; it is "tested" by tag-pushing a release candidate and observing all six steps green.
- Slice 23.6 includes a smoke step that runs `cosign verify` against the published image; this is the operational test of slice 23.5.

### Acceptance command

```
git tag v0.0.0-test && git push origin v0.0.0-test \
  && gh run watch --workflow docker-publish.yml
```

### Exit conditions

- `.github/workflows/docker-publish.yml` exists and contains every step listed above.
- A test tag push produces a signed, multi-arch image at `ghcr.io/gasmanc/trilithon` visible to `cosign verify`.
- The image-size budget step fails the workflow on a synthetic over-budget image.
- The SBOM attestation is attached and retrievable via `cosign download attestation`.

### Audit kinds emitted

None. Image publication is a CI artefact; no daemon process runs.

### Tracing events emitted

None. Same reason.

### Cross-references

- ADR-0010.
- PRD T2.8.
- Architecture §11 (supply-chain posture; see ADR-0010 §Consequences).

---

## Slice 23.6 [standard] — Compose smoke-test script and 30-second bootstrap timing gate

### Goal

A bash script `deploy/compose/test/smoke.sh` boots the stack on a fresh runner, polls the health endpoint until 200 OK or 30 seconds elapse, logs in as the bootstrap user, posts a route, asserts Caddy serves the route, verifies the upstream cosign signature, and tears down. After this slice, the script exits zero in CI on every supported runner.

### Entry conditions

- Slices 23.1 through 23.5 complete.
- The Trilithon HTTP API exposes `POST /api/v1/auth/login`, `POST /api/v1/routes`, and `GET /api/v1/health` (Phases 9 and 11).

### Files to create or modify

- `deploy/compose/test/smoke.sh` — the smoke test.
- `deploy/compose/test/verify-caddy-digest.sh` — already authored by slice 23.2; smoke wires it in.
- `.github/workflows/compose-smoke.yml` — CI workflow that runs the smoke test on a tagged release candidate and on a daily schedule.

### Signatures and shapes

`deploy/compose/test/smoke.sh` (verbatim):

```bash
#!/usr/bin/env bash
# Boots the Compose stack and exercises a happy-path login + route apply.
# Asserts bootstrap reaches 200 OK within 30 seconds (T2.8 acceptance).

set -euo pipefail

cd "$(dirname "$0")/.."

cleanup() {
    docker compose -f docker-compose.yml down --volumes --remove-orphans || true
}
trap cleanup EXIT

echo "smoke: verifying upstream Caddy digest"
bash test/verify-caddy-digest.sh

echo "smoke: booting stack"
docker compose -f docker-compose.yml up -d

echo "smoke: polling /api/v1/health (30s budget)"
deadline=$(( $(date +%s) + 30 ))
until curl --silent --fail http://127.0.0.1:7878/api/v1/health > /dev/null 2>&1; do
    if (( $(date +%s) > deadline )); then
        echo "smoke: bootstrap exceeded 30 seconds" >&2
        docker compose logs >&2
        exit 1
    fi
    sleep 1
done
echo "smoke: health 200 OK within budget"

echo "smoke: reading bootstrap credentials"
volume_path="$(docker volume inspect trilithon_trilithon_data --format '{{ .Mountpoint }}')"
bootstrap_token="$(sudo cat "${volume_path}/bootstrap.token")"

echo "smoke: logging in"
session="$(curl --silent --fail \
    -X POST http://127.0.0.1:7878/api/v1/auth/login \
    -H 'Content-Type: application/json' \
    -d "{\"bootstrap_token\":\"${bootstrap_token}\"}" \
    | jq -r '.session_id')"

echo "smoke: creating route smoke.invalid -> 127.0.0.1:9999"
curl --silent --fail \
    -X POST http://127.0.0.1:7878/api/v1/routes \
    -H "X-Trilithon-Session: ${session}" \
    -H 'Content-Type: application/json' \
    -d '{"hostname":"smoke.invalid","upstream":"127.0.0.1:9999"}'

echo "smoke: probing route"
status="$(curl --silent --output /dev/null --write-out '%{http_code}' \
    -H 'Host: smoke.invalid' http://127.0.0.1/)"
case "${status}" in
    502|503|504) echo "smoke: route present, upstream unreachable as expected" ;;
    *) echo "smoke: unexpected status ${status}" >&2; exit 1 ;;
esac

echo "smoke: verifying cosign signature"
cosign verify ghcr.io/gasmanc/trilithon:latest \
    --certificate-identity-regexp '^https://github\.com/gasmanc/Trilithon/\.github/workflows/docker-publish\.yml@refs/tags/v.*$' \
    --certificate-oidc-issuer https://token.actions.githubusercontent.com

echo "smoke: ok"
```

`.github/workflows/compose-smoke.yml` (verbatim):

```yaml
name: compose-smoke

on:
  workflow_dispatch:
  schedule:
    - cron: '0 6 * * *'
  push:
    paths:
      - 'deploy/compose/**'
      - 'core/Dockerfile'

jobs:
  smoke:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - name: Install cosign
        uses: sigstore/cosign-installer@v3
      - name: Run smoke test
        run: bash deploy/compose/test/smoke.sh
```

### Algorithm

1. Trap on EXIT to run `docker compose down --volumes`.
2. Verify the upstream Caddy digest before booting.
3. Boot via `docker compose up -d`.
4. Poll `/api/v1/health` once per second until 200 OK or 30 seconds elapse; on timeout, dump `docker compose logs` and exit 1.
5. Read the bootstrap token from the `trilithon_data` volume mount path.
6. Log in via `POST /api/v1/auth/login`, capture the session id.
7. Create a route via `POST /api/v1/routes`.
8. Probe `Host: smoke.invalid http://127.0.0.1/`; the upstream is unreachable so any 5xx status is expected.
9. Run `cosign verify` against the published image.
10. Exit 0.

### Tests

- The smoke script is itself the test. Two CI-runnable assertions: it exits 0 on a healthy build, and an injected 60-second `sleep` in the daemon's startup path causes it to exit 1 inside the bootstrap-budget loop.

### Acceptance command

```
bash deploy/compose/test/smoke.sh
```

### Exit conditions

- `deploy/compose/test/smoke.sh` exists, is executable, and passes on a fresh `ubuntu-24.04` runner.
- The 30-second bootstrap budget is enforced: a synthetic delay causes the script to exit 1.
- `.github/workflows/compose-smoke.yml` is registered and runs on push to `deploy/compose/**` and `core/Dockerfile`.

### Audit kinds emitted

The smoke test exercises (but does not directly assert) the following audit kinds emitted by the daemon during the run:

- `auth.login-succeeded` (architecture §6.6).
- `mutation.submitted`, `mutation.applied`, `config.applied` (architecture §6.6).

### Tracing events emitted

The smoke test exercises:

- `daemon.started`, `http.request.received`, `http.request.completed`, `apply.started`, `apply.succeeded` (architecture §12.1).

### Cross-references

- ADR-0010, ADR-0011.
- PRD T2.8.
- Hazards: H1, H13.

---

## Slice 23.7 [standard] — Upgrade-from-prior smoke test and `UPGRADING.md` rationale paragraph

### Goal

A second smoke script boots the most recently published image, applies a route, then upgrades to the current built image, asserts the route persists and migrations apply cleanly. The accompanying `UPGRADING.md` documents the four-step procedure plus the rollback story and contains the verbatim Caddy-version rationale paragraph.

### Entry conditions

- Slice 23.5 complete (a previously published image exists at `ghcr.io/gasmanc/trilithon:latest`).
- Slice 23.6 complete (the smoke baseline exists for reuse).

### Files to create or modify

- `deploy/compose/test/upgrade-from-prior.sh` — the upgrade smoke test.
- `deploy/compose/UPGRADING.md` — operator-facing upgrade documentation including the verbatim rationale paragraph.

### Signatures and shapes

`deploy/compose/test/upgrade-from-prior.sh` (verbatim):

```bash
#!/usr/bin/env bash
# Boots the previously published Trilithon image, applies a route, then
# upgrades to the local build, verifies the route persists and migrations
# apply.

set -euo pipefail

cd "$(dirname "$0")/.."

PRIOR_TAG="${PRIOR_TAG:-latest}"
NEW_IMAGE="ghcr.io/gasmanc/trilithon:edge"

cleanup() {
    docker compose -f docker-compose.yml down --volumes --remove-orphans || true
}
trap cleanup EXIT

# Stage 1: boot prior image.
TRILITHON_IMAGE="ghcr.io/gasmanc/trilithon:${PRIOR_TAG}" \
    docker compose -f docker-compose.yml up -d
deadline=$(( $(date +%s) + 60 ))
until curl --silent --fail http://127.0.0.1:7878/api/v1/health > /dev/null 2>&1; do
    if (( $(date +%s) > deadline )); then
        echo "upgrade: prior image failed to boot" >&2; exit 1
    fi
    sleep 1
done

# Apply a sentinel route under the prior version.
volume_path="$(docker volume inspect trilithon_trilithon_data --format '{{ .Mountpoint }}')"
bootstrap_token="$(sudo cat "${volume_path}/bootstrap.token")"
session="$(curl --silent --fail \
    -X POST http://127.0.0.1:7878/api/v1/auth/login \
    -H 'Content-Type: application/json' \
    -d "{\"bootstrap_token\":\"${bootstrap_token}\"}" \
    | jq -r '.session_id')"
curl --silent --fail \
    -X POST http://127.0.0.1:7878/api/v1/routes \
    -H "X-Trilithon-Session: ${session}" \
    -H 'Content-Type: application/json' \
    -d '{"hostname":"upgrade-sentinel.invalid","upstream":"127.0.0.1:9999"}'

# Stage 2: upgrade.
docker compose -f docker-compose.yml down
TRILITHON_IMAGE="${NEW_IMAGE}" \
    docker compose -f docker-compose.yml up -d
deadline=$(( $(date +%s) + 60 ))
until curl --silent --fail http://127.0.0.1:7878/api/v1/health > /dev/null 2>&1; do
    if (( $(date +%s) > deadline )); then
        echo "upgrade: new image failed to boot" >&2
        docker compose logs >&2
        exit 1
    fi
    sleep 1
done

# Re-acquire session (database persisted) and assert sentinel route present.
session="$(curl --silent --fail \
    -X POST http://127.0.0.1:7878/api/v1/auth/login \
    -H 'Content-Type: application/json' \
    -d "{\"bootstrap_token\":\"${bootstrap_token}\"}" \
    | jq -r '.session_id')"
routes="$(curl --silent --fail \
    -H "X-Trilithon-Session: ${session}" \
    http://127.0.0.1:7878/api/v1/routes)"
echo "${routes}" | jq -e '.[] | select(.hostname == "upgrade-sentinel.invalid")' > /dev/null

echo "upgrade-from-prior: ok"
```

`deploy/compose/UPGRADING.md` (verbatim, including the mandated rationale paragraph):

```markdown
# Upgrading the Trilithon Compose deployment

This page documents the four-step upgrade procedure and the rollback
story when migrations fail.

## Caddy version pin rationale

Trilithon's Compose deployment pins Caddy to the latest 2.8 patch
(currently `caddy:2.8-alpine`) for stability of the deployment
artefact; continuous integration tests against the latest stable Caddy
(currently 2.11.2 per `caddy-version.txt`) for forward-compatibility.
Both pins are intentional and tracked separately.

## Procedure

1. **Pull new images.** `docker compose pull` fetches the new
   `ghcr.io/gasmanc/trilithon` tag and the upstream Caddy digest.
2. **Recreate containers.** `docker compose up -d` restarts both
   services with the new images.
3. **Migrations apply.** Trilithon's startup runs SQLite schema
   migrations under a transaction; on success the daemon proceeds to
   reconcile against Caddy's running configuration.
4. **Verify.** `curl http://127.0.0.1:7878/api/v1/health` returns
   `200 OK` within 30 seconds.

## Rollback when migrations fail

On migration failure the daemon exits with code `4` and writes a
`migration-failed` audit row to a side-car file under
`/var/lib/trilithon/`. The previous container's database remains
unchanged because migrations run in a single transaction. To return
to the prior version:

```
docker compose down
docker tag ghcr.io/gasmanc/trilithon:vX.Y.Z-1 \
           ghcr.io/gasmanc/trilithon:latest
docker compose up -d
```

Rolling forward across more than one minor version is supported.
Rolling back across a schema-incompatible migration is OUT OF SCOPE
FOR V1 and surfaces as a `manifest_incompatible` error from the
Phase 26 restore path if attempted.
```

### Algorithm

1. Boot the prior published image with the canonical Compose file (override `image:` via the `TRILITHON_IMAGE` environment variable).
2. Wait for health, log in, post a sentinel route.
3. `docker compose down` (preserve volumes).
4. Boot the new image (`edge` tag from CI, or `latest` on a release tag).
5. Wait for health within 60 seconds; on timeout dump logs and fail.
6. Log in again (the bootstrap token persists in the volume).
7. Fetch routes, assert the sentinel route is present (jq predicate).

### Tests

- The script itself is the test. CI runs it on every release tag.
- A negative-path test injects a deliberate breaking migration and asserts the daemon exits with code 4 and the route is recoverable by rolling back.

### Acceptance command

```
bash deploy/compose/test/upgrade-from-prior.sh
```

### Exit conditions

- `deploy/compose/test/upgrade-from-prior.sh` passes against the most recent published image.
- `deploy/compose/UPGRADING.md` exists and contains the verbatim Caddy-version rationale paragraph from the phased plan.
- The four-step procedure is documented.
- The rollback story is documented.

### Audit kinds emitted

- `config.applied` (architecture §6.6) — emitted by the prior version when the sentinel route is applied.
- `storage.migrations.applied` (tracing event) and the corresponding system audit row recording the schema version (architecture §14).

### Tracing events emitted

- `storage.migrations.applied` (architecture §12.1).
- `daemon.started` (architecture §12.1) — emitted twice (prior boot, post-upgrade boot).

### Cross-references

- ADR-0010.
- PRD T2.8.
- Architecture §14 (upgrade and migration).
- Hazards: H9 (Caddy version skew across snapshots).

---

## Slice 23.8 [trivial] — Operator and end-user documentation

### Goal

Write the operator-facing `deploy/compose/README.md` and the user-facing `docs/install/compose.md` with the headings mandated by the phased plan. After this slice, a documentation lint asserts every mandated heading is present in both files.

### Entry conditions

- Slices 23.1 through 23.7 complete.

### Files to create or modify

- `deploy/compose/README.md` — operator-facing Compose README (full content; slice 23.5 added the signature-verification heading only).
- `docs/install/compose.md` — user-facing installation page.
- `deploy/compose/test/lint-readme-headings.sh` — heading lint.

### Signatures and shapes

`deploy/compose/README.md` outline (each `## ` heading MUST appear verbatim):

```markdown
# Trilithon — Docker Compose deployment

## Prerequisites

## First run

## Bootstrap credentials

## Enabling Docker discovery

## Upgrading

## Backing up volumes

## Verifying image signatures

## Troubleshooting
```

`docs/install/compose.md` outline:

```markdown
# Installing Trilithon with Docker Compose

## Prerequisites

## First run

## Bootstrap credentials

## Enabling Docker discovery

## Upgrading

## Backing up volumes

## Verifying image signatures

## Troubleshooting

## See also

- [Trilithon audit log model](../architecture/architecture.md#66-audit_log)
- [Hazard H11 — Docker socket trust boundary](../prompts/PROMPT-spec-generation.md#h11-docker-socket-trust-boundary)
```

`deploy/compose/test/lint-readme-headings.sh` (verbatim):

```bash
#!/usr/bin/env bash
set -euo pipefail
required=(
    "## Prerequisites"
    "## First run"
    "## Bootstrap credentials"
    "## Enabling Docker discovery"
    "## Upgrading"
    "## Backing up volumes"
    "## Verifying image signatures"
    "## Troubleshooting"
)
for file in deploy/compose/README.md docs/install/compose.md; do
    for heading in "${required[@]}"; do
        if ! grep -Fxq "${heading}" "${file}"; then
            echo "lint-readme-headings: ${file} missing heading: ${heading}" >&2
            exit 1
        fi
    done
done
echo "lint-readme-headings: ok"
```

### Algorithm

1. For each of the two documentation files, ensure every required heading appears as a top-level (`## `) heading byte-for-byte.
2. The lint script enforces this on every CI run.

### Tests

- `deploy/compose/test/lint-readme-headings.sh` — passes on canonical files, fails on a synthetic deletion of any required heading.

### Acceptance command

```
bash deploy/compose/test/lint-readme-headings.sh
```

### Exit conditions

- `deploy/compose/README.md` exists with every mandated heading.
- `docs/install/compose.md` exists with every mandated heading plus the cross-reference list to the audit log and the H11 hazard.
- The heading lint passes.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- ADR-0010, ADR-0011.
- PRD T2.8.
- Hazards: H11, H13.

---

## Slice 23.9 [trivial] — Wire deployment lints into `just check` and image-size budget into CI

### Goal

The deployment lint scripts (`lint-no-socket.sh`, `lint-readme-headings.sh`, `verify-caddy-digest.sh`) and the image-size budget gate are wired into `just check` and into the publish workflow so regressions fail builds. After this slice, `just check` runs every deployment lint locally and CI fails on any violation.

### Entry conditions

- Slices 23.3, 23.5, and 23.8 complete.

### Files to create or modify

- `justfile` — add the deployment-checks recipe.
- `.github/workflows/docker-publish.yml` — extend with a lints job that depends on the build job.

### Signatures and shapes

`justfile` recipe addition:

```just
# Run deployment lints. Wired into `just check`.
deployment-checks:
    bash deploy/compose/test/lint-no-socket.sh
    bash deploy/compose/test/lint-readme-headings.sh
    bash deploy/compose/test/verify-caddy-digest.sh

check: check-rust check-typescript deployment-checks
```

`.github/workflows/docker-publish.yml` extension (additional job, appended after the existing `build-and-publish` job):

```yaml
  deployment-lints:
    runs-on: ubuntu-24.04
    needs: build-and-publish
    steps:
      - uses: actions/checkout@v4
      - name: Install yq
        run: |
          sudo wget -O /usr/local/bin/yq \
            https://github.com/mikefarah/yq/releases/download/v4.44.3/yq_linux_amd64
          sudo chmod +x /usr/local/bin/yq
      - run: bash deploy/compose/test/lint-no-socket.sh
      - run: bash deploy/compose/test/lint-readme-headings.sh
```

### Algorithm

1. The `deployment-checks` recipe runs each lint script; any non-zero exit fails `just check`.
2. The `deployment-lints` CI job runs after the build job and re-runs the same scripts on the canonical files in the repository.

### Tests

- `just deployment-checks` exits 0 on a clean tree.
- A synthetic violation in `docker-compose.yml` (mounting the socket into Caddy) fails `just check`.

### Acceptance command

```
just check
```

### Exit conditions

- `just check` runs the three deployment lints in order and fails on any violation.
- The publish workflow runs the lints in CI.
- The image-size budget step (slice 23.5) is referenced from this slice's documentation as the publish-time gate.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- ADR-0010.
- PRD T2.8.

---

## Phase exit checklist

- [ ] `just check` passes.
- [ ] `docker compose up` on a fresh host produces a working web UI on `http://127.0.0.1:7878` within 30 seconds (slice 23.6).
- [ ] The Caddy image is an unmodified official image (digest match per `verify-caddy-digest.sh`).
- [ ] The Trilithon image is a multi-stage Rust build on a distroless base, under 50 MiB (slice 23.5).
- [ ] The Docker socket is not visible in either container under the default profile (slice 23.3).
- [ ] Under the `docker-discovery` profile, the socket is mounted into `trilithon` only, and the trust-grant warning is printed (slices 23.3 and 23.4).
- [ ] Images are signed with cosign and the smoke test verifies the signature (slices 23.5 and 23.6).
- [ ] The upgrade-from-prior test passes against the most recent published image (slice 23.7).
- [ ] `deploy/compose/UPGRADING.md` contains the verbatim Caddy-version rationale paragraph (slice 23.7).
- [ ] `deploy/compose/README.md` and `docs/install/compose.md` contain every mandated heading (slice 23.8).
