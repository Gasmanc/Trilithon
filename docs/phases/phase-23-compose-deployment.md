# Phase 23 — Two-container Docker Compose deployment

Source of truth: [`../phases/phased-plan.md#phase-23--two-container-docker-compose-deployment`](../phases/phased-plan.md#phase-23--two-container-docker-compose-deployment).

## Pre-flight checklist

- [ ] Phase 22 complete.
- [ ] The Trilithon binary builds against `gcr.io/distroless/cc-debian12` without missing shared libraries (or links against `musl` statically).
- [ ] A signed-in `ghcr.io` registry context is available in CI for image push (PAT or OIDC).
- [ ] `cosign` and `syft` are installable in the publish workflow runner.

## Tasks

### Trilithon container image

- [ ] **Author the multi-stage `Dockerfile`.**
  - Path: `core/Dockerfile`.
  - Acceptance: Stage 1 `FROM rust:1.80-slim-bookworm AS builder`, installs `pkg-config`, `libssl-dev`, `clang`, `mold`. Builds with `cargo build --release --workspace --bin trilithon-cli`, sets `CARGO_PROFILE_RELEASE_LTO=thin`, `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1`, runs `strip --strip-all`. Stage 2 `FROM gcr.io/distroless/cc-debian12:nonroot`, copies the binary to `/usr/local/bin/trilithon`, sets `USER nonroot:nonroot`, `WORKDIR /var/lib/trilithon`, `EXPOSE 7878`, `ENTRYPOINT ["/usr/local/bin/trilithon"]`, `CMD ["serve"]`.
  - Done when: `docker build -t trilithon:dev -f core/Dockerfile .` produces an image and `docker run --rm trilithon:dev --version` prints the version.
  - Feature: T2.8.
- [ ] **Enforce the 50 MB image size budget in CI.**
  - Path: `.github/workflows/docker-publish.yml`.
  - Acceptance: After build, the workflow MUST run `docker image inspect --format='{{.Size}}' trilithon:<tag>` and fail if the result exceeds `50 * 1024 * 1024`.
  - Done when: the CI step asserts the budget.
  - Feature: T2.8.
- [ ] **Implement the `trilithon healthcheck` subcommand.**
  - Module: `core/crates/cli/src/commands/healthcheck.rs`.
  - Acceptance: `pub fn run(args: HealthcheckArgs) -> ExitCode` opens `http://127.0.0.1:7878/api/v1/health`, exits zero on `200 OK`, non-zero otherwise. Five-second connection and read timeout.
  - Done when: a unit test exercises both branches against a local mock server.
  - Feature: T2.8.

### Compose topology

- [ ] **Author `docker-compose.yml` (default profile).**
  - Path: `deploy/compose/docker-compose.yml`.
  - Acceptance: Two services (`caddy`, `trilithon`), one network (`trilithon_internal`), four volumes (`caddy_data`, `caddy_config`, `caddy_admin_socket`, `trilithon_data`). `caddy` image pinned by digest to `caddy:2.8-alpine@sha256:<digest>`. `caddy` exposes `80:80` and `443:443`; `trilithon` exposes `127.0.0.1:7878:7878`. `caddy` has `cap_add: [NET_BIND_SERVICE]`, `cap_drop: [ALL]`, `read_only: true`, `tmpfs: [/tmp]`. `trilithon` has `cap_drop: [ALL]`, `read_only: true`, `tmpfs: [/tmp]`. Both services have the healthcheck described in the phased plan.
  - Done when: `docker compose -f deploy/compose/docker-compose.yml config` validates without error.
  - Feature: T2.8.
- [ ] **Author `docker-compose.discovery.yml` overlay.**
  - Path: `deploy/compose/docker-compose.discovery.yml`.
  - Acceptance: An override file activated under the `docker-discovery` profile that mounts `/var/run/docker.sock:/var/run/docker.sock:ro` into the `trilithon` service ONLY. The Caddy service MUST NOT receive the mount under any composition.
  - Done when: a smoke test under `docker compose --profile docker-discovery up` confirms the mount in `trilithon` and absent in `caddy`.
  - Feature: T2.8 (mitigates H11).
- [ ] **Author `trilithon.env.example`.**
  - Path: `deploy/compose/trilithon.env.example`.
  - Acceptance: Documents `TRILITHON_BIND`, `TRILITHON_DATA_DIR`, `TRILITHON_LOG_LEVEL`, `TRILITHON_BOOTSTRAP_TOKEN_PATH`. No secret default values.
  - Done when: the file exists and a documentation lint asserts every variable is documented in the README.
  - Feature: T2.8.

### First-run trust warning

- [ ] **Detect Docker socket and emit warning on startup.**
  - Module: `core/crates/cli/src/startup.rs`.
  - Acceptance: On startup, when `/var/run/docker.sock` is present and writable from inside the container, Trilithon MUST emit the multi-line warning block in the phased plan to stdout, stderr, and the audit log with kind `docker.socket-trust-grant`.
  - Done when: an integration test running the binary inside a container with a mounted socket asserts the warning in stdout, stderr, and the audit log.
  - Feature: T2.8 (mitigates H11).
- [ ] **Detect Docker socket absence in stock profile.**
  - Acceptance: An integration test under the default profile asserts the socket is absent inside both containers.
  - Done when: the test passes.
  - Feature: T2.8 (mitigates H11).

### Lint enforcement

- [ ] **Author `lint-no-socket.sh`.**
  - Path: `deploy/compose/test/lint-no-socket.sh`.
  - Acceptance: Parses both compose files with `yq` and asserts the `caddy` service has zero entries in `volumes` matching `/var/run/docker.sock` or `/run/docker.sock`. Exits non-zero on violation.
  - Done when: a CI step runs the script and exits zero.
  - Feature: T2.8 (mitigates H11).
- [ ] **Wire the lint into `just check`.**
  - Path: `justfile`.
  - Acceptance: `just check` MUST run the lint as part of the deployment-checks gate.
  - Done when: `just check` invokes the script.
  - Feature: T2.8.

### Image publishing

- [ ] **Author `.github/workflows/docker-publish.yml`.**
  - Path: `.github/workflows/docker-publish.yml`.
  - Acceptance: Triggers on `push: tags: v*` and `workflow_dispatch`. Uses `docker/setup-buildx-action`, `docker/login-action` with OIDC, builds `linux/amd64` and `linux/arm64`, pushes to `ghcr.io/gasmanc/trilithon:<tag>` plus the `<major>`, `<major>.<minor>`, `latest` tags as appropriate. Runs cosign keyless signing on the resulting digest. Generates an SPDX SBOM with Syft and attaches it as a registry attestation. Repository write authentication will be supplied at workflow-implementation time via a `GHCR_TOKEN` GitHub Actions secret; the `.github/workflows/docker-publish.yml` reads it via `${{ secrets.GHCR_TOKEN }}`.
  - Done when: a tag push produces a signed multi-arch image visible in the registry.
  - Feature: T2.8.
- [ ] **Document signature verification.**
  - Path: `deploy/compose/README.md`.
  - Acceptance: The README MUST include the cosign verification command using the workflow's identity issuer and subject pattern.
  - Done when: the section exists and the smoke test runs the verification command.
  - Feature: T2.8.

### Smoke test and lifecycle

- [ ] **Author `deploy/compose/test/smoke.sh`.**
  - Path: `deploy/compose/test/smoke.sh`.
  - Acceptance: Boots the compose stack on a fresh GitHub Actions runner, polls `http://127.0.0.1:7878/api/v1/health` until 200 OK or 30 seconds elapse, reads the bootstrap credentials from the `trilithon_data` volume, logs in, posts a route for `smoke.invalid` upstream `127.0.0.1:9999`, polls `Host: smoke.invalid` and asserts the expected upstream-error response, tears down with `docker compose down --volumes`.
  - Done when: the script exits zero in CI.
  - Feature: T2.8.
- [ ] **Author `deploy/compose/test/upgrade-from-prior.sh`.**
  - Path: `deploy/compose/test/upgrade-from-prior.sh`.
  - Acceptance: Boots the previously published image, applies a route, then upgrades to the current image with `docker compose pull && docker compose up -d`, verifies the route persists and migrations apply.
  - Done when: the test passes against the most recent published image.
  - Feature: T2.8.
- [ ] **Author `deploy/compose/UPGRADING.md`.**
  - Path: `deploy/compose/UPGRADING.md`.
  - Acceptance: Documents the four-step upgrade procedure plus the rollback story when migrations fail. MUST include the Caddy-version rationale paragraph verbatim: "Trilithon's Compose deployment pins Caddy to the latest 2.8 patch (currently `caddy:2.8-alpine`) for stability of the deployment artefact; continuous integration tests against the latest stable Caddy (currently 2.11.2 per `caddy-version.txt`) for forward-compatibility. Both pins are intentional and tracked separately."
  - Done when: the file exists, the smoke-test runbook references it, and the Caddy-version rationale paragraph is present verbatim.
  - Feature: T2.8.
- [ ] **Author `deploy/compose/README.md`.**
  - Path: `deploy/compose/README.md`.
  - Acceptance: Headings: "Prerequisites", "First run", "Bootstrap credentials", "Enabling Docker discovery", "Upgrading", "Backing up volumes", "Verifying image signatures", "Troubleshooting".
  - Done when: the file exists and a documentation lint asserts every heading is present.
  - Feature: T2.8.

### Documentation

- [ ] **Author `docs/install/compose.md`.**
  - Path: `docs/install/compose.md`.
  - Acceptance: User-facing installation page with the same headings as the deploy README plus links into Trilithon's audit log model and the H11 trust warning.
  - Done when: the file exists.
  - Feature: T2.8.

### Tests (cross-cutting)

- [ ] **Bootstrap page renders within 30 seconds (CI gate).**
  - Acceptance: An assertion inside `smoke.sh` MUST fail if the bootstrap page takes more than 30 seconds.
  - Done when: the timing is enforced.
  - Feature: T2.8.
- [ ] **Image-size budget enforced.**
  - Acceptance: The CI publish workflow asserts the 50 MB budget on every produced image.
  - Done when: a synthetic over-budget image fails CI.
  - Feature: T2.8.
- [ ] **Caddy image is unmodified upstream.**
  - Acceptance: A test in `deploy/compose/test/verify-caddy-digest.sh` MUST resolve the upstream Docker Hub digest for `caddy:2.8-alpine` and assert it matches the digest pinned in `docker-compose.yml`.
  - Done when: the test passes.
  - Feature: T2.8.
- [ ] **No-socket invariant test (default profile).**
  - Acceptance: An integration test under the default profile asserts neither container can `stat /var/run/docker.sock`.
  - Done when: the test passes.
  - Feature: T2.8 (mitigates H11).
- [ ] **Trilithon-only socket invariant (discovery profile).**
  - Acceptance: An integration test under the `docker-discovery` profile asserts the socket is `stat`-able inside `trilithon` and not inside `caddy`.
  - Done when: the test passes.
  - Feature: T2.8 (mitigates H11).

## Cross-references

- ADR-0010 (two-container deployment with unmodified official Caddy).
- ADR-0011 (loopback-only by default with explicit opt-in for remote access).
- PRD T2.8 (two-container Docker Compose deployment path).
- Architecture: "Deployment — compose," "Trust boundary — Docker socket."
- Hazards: H1 (Caddy admin endpoint exposure), H11 (Docker socket trust boundary).

## Sign-off checklist

- [ ] `just check` passes.
- [ ] `docker compose up` on a fresh host produces a working web UI on `http://127.0.0.1:7878` within 30 seconds.
- [ ] The Caddy image is an unmodified official image (digest match).
- [ ] The Trilithon image is a multi-stage Rust build on a distroless base, under 50 MB.
- [ ] The Docker socket is not visible in either container under the default profile.
- [ ] Under the `docker-discovery` profile, the socket is mounted into `trilithon` only and the trust-grant warning is printed.
- [ ] Images are signed with cosign and the smoke test verifies the signature.
- [ ] The upgrade-from-prior test passes against the most recent published image.
