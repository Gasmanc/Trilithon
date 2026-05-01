# Phase 24 — Bare-metal systemd deployment

Source of truth: [`../phases/phased-plan.md#phase-24--bare-metal-systemd-deployment`](../phases/phased-plan.md#phase-24--bare-metal-systemd-deployment).

## Pre-flight checklist

- [ ] Phase 22 complete.
- [ ] The Trilithon binary builds as a single artefact (statically linked or with a documented `glibc >= 2.36` dependency) suitable for Ubuntu 24.04 LTS and Debian 12.
- [ ] CI runners support privileged systemd containers (or a nested-virt path) for the smoke matrix.

## Tasks

### Systemd unit

- [ ] **Author `deploy/systemd/trilithon.service`.**
  - Path: `deploy/systemd/trilithon.service`.
  - Acceptance: The unit file MUST contain every directive listed in the phased-plan section verbatim, including all hardening directives: `ProtectSystem=strict`, `ProtectHome=true`, `PrivateTmp=true`, `PrivateDevices=true`, `NoNewPrivileges=true`, `LockPersonality=true`, `RestrictRealtime=true`, `RestrictSUIDSGID=true`, `RestrictNamespaces=true`, `ProtectClock=true`, `ProtectHostname=true`, `ProtectKernelLogs=true`, `ProtectKernelModules=true`, `ProtectKernelTunables=true`, `ProtectControlGroups=true`, `ProtectProc=invisible`, `ProcSubset=pid`, `CapabilityBoundingSet=`, `AmbientCapabilities=`, `SystemCallArchitectures=native`, `SystemCallFilter=@system-service`, `SystemCallFilter=~@privileged @resources @mount @debug @cpu-emulation @obsolete @raw-io`, `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6`, `IPAddressDeny=any`, `IPAddressAllow=localhost` (note: loopback only — `link-local` and mDNS are explicitly NOT permitted; V1 needs no network egress), `ReadWritePaths=/var/lib/trilithon /var/log/trilithon /run/trilithon`, `ReadOnlyPaths=/etc/trilithon`. The `[Service]` `Type` MUST be `notify`, `EnvironmentFile=-/etc/trilithon/environment`, `ExecStart=/usr/bin/trilithon daemon --config /etc/trilithon/config.toml`, `Restart=on-failure`, `RestartSec=5s`.
  - Done when: a `systemd-analyze verify` step in CI passes against the unit, and a parser test asserts every required directive.
  - Feature: T2.7.
- [ ] **CI hardening-score gate.**
  - Path: `.github/workflows/systemd-hardening.yml`.
  - Acceptance: The workflow MUST boot a fresh `ubuntu:24.04` container, install Trilithon (using the install script under test), and run `systemd-analyze security trilithon.service`. The resulting numeric exposure score MUST be ≤ 1.5 on systemd's 0–10 scale (lower is better). The job MUST fail the build if the score exceeds 1.5. The score MUST be captured into the workflow summary so regressions are visible at review time.
  - Done when: the workflow exists, runs against the unit produced by this phase, and asserts the threshold; a deliberately-weakened unit (one of the protections removed) MUST cause the job to fail in a fixture-driven self-test.
  - Feature: T2.7.
- [ ] **Author the tmpfiles.d snippets.**
  - Path: `deploy/systemd/tmpfiles.d/trilithon.conf`.
  - Acceptance: Contains `d /run/trilithon 0755 trilithon trilithon -` and `d /run/caddy 0750 caddy trilithon -`.
  - Done when: the file exists and `systemd-tmpfiles --create --dry-run` validates it.
  - Feature: T2.7.
- [ ] **Author the Caddy drop-in.**
  - Path: `deploy/systemd/caddy-drop-in/trilithon-socket.conf`.
  - Acceptance: Contents `[Service]\nUMask=0007\nReadWritePaths=/run/caddy`.
  - Done when: the file exists.
  - Feature: T2.7.
- [ ] **Author `config.toml.example`.**
  - Path: `deploy/systemd/config.toml.example`.
  - Acceptance: Documented sections `[server] bind = "127.0.0.1:7878"`, `[caddy] admin_endpoint = "unix:///run/caddy/admin.sock"`, `[caddy] version = "2.8.x"`, `[storage] data_dir = "/var/lib/trilithon"`, `[secrets] master_key_backend = "keychain | file"`.
  - Done when: the file exists.
  - Feature: T2.7.

### Install script (one task per function)

- [ ] **Implement `detect_caddy`.**
  - Path: `deploy/systemd/install.sh` (function `detect_caddy`).
  - Acceptance: Runs `caddy version`; captures the output. If `caddy` is not on `PATH`, prints the missing-Caddy message and exits 1. Parses the version with regex `^v?([0-9]+)\.([0-9]+)\.([0-9]+)`. If the major.minor is less than 2.8, prints the upgrade-required message and exits 1. Records the detected version into `/etc/trilithon/config.toml`.
  - Done when: shellcheck passes; a unit test (using a stub `caddy` binary) exercises both negative paths and the positive path.
  - Feature: T2.7.
- [ ] **Implement `install_caddy_apt_repo`.**
  - Path: `deploy/systemd/install.sh` (function `install_caddy_apt_repo`).
  - Acceptance: On Debian or Ubuntu (detected via `/etc/os-release`), prompts the user to add the official Caddy APT repository (`curl -fsSL https://dl.cloudsmith.io/public/caddy/stable/gpg.key | sudo tee /etc/apt/keyrings/caddy.asc`, write the source list, `apt-get update && apt-get install -y caddy`). On other distributions, prints manual instructions and exits 1.
  - Done when: an integration test on Ubuntu 24.04 exercises the flow with `TRILITHON_NONINTERACTIVE=1` accepting the install.
  - Feature: T2.7.
- [ ] **Implement `create_trilithon_user`.**
  - Path: `deploy/systemd/install.sh` (function `create_trilithon_user`).
  - Acceptance: Idempotently runs `groupadd --system trilithon` and `useradd --system --gid trilithon --home-dir /var/lib/trilithon --shell /usr/sbin/nologin --comment "Trilithon control plane" trilithon`. Adds the `trilithon` user to the `caddy` group via `usermod -aG caddy trilithon`.
  - Done when: a unit test on a fresh container asserts the user, group, shell, and group membership.
  - Feature: T2.7.
- [ ] **Implement `install_binary`.**
  - Path: `deploy/systemd/install.sh` (function `install_binary`).
  - Acceptance: Copies the bundled binary to `/usr/local/bin/trilithon` mode `0755` owner `root:root`. Verifies the binary's SHA-256 against an expected value.
  - Done when: an integration test asserts the binary's permissions and the SHA-256 verification.
  - Feature: T2.7.
- [ ] **Implement `seed_config`.**
  - Path: `deploy/systemd/install.sh` (function `seed_config`).
  - Acceptance: Creates `/etc/trilithon` mode `0750` owner `root:trilithon`, copies `config.toml.example` to `config.toml` mode `0640` owner `root:trilithon`, creates `/etc/trilithon/trilithon.env` mode `0640`. Creates `/var/lib/trilithon` mode `0750` owner `trilithon:trilithon` and `/var/log/trilithon` mode `0750` owner `trilithon:trilithon`.
  - Done when: integration test asserts the directories, files, and permissions.
  - Feature: T2.7.
- [ ] **Implement `start_service`.**
  - Path: `deploy/systemd/install.sh` (function `start_service`).
  - Acceptance: Drops the unit into `/etc/systemd/system/`, copies the tmpfiles snippet to `/usr/lib/tmpfiles.d/`, copies the Caddy drop-in into `/etc/systemd/system/caddy.service.d/`, runs `systemctl daemon-reload`, `systemd-tmpfiles --create`, `systemctl restart caddy`, `systemctl enable --now trilithon`.
  - Done when: an integration test asserts the active state.
  - Feature: T2.7.
- [ ] **Implement `verify_running`.**
  - Path: `deploy/systemd/install.sh` (function `verify_running`).
  - Acceptance: Polls `http://127.0.0.1:7878/api/v1/health` until 200 OK or 60 seconds elapse. Verifies `stat -c %G /run/caddy/admin.sock` reports `trilithon` (or a group `trilithon` belongs to). Verifies the Trilithon process UID matches the `trilithon` user. Aborts with a precise diagnostic on failure.
  - Done when: an integration test asserts the verification.
  - Feature: T2.7.
- [ ] **Wrap the seven functions into the install entry point.**
  - Path: `deploy/systemd/install.sh`.
  - Acceptance: The entry point MUST run the functions in the order `detect_caddy → install_caddy_apt_repo (if needed) → create_trilithon_user → install_binary → seed_config → start_service → verify_running`. Every step prints a status line.
  - Done when: shellcheck passes and the integration test runs the script end-to-end.
  - Feature: T2.7.

### Uninstall script (one task per function)

- [ ] **Implement `stop_service`.**
  - Path: `deploy/systemd/uninstall.sh` (function `stop_service`).
  - Acceptance: Runs `systemctl stop trilithon` and `systemctl disable trilithon`.
  - Done when: an integration test asserts the inactive state.
  - Feature: T2.7.
- [ ] **Implement `remove_unit_files`.**
  - Path: `deploy/systemd/uninstall.sh` (function `remove_unit_files`).
  - Acceptance: Removes `/etc/systemd/system/trilithon.service`, `/usr/lib/tmpfiles.d/trilithon.conf`, `/etc/systemd/system/caddy.service.d/trilithon-socket.conf`. Runs `systemctl daemon-reload`.
  - Done when: an integration test asserts the files are gone.
  - Feature: T2.7.
- [ ] **Implement `remove_config`.**
  - Path: `deploy/systemd/uninstall.sh` (function `remove_config`).
  - Acceptance: Removes `/etc/trilithon`.
  - Done when: an integration test asserts removal.
  - Feature: T2.7.
- [ ] **Implement `remove_data`.**
  - Path: `deploy/systemd/uninstall.sh` (function `remove_data`).
  - Acceptance: With `--remove-data` removes `/var/lib/trilithon` and `/var/log/trilithon` without prompting; without the flag, prompts interactively (default no).
  - Done when: an integration test asserts both branches.
  - Feature: T2.7.
- [ ] **Implement `remove_user`.**
  - Path: `deploy/systemd/uninstall.sh` (function `remove_user`).
  - Acceptance: Removes the `trilithon` user and group; removes the `trilithon` user's membership in the `caddy` group beforehand.
  - Done when: an integration test asserts the user is gone.
  - Feature: T2.7.
- [ ] **Implement `verify_clean`.**
  - Path: `deploy/systemd/uninstall.sh` (function `verify_clean`).
  - Acceptance: Confirms no `trilithon` files in `/etc`, `/var/lib`, `/var/log`, `/run`; no `trilithon` user; no `trilithon.service` unit. Reports residue.
  - Done when: an integration test asserts cleanup.
  - Feature: T2.7.

### OS detection and packaging

- [ ] **Detect Debian 12 vs Ubuntu 24.04.**
  - Path: `deploy/systemd/install.sh` (function `detect_os`).
  - Acceptance: Reads `/etc/os-release` and sets `OS_ID`, `OS_VERSION_ID`. Refuses to proceed on any other distribution; prints manual instructions.
  - Done when: shellcheck passes; a unit test on a stubbed `/etc/os-release` exercises positive and negative paths.
  - Feature: T2.7.
- [ ] **Build a `.deb` package.**
  - Path: `core/Cargo.toml` (under `[package.metadata.deb]`) and `.github/workflows/deb-build.yml`.
  - Acceptance: `cargo deb` produces `trilithon_<version>_amd64.deb` containing the binary, the unit, the tmpfiles snippet, the Caddy drop-in, the example config, and `postinst`/`prerm`/`postrm` hooks. The distribution to APT archives is OUT OF SCOPE FOR V1; the package is downloadable from project releases.
  - Done when: CI builds the package and `dpkg -i` followed by `systemctl status trilithon` reports active.
  - Feature: T2.7.
- [ ] **Author postinst, prerm, postrm hooks.**
  - Path: `deploy/systemd/debian/postinst`, `prerm`, `postrm`.
  - Acceptance: `postinst` runs the install-script steps idempotently and on upgrade runs `systemctl daemon-reload && systemctl restart trilithon`. `prerm` stops the service. `postrm` on `purge` removes data, user, group; on `remove` leaves data in place.
  - Done when: shellcheck passes and `dpkg --purge` followed by `verify_clean` succeeds.
  - Feature: T2.7.

### Error handling for partial installs

- [ ] **Roll back on partial-install failure.**
  - Path: `deploy/systemd/install.sh` (function `rollback_partial`).
  - Acceptance: If any step after `create_trilithon_user` fails, the rollback function MUST undo the steps already taken (remove unit files, remove `/etc/trilithon`, NOT remove `/var/lib/trilithon` because data may already be present from a prior install).
  - Done when: an integration test injecting a failure in `start_service` asserts the rollback.
  - Feature: T2.7.

### CI smoke matrix

- [ ] **Author `.github/workflows/systemd-smoke.yml`.**
  - Path: `.github/workflows/systemd-smoke.yml`.
  - Acceptance: Two matrix jobs: `ubuntu:24.04` and `debian:12`. Each runs a privileged container with systemd as PID 1, pre-installs Caddy 2.8 via APT, runs `install.sh` non-interactively, polls `/api/v1/health`, asserts the daemon UID, asserts the admin socket connection, runs `uninstall.sh --remove-data`, runs `verify_clean`.
  - Done when: the workflow passes on both matrix entries.
  - Feature: T2.7.
- [ ] **Author `deploy/systemd/test/smoke.sh`.**
  - Path: `deploy/systemd/test/smoke.sh`.
  - Acceptance: The script invoked by the CI matrix; encapsulates the seven verification steps from the phased plan.
  - Done when: shellcheck passes and CI invokes it.
  - Feature: T2.7.
- [ ] **Negative test: missing Caddy.**
  - Path: `deploy/systemd/test/smoke-no-caddy.sh`.
  - Acceptance: Runs `install.sh` on a container without Caddy and asserts exit 1 with the documented message.
  - Done when: the test passes.
  - Feature: T2.7.
- [ ] **Negative test: Caddy < 2.8.**
  - Path: `deploy/systemd/test/smoke-old-caddy.sh`.
  - Acceptance: Runs `install.sh` on a container with Caddy 2.7 (or stubbed `caddy version` output) and asserts exit 1 with the documented message.
  - Done when: the test passes.
  - Feature: T2.7.

### Documentation

- [ ] **Author `docs/install/systemd.md`.**
  - Path: `docs/install/systemd.md`.
  - Acceptance: Headings: "Prerequisites", "Caddy install", "Trilithon install", "Bootstrap credentials", "Configuration", "Logs and journald", "Upgrading", "Uninstalling", "Troubleshooting", "Hardening notes".
  - Done when: the file exists and a documentation lint asserts every heading.
  - Feature: T2.7.
- [ ] **Author `deploy/systemd/README.md`.**
  - Path: `deploy/systemd/README.md`.
  - Acceptance: Operator-facing README mirroring the install/uninstall flow.
  - Done when: the file exists.
  - Feature: T2.7.

## Cross-references

- ADR-0010 (two-container deployment with unmodified official Caddy — adjacent path).
- ADR-0011 (loopback-only by default).
- ADR-0014 (secrets vault — keychain backend interaction with hardened systemd).
- PRD T2.7 (bare-metal systemd deployment path).
- Architecture: "Deployment — systemd," "User isolation."
- Hazards: H1 (Caddy admin endpoint exposure), H13 (Bootstrap credential leak).

## Sign-off checklist

- [ ] `just check` passes.
- [ ] A fresh Ubuntu 24.04 LTS or Debian 12 system installs Trilithon in one command and has a working web UI within 60 seconds.
- [ ] The daemon runs as the dedicated `trilithon` user (verified by inspecting the running PID's UID).
- [ ] The daemon talks to Caddy over `/run/caddy/admin.sock` (verified by inspecting the daemon's open file descriptors).
- [ ] Uninstall removes the service, the user, the group, and (with confirmation) the data directory.
- [ ] The Caddy detection refuses to proceed without Caddy and on Caddy older than 2.8.
- [ ] `systemd-analyze verify` passes for `trilithon.service`.
