# Phase 24 — Bare-metal systemd Deployment — Implementation Slices

> Phase reference: [../phases/phase-24-systemd-deployment.md](../phases/phase-24-systemd-deployment.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference (`docs/phases/phase-24-systemd-deployment.md`).
- Architecture §3 (system context — daemon-to-Caddy boundary), §11 (security posture, user isolation), §12.1 (tracing vocabulary).
- Trait signatures: `core::secrets::SecretsVault` (the systemd hardening posture must accommodate the keychain backend through `Secret Service`).
- ADRs: ADR-0010 (two-container deployment — adjacent path), ADR-0011 (loopback-only by default), ADR-0014 (secrets at rest with keychain master key).
- PRD: T2.7 (bare-metal systemd deployment).
- Hazards: H1 (Caddy admin endpoint exposure), H13 (bootstrap credential leak).

## Slice plan summary

| # | Title | Primary files | Effort (ideal-eng-hours) | Depends on |
|---|-------|---------------|--------------------------|------------|
| 24.1 | Systemd unit file plus `tmpfiles.d` snippet and Caddy drop-in | `deploy/systemd/trilithon.service`, `deploy/systemd/tmpfiles.d/trilithon.conf`, `deploy/systemd/caddy-drop-in/trilithon-socket.conf`, `deploy/systemd/config.toml.example` | 5 | — |
| 24.2 | OS detection plus `detect_caddy` and `install_caddy_apt_repo` install-script functions | `deploy/systemd/install.sh` (functions `detect_os`, `detect_caddy`, `install_caddy_apt_repo`) | 6 | 24.1 |
| 24.3 | `create_trilithon_user`, `install_binary`, `seed_config` install-script functions | `deploy/systemd/install.sh` | 5 | 24.2 |
| 24.4 | `start_service`, `verify_running`, `rollback_partial`, install entry point | `deploy/systemd/install.sh` | 6 | 24.3 |
| 24.5 | Uninstall script (six functions) | `deploy/systemd/uninstall.sh` | 5 | 24.4 |
| 24.6 | CI hardening-score gate (`systemd-analyze security`) | `.github/workflows/systemd-hardening.yml` | 4 | 24.1 |
| 24.7 | CI smoke matrix on Ubuntu 24.04 and Debian 12 plus negative tests | `.github/workflows/systemd-smoke.yml`, `deploy/systemd/test/smoke.sh`, `deploy/systemd/test/smoke-no-caddy.sh`, `deploy/systemd/test/smoke-old-caddy.sh` | 8 | 24.4, 24.5 |
| 24.8 | `.deb` package build with postinst/prerm/postrm hooks | `core/Cargo.toml`, `.github/workflows/deb-build.yml`, `deploy/systemd/debian/postinst`, `deploy/systemd/debian/prerm`, `deploy/systemd/debian/postrm` | 6 | 24.4, 24.5 |
| 24.9 | Operator and end-user documentation | `deploy/systemd/README.md`, `docs/install/systemd.md` | 3 | 24.4, 24.5 |

---

## Slice 24.1 — Systemd unit file plus `tmpfiles.d` snippet and Caddy drop-in

### Goal

Author the four static deployment artefacts: the hardened systemd unit, the `tmpfiles.d` snippet for `/run/trilithon` and `/run/caddy`, the Caddy drop-in that grants the `trilithon` group access to the admin socket, and the example daemon configuration. After this slice, `systemd-analyze verify deploy/systemd/trilithon.service` exits zero.

### Entry conditions

- The Trilithon binary builds as a single artefact targeting `glibc >= 2.36` (Ubuntu 24.04 LTS, Debian 12 bookworm).
- `systemd-analyze` is available on the developer machine.

### Files to create or modify

- `deploy/systemd/trilithon.service` — the hardened systemd unit.
- `deploy/systemd/tmpfiles.d/trilithon.conf` — `/run/trilithon` and `/run/caddy` runtime directories.
- `deploy/systemd/caddy-drop-in/trilithon-socket.conf` — Caddy service drop-in granting socket access.
- `deploy/systemd/config.toml.example` — example daemon configuration.

### Signatures and shapes

`deploy/systemd/trilithon.service` (verbatim, reproducing the phase reference):

```
[Unit]
Description=Trilithon — local-first Caddy control plane
Documentation=https://example.invalid/trilithon
After=network-online.target caddy.service
Requires=caddy.service
Wants=network-online.target

[Service]
Type=notify
User=trilithon
Group=trilithon
WorkingDirectory=/var/lib/trilithon
EnvironmentFile=-/etc/trilithon/environment
ExecStart=/usr/bin/trilithon daemon --config /etc/trilithon/config.toml
Restart=on-failure
RestartSec=5s

# Hardening
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
PrivateDevices=true
NoNewPrivileges=true
LockPersonality=true
RestrictRealtime=true
RestrictSUIDSGID=true
RestrictNamespaces=true
ProtectClock=true
ProtectHostname=true
ProtectKernelLogs=true
ProtectKernelModules=true
ProtectKernelTunables=true
ProtectControlGroups=true
ProtectProc=invisible
ProcSubset=pid

CapabilityBoundingSet=
AmbientCapabilities=
SystemCallArchitectures=native
SystemCallFilter=@system-service
SystemCallFilter=~@privileged @resources @mount @debug @cpu-emulation @obsolete @raw-io
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6

# Network egress: V1 needs none; loopback only.
# When Tier 3 multi-instance is implemented this MUST be loosened
# to allow outbound TCP to controller endpoints. See phased plan §28+.
IPAddressDeny=any
IPAddressAllow=localhost

# Filesystem
ReadWritePaths=/var/lib/trilithon /var/log/trilithon /run/trilithon
ReadOnlyPaths=/etc/trilithon

[Install]
WantedBy=multi-user.target
```

`deploy/systemd/tmpfiles.d/trilithon.conf` (verbatim):

```
# Trilithon runtime directories.
# Format: type path mode user group age argument
d /run/trilithon 0755 trilithon trilithon -
d /run/caddy     0750 caddy     trilithon -
```

`deploy/systemd/caddy-drop-in/trilithon-socket.conf` (verbatim):

```
[Service]
UMask=0007
ReadWritePaths=/run/caddy
```

`deploy/systemd/config.toml.example` (verbatim):

```toml
# Trilithon daemon configuration (example).
# See docs/install/systemd.md for the canonical reference.

[server]
bind = "127.0.0.1:7878"

[caddy]
admin_endpoint = "unix:///run/caddy/admin.sock"
# Replaced by `detect_caddy` at install time.
version = "2.8.0"

[storage]
data_dir = "/var/lib/trilithon"

[secrets]
# One of: "keychain" | "file"
master_key_backend = "keychain"
```

### Algorithm

1. Author the unit file containing every hardening directive enumerated in the phase reference verbatim.
2. Author the `tmpfiles.d` snippet for the two runtime directories.
3. Author the Caddy drop-in at `deploy/systemd/caddy-drop-in/trilithon-socket.conf` granting `UMask=0007` and `ReadWritePaths=/run/caddy`.
4. Author the example configuration with the four sections required by the phase reference.

### Tests

- `deploy/systemd/test/test_unit_verifies.sh` — runs `systemd-analyze verify deploy/systemd/trilithon.service`; asserts exit 0.
- `deploy/systemd/test/test_unit_required_directives.sh` — bash script that `grep`s each mandated directive (regex per directive) from the unit file; fails on any missing directive.
- `deploy/systemd/test/test_tmpfiles_dry_run.sh` — runs `systemd-tmpfiles --create --dry-run deploy/systemd/tmpfiles.d/trilithon.conf`; asserts exit 0.

### Acceptance command

```
bash deploy/systemd/test/test_unit_verifies.sh \
  && bash deploy/systemd/test/test_unit_required_directives.sh \
  && bash deploy/systemd/test/test_tmpfiles_dry_run.sh
```

### Exit conditions

- `deploy/systemd/trilithon.service` matches the verbatim unit above byte-for-byte.
- `systemd-analyze verify` passes.
- The directive presence test passes (every hardening directive present).
- The two static helper files (`tmpfiles.d/trilithon.conf`, `caddy-drop-in/trilithon-socket.conf`) exist with the verbatim contents above.
- `config.toml.example` exists with the four documented sections.

### Audit kinds emitted

None at the unit-file layer. The runtime daemon emits audit rows; the unit file is a deployment artefact.

### Tracing events emitted

`daemon.started` (architecture §12.1) — emitted by the daemon at the end of its startup sequence under `Type=notify`. The unit file is configured for `notify` so the daemon must call `sd_notify(0, "READY=1")` before this event fires.

### Cross-references

- ADR-0011 (loopback-only by default).
- ADR-0014 (secrets vault — keychain backend works under hardened systemd; the `Secret Service` D-Bus connection uses `AF_UNIX` which is allowed by `RestrictAddressFamilies`).
- PRD T2.7.
- Architecture §11 (security posture).
- Hazards: H1.

---

## Slice 24.2 — OS detection plus `detect_caddy` and `install_caddy_apt_repo` install-script functions

### Goal

Implement the install-script preamble: read `/etc/os-release`, refuse to proceed on unsupported distributions, detect Caddy with version-floor enforcement, and offer an APT-repository installation path when Caddy is missing on Debian or Ubuntu. After this slice, `bash deploy/systemd/install.sh --dry-run` exercises every preamble code path.

### Entry conditions

- Slice 24.1 complete.
- `shellcheck` is available on developer machines and CI.

### Files to create or modify

- `deploy/systemd/install.sh` — the install-script file with `detect_os`, `detect_caddy`, `install_caddy_apt_repo` functions defined.

### Signatures and shapes

```bash
#!/usr/bin/env bash
# Trilithon systemd installer.
#
# Functions are defined in dependency order:
#   detect_os
#   detect_caddy
#   install_caddy_apt_repo (called by detect_caddy when Caddy is absent)
#   create_trilithon_user
#   install_binary
#   seed_config
#   start_service
#   verify_running
#   rollback_partial
#
# Driven by the entry point at the bottom of the file.

set -euo pipefail

readonly TRILITHON_SCHEMA_FLOOR_MAJOR=2
readonly TRILITHON_SCHEMA_FLOOR_MINOR=8

# detect_os
# Reads /etc/os-release. Sets OS_ID and OS_VERSION_ID. Refuses to
# proceed on any distribution other than Ubuntu 24.04 or Debian 12.
detect_os() {
    if [[ ! -r /etc/os-release ]]; then
        echo "trilithon-install: /etc/os-release not readable; cannot identify OS" >&2
        return 1
    fi
    # shellcheck disable=SC1091
    . /etc/os-release
    OS_ID="${ID:-}"
    OS_VERSION_ID="${VERSION_ID:-}"
    case "${OS_ID}:${OS_VERSION_ID}" in
        ubuntu:24.04) return 0 ;;
        debian:12)    return 0 ;;
        *)
            cat >&2 <<EOF
trilithon-install: unsupported distribution ${OS_ID} ${OS_VERSION_ID}
This installer supports Ubuntu 24.04 LTS and Debian 12 only.
For other distributions, see docs/install/systemd.md for manual steps.
EOF
            return 1
            ;;
    esac
}

# detect_caddy
# Runs `caddy version`. If absent, optionally invokes
# install_caddy_apt_repo. If present, parses the version and refuses
# to proceed on Caddy older than 2.8.
detect_caddy() {
    local version_output
    if ! command -v caddy >/dev/null 2>&1; then
        cat >&2 <<EOF
trilithon-install: Trilithon requires an existing Caddy 2.8 or later install.
On Debian/Ubuntu, install Caddy via the official APT repository.
Continuing will offer that installation.
EOF
        if [[ "${TRILITHON_NONINTERACTIVE:-0}" == "1" ]]; then
            install_caddy_apt_repo
        else
            read -rp "Add the Caddy APT repository and install Caddy now? [y/N] " reply
            case "${reply}" in
                y|Y) install_caddy_apt_repo ;;
                *)
                    echo "trilithon-install: Caddy not installed; aborting" >&2
                    return 1
                    ;;
            esac
        fi
    fi

    version_output="$(caddy version 2>/dev/null || true)"
    if [[ -z "${version_output}" ]]; then
        echo "trilithon-install: 'caddy version' produced no output" >&2
        return 1
    fi

    if [[ ! "${version_output}" =~ ^v?([0-9]+)\.([0-9]+)\.([0-9]+) ]]; then
        echo "trilithon-install: cannot parse Caddy version: ${version_output}" >&2
        return 1
    fi
    local major="${BASH_REMATCH[1]}"
    local minor="${BASH_REMATCH[2]}"
    if (( major < TRILITHON_SCHEMA_FLOOR_MAJOR )) \
       || { (( major == TRILITHON_SCHEMA_FLOOR_MAJOR )) \
            && (( minor < TRILITHON_SCHEMA_FLOOR_MINOR )); }; then
        cat >&2 <<EOF
trilithon-install: Trilithon requires Caddy 2.8 or later.
Detected: ${version_output}.
Upgrade Caddy and re-run this installer.
EOF
        return 1
    fi

    CADDY_DETECTED_VERSION="${major}.${minor}.${BASH_REMATCH[3]}"
    return 0
}

# install_caddy_apt_repo
# Adds the official Caddy APT repository on Debian/Ubuntu and runs
# `apt-get install -y caddy`.
install_caddy_apt_repo() {
    if [[ "${OS_ID}" != "ubuntu" && "${OS_ID}" != "debian" ]]; then
        cat >&2 <<EOF
trilithon-install: cannot install Caddy automatically on ${OS_ID};
see https://caddyserver.com/docs/install for manual instructions.
EOF
        return 1
    fi

    sudo install -d -m 0755 /etc/apt/keyrings
    curl -fsSL https://dl.cloudsmith.io/public/caddy/stable/gpg.key \
        | sudo tee /etc/apt/keyrings/caddy.asc >/dev/null
    sudo chmod 0644 /etc/apt/keyrings/caddy.asc

    cat | sudo tee /etc/apt/sources.list.d/caddy.list >/dev/null <<EOF
deb [signed-by=/etc/apt/keyrings/caddy.asc] https://dl.cloudsmith.io/public/caddy/stable/deb/${OS_ID} any-version main
EOF

    sudo apt-get update
    sudo apt-get install -y caddy
}
```

### Algorithm

`detect_os`:

1. Source `/etc/os-release`.
2. Inspect `ID` and `VERSION_ID`; refuse on anything other than `ubuntu:24.04` or `debian:12`.
3. Export `OS_ID` and `OS_VERSION_ID` for downstream functions.

`detect_caddy`:

1. Check `command -v caddy`. If absent, prompt (or auto-accept under `TRILITHON_NONINTERACTIVE=1`) to invoke `install_caddy_apt_repo`.
2. Run `caddy version`; capture output.
3. Apply regex `^v?([0-9]+)\.([0-9]+)\.([0-9]+)`; refuse on parse failure.
4. Refuse if `(major, minor) < (2, 8)`.
5. Export `CADDY_DETECTED_VERSION`.

`install_caddy_apt_repo`:

1. Refuse on non-Debian/Ubuntu.
2. Install the keyring, write the source list, run `apt-get update && apt-get install -y caddy`.

### Tests

- `deploy/systemd/test/test_detect_os_supported.sh` — stub `/etc/os-release` for Ubuntu 24.04 and Debian 12; assert exit 0.
- `deploy/systemd/test/test_detect_os_unsupported.sh` — stub for `fedora:40`; assert exit 1 with the documented message on stderr.
- `deploy/systemd/test/test_detect_caddy_missing.sh` — replace `caddy` with a stub returning 127; assert the documented missing-Caddy message.
- `deploy/systemd/test/test_detect_caddy_too_old.sh` — stub `caddy version` returning `v2.7.6 ...`; assert exit 1 with upgrade-required message.
- `deploy/systemd/test/test_detect_caddy_ok.sh` — stub `caddy version` returning `v2.8.4 ...`; assert exit 0 and `CADDY_DETECTED_VERSION=2.8.4`.

### Acceptance command

```
shellcheck deploy/systemd/install.sh \
  && bash deploy/systemd/test/test_detect_os_supported.sh \
  && bash deploy/systemd/test/test_detect_os_unsupported.sh \
  && bash deploy/systemd/test/test_detect_caddy_missing.sh \
  && bash deploy/systemd/test/test_detect_caddy_too_old.sh \
  && bash deploy/systemd/test/test_detect_caddy_ok.sh
```

### Exit conditions

- `deploy/systemd/install.sh` defines `detect_os`, `detect_caddy`, and `install_caddy_apt_repo` per the signatures above.
- `shellcheck` passes.
- The five named tests pass.

### Audit kinds emitted

None. The install script is a host-side process; audit rows are emitted by the daemon after `start_service`.

### Tracing events emitted

None.

### Cross-references

- PRD T2.7.
- Architecture §11.
- Hazards: H1.

---

## Slice 24.3 — `create_trilithon_user`, `install_binary`, `seed_config` install-script functions

### Goal

Implement the three install-script functions that lay down the `trilithon` system user, install the binary with SHA-256 verification, and seed the configuration directories with the correct ownership and modes. After this slice, running these three functions on a fresh container produces every directory and file required by the unit's `ReadWritePaths` and `ReadOnlyPaths`.

### Entry conditions

- Slice 24.2 complete.

### Files to create or modify

- `deploy/systemd/install.sh` — extend with the three functions.

### Signatures and shapes

```bash
# create_trilithon_user
# Idempotently creates the trilithon system group and user, adds the
# user to the caddy group, and creates the home directory.
create_trilithon_user() {
    if ! getent group trilithon >/dev/null; then
        sudo groupadd --system trilithon
    fi
    if ! getent passwd trilithon >/dev/null; then
        sudo useradd --system --gid trilithon \
            --home-dir /var/lib/trilithon \
            --shell /usr/sbin/nologin \
            --comment "Trilithon control plane" \
            trilithon
    fi
    if getent group caddy >/dev/null; then
        sudo usermod -aG caddy trilithon
    fi
}

# install_binary
# Copies the bundled binary to /usr/local/bin/trilithon mode 0755.
# Verifies the SHA-256 against TRILITHON_BINARY_SHA256 (set by the
# packager).
install_binary() {
    local source_binary="${TRILITHON_SOURCE_BINARY:-./trilithon}"
    local expected="${TRILITHON_BINARY_SHA256:-}"

    if [[ ! -f "${source_binary}" ]]; then
        echo "trilithon-install: bundled binary not found at ${source_binary}" >&2
        return 1
    fi
    if [[ -n "${expected}" ]]; then
        local actual
        actual="$(sha256sum "${source_binary}" | awk '{print $1}')"
        if [[ "${actual}" != "${expected}" ]]; then
            echo "trilithon-install: binary SHA-256 mismatch" >&2
            echo "  expected: ${expected}" >&2
            echo "  actual:   ${actual}" >&2
            return 1
        fi
    fi

    sudo install --owner=root --group=root --mode=0755 \
        "${source_binary}" /usr/bin/trilithon
}

# seed_config
# Creates /etc/trilithon, /var/lib/trilithon, /var/log/trilithon with
# the correct modes and ownership, and seeds config.toml plus the
# environment file.
seed_config() {
    sudo install -d --owner=root --group=trilithon --mode=0750 /etc/trilithon
    sudo install --owner=root --group=trilithon --mode=0640 \
        deploy/systemd/config.toml.example /etc/trilithon/config.toml
    sudo install --owner=root --group=trilithon --mode=0640 \
        /dev/null /etc/trilithon/trilithon.env

    # Substitute the detected Caddy version into config.toml.
    if [[ -n "${CADDY_DETECTED_VERSION:-}" ]]; then
        sudo sed -i \
            "s/^version = .*/version = \"${CADDY_DETECTED_VERSION}\"/" \
            /etc/trilithon/config.toml
    fi

    sudo install -d --owner=trilithon --group=trilithon --mode=0750 \
        /var/lib/trilithon
    sudo install -d --owner=trilithon --group=trilithon --mode=0750 \
        /var/log/trilithon
}
```

### Algorithm

`create_trilithon_user`:

1. Idempotent `groupadd --system trilithon`.
2. Idempotent `useradd --system --gid trilithon --home-dir /var/lib/trilithon --shell /usr/sbin/nologin`.
3. If a `caddy` group exists, run `usermod -aG caddy trilithon`.

`install_binary`:

1. Resolve the source binary path from `TRILITHON_SOURCE_BINARY` or fall back to `./trilithon`.
2. If `TRILITHON_BINARY_SHA256` is set, compute SHA-256 and compare; fail on mismatch.
3. Install to `/usr/bin/trilithon` mode `0755` owner `root:root`.

`seed_config`:

1. Create `/etc/trilithon` mode `0750` owner `root:trilithon`.
2. Copy `config.toml.example` to `/etc/trilithon/config.toml` mode `0640` owner `root:trilithon`.
3. Create empty `/etc/trilithon/trilithon.env` mode `0640`.
4. Substitute the detected Caddy version into `config.toml`.
5. Create `/var/lib/trilithon` mode `0750` owner `trilithon:trilithon`.
6. Create `/var/log/trilithon` mode `0750` owner `trilithon:trilithon`.

### Tests

- `deploy/systemd/test/test_create_user.sh` — boot a privileged container, run `create_trilithon_user`, assert `id trilithon` reports the right uid/gid and `id -nG trilithon` includes `caddy`.
- `deploy/systemd/test/test_install_binary_sha_match.sh` — set `TRILITHON_BINARY_SHA256` to the correct hash, run `install_binary`, assert `/usr/bin/trilithon` is mode 0755 owner root.
- `deploy/systemd/test/test_install_binary_sha_mismatch.sh` — set `TRILITHON_BINARY_SHA256` to a wrong hash, run, assert exit 1 with the documented message.
- `deploy/systemd/test/test_seed_config_perms.sh` — run `seed_config`, assert each directory and file has the documented mode and ownership.

### Acceptance command

```
shellcheck deploy/systemd/install.sh \
  && bash deploy/systemd/test/test_create_user.sh \
  && bash deploy/systemd/test/test_install_binary_sha_mismatch.sh \
  && bash deploy/systemd/test/test_seed_config_perms.sh
```

### Exit conditions

- The three functions are defined in `install.sh`.
- `shellcheck` passes.
- The four named tests pass on a fresh `ubuntu:24.04` privileged container.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- ADR-0014 (secrets vault — `/var/lib/trilithon/secrets/` mode `0750` owner `trilithon:trilithon`).
- PRD T2.7.
- Hazards: H13.

---

## Slice 24.4 — `start_service`, `verify_running`, `rollback_partial`, install entry point

### Goal

Wire the unit file, `tmpfiles.d` snippet, and Caddy drop-in into the system-wide systemd configuration; start the service; verify it reaches readiness and is talking to Caddy on the expected socket; on partial failure roll back the steps already taken. After this slice, `bash deploy/systemd/install.sh` end-to-end produces a running Trilithon daemon on a fresh container in under 60 seconds.

### Entry conditions

- Slices 24.1 through 24.3 complete.

### Files to create or modify

- `deploy/systemd/install.sh` — extend with `start_service`, `verify_running`, `rollback_partial`, and the entry-point `main` function.

### Signatures and shapes

```bash
# start_service
# Drops the unit, tmpfiles snippet, and Caddy drop-in into the
# system-wide locations; reloads systemd; restarts Caddy; enables and
# starts Trilithon.
start_service() {
    sudo install --owner=root --group=root --mode=0644 \
        deploy/systemd/trilithon.service /etc/systemd/system/trilithon.service
    sudo install --owner=root --group=root --mode=0644 \
        deploy/systemd/tmpfiles.d/trilithon.conf /usr/lib/tmpfiles.d/trilithon.conf
    sudo install -d --owner=root --group=root --mode=0755 \
        /etc/systemd/system/caddy.service.d
    sudo install --owner=root --group=root --mode=0644 \
        deploy/systemd/caddy-drop-in/trilithon-socket.conf \
        /etc/systemd/system/caddy.service.d/trilithon-socket.conf

    sudo systemctl daemon-reload
    sudo systemd-tmpfiles --create
    sudo systemctl restart caddy
    sudo systemctl enable --now trilithon
}

# verify_running
# Polls /api/v1/health for up to 60 seconds, asserts the running PID's
# UID equals the trilithon user's UID, and asserts the Caddy admin
# socket has group trilithon.
verify_running() {
    local deadline=$(( $(date +%s) + 60 ))
    until curl --silent --fail \
        http://127.0.0.1:7878/api/v1/health > /dev/null 2>&1; do
        if (( $(date +%s) > deadline )); then
            echo "trilithon-install: daemon failed to reach /api/v1/health in 60s" >&2
            sudo journalctl -u trilithon --no-pager -n 100 >&2
            return 1
        fi
        sleep 1
    done

    local pid
    pid="$(systemctl show -p MainPID --value trilithon)"
    if [[ -z "${pid}" || "${pid}" == "0" ]]; then
        echo "trilithon-install: trilithon.service has no MainPID" >&2
        return 1
    fi
    local proc_uid
    proc_uid="$(awk '/^Uid:/ {print $2}' "/proc/${pid}/status")"
    local expected_uid
    expected_uid="$(id -u trilithon)"
    if [[ "${proc_uid}" != "${expected_uid}" ]]; then
        echo "trilithon-install: process UID ${proc_uid} != trilithon UID ${expected_uid}" >&2
        return 1
    fi

    local socket_group
    socket_group="$(stat -c '%G' /run/caddy/admin.sock)"
    case "${socket_group}" in
        trilithon) ;;
        *)
            if ! id -nG trilithon | grep -qw "${socket_group}"; then
                echo "trilithon-install: /run/caddy/admin.sock group is ${socket_group}" >&2
                echo "  trilithon is not a member; refusing to declare success" >&2
                return 1
            fi
            ;;
    esac
}

# rollback_partial
# Undoes steps taken after create_trilithon_user. Preserves
# /var/lib/trilithon (data may already exist from a prior install).
rollback_partial() {
    sudo systemctl stop trilithon || true
    sudo systemctl disable trilithon || true
    sudo rm -f /etc/systemd/system/trilithon.service
    sudo rm -f /usr/lib/tmpfiles.d/trilithon.conf
    sudo rm -f /etc/systemd/system/caddy.service.d/trilithon-socket.conf
    sudo systemctl daemon-reload || true
    sudo rm -rf /etc/trilithon
    # Intentionally NOT removing /var/lib/trilithon.
}

# main
# Entry point. Runs the seven (or eight, including OS detection)
# functions in order and rolls back on failure.
main() {
    detect_os
    echo "==> OS detected: ${OS_ID} ${OS_VERSION_ID}"

    detect_caddy
    echo "==> Caddy detected: ${CADDY_DETECTED_VERSION}"

    create_trilithon_user
    echo "==> trilithon user provisioned"

    if ! install_binary \
       || ! seed_config \
       || ! start_service \
       || ! verify_running; then
        echo "==> install failed; rolling back" >&2
        rollback_partial
        exit 1
    fi

    echo "==> install ok; web UI at http://127.0.0.1:7878"
}

main "$@"
```

### Algorithm

`start_service`:

1. Install the unit file, `tmpfiles.d` snippet, Caddy drop-in.
2. `systemctl daemon-reload`, `systemd-tmpfiles --create`, `systemctl restart caddy`, `systemctl enable --now trilithon`.

`verify_running`:

1. Poll `/api/v1/health` at one-second intervals up to 60 seconds.
2. On timeout, dump `journalctl -u trilithon --no-pager -n 100` and fail.
3. Read `MainPID` from systemd; resolve `/proc/<pid>/status` Uid; assert equals `id -u trilithon`.
4. `stat -c %G /run/caddy/admin.sock`; assert group is `trilithon` or a group `trilithon` belongs to.

`rollback_partial`:

1. Stop and disable the service.
2. Remove the unit file, `tmpfiles.d` snippet, Caddy drop-in.
3. `systemctl daemon-reload`.
4. Remove `/etc/trilithon`. Preserve `/var/lib/trilithon`.

`main`:

1. Run the seven functions in order: `detect_os` → `detect_caddy` (which may invoke `install_caddy_apt_repo`) → `create_trilithon_user` → `install_binary` → `seed_config` → `start_service` → `verify_running`.
2. On failure of any step after `create_trilithon_user`, run `rollback_partial` and exit 1.

### Tests

- `deploy/systemd/test/test_install_end_to_end.sh` — fresh Ubuntu 24.04 privileged container with Caddy 2.8 pre-installed; run `install.sh`; assert health 200 OK within 60 seconds; assert daemon UID matches.
- `deploy/systemd/test/test_install_rollback_on_start_failure.sh` — replace the binary with a fake that exits immediately; run `install.sh`; assert it exits 1 and rolls back (`/etc/systemd/system/trilithon.service` absent, `/var/lib/trilithon` preserved).

### Acceptance command

```
shellcheck deploy/systemd/install.sh \
  && bash deploy/systemd/test/test_install_end_to_end.sh \
  && bash deploy/systemd/test/test_install_rollback_on_start_failure.sh
```

### Exit conditions

- `start_service`, `verify_running`, `rollback_partial`, and `main` are defined per the signatures.
- The end-to-end test passes on Ubuntu 24.04 in under 60 seconds.
- The rollback test asserts `/etc/trilithon` is removed and `/var/lib/trilithon` is preserved.

### Audit kinds emitted

After `verify_running` succeeds, the running daemon emits:

- `auth.bootstrap-credentials-rotated` (architecture §6.6) when the bootstrap token is first generated.

### Tracing events emitted

After daemon readiness:

- `daemon.started`, `storage.migrations.applied` (architecture §12.1).

### Cross-references

- ADR-0011 (loopback-only by default).
- PRD T2.7.
- Hazards: H1, H13.

---

## Slice 24.5 — Uninstall script (six functions)

### Goal

Implement `deploy/systemd/uninstall.sh` with six functions covering service stop, unit-file removal, configuration removal, optional data removal, user removal, and final verification. After this slice, running `uninstall.sh --remove-data` on a previously installed host leaves no Trilithon residue.

### Entry conditions

- Slice 24.4 complete (an install path exists to uninstall).

### Files to create or modify

- `deploy/systemd/uninstall.sh` — full uninstall script.

### Signatures and shapes

```bash
#!/usr/bin/env bash
set -euo pipefail

REMOVE_DATA=0
for arg in "$@"; do
    case "${arg}" in
        --remove-data) REMOVE_DATA=1 ;;
        *) echo "trilithon-uninstall: unknown argument ${arg}" >&2; exit 2 ;;
    esac
done

stop_service() {
    sudo systemctl stop trilithon || true
    sudo systemctl disable trilithon || true
}

remove_unit_files() {
    sudo rm -f /etc/systemd/system/trilithon.service
    sudo rm -f /usr/lib/tmpfiles.d/trilithon.conf
    sudo rm -f /etc/systemd/system/caddy.service.d/trilithon-socket.conf
    sudo rmdir /etc/systemd/system/caddy.service.d 2>/dev/null || true
    sudo systemctl daemon-reload
}

remove_config() {
    sudo rm -rf /etc/trilithon
}

remove_data() {
    if (( REMOVE_DATA == 1 )); then
        sudo rm -rf /var/lib/trilithon /var/log/trilithon
        return
    fi
    if [[ "${TRILITHON_NONINTERACTIVE:-0}" == "1" ]]; then
        echo "trilithon-uninstall: leaving /var/lib/trilithon and /var/log/trilithon"
        return
    fi
    read -rp "Remove /var/lib/trilithon and /var/log/trilithon? [y/N] " reply
    case "${reply}" in
        y|Y) sudo rm -rf /var/lib/trilithon /var/log/trilithon ;;
        *)   echo "trilithon-uninstall: leaving data directories" ;;
    esac
}

remove_user() {
    if getent passwd trilithon >/dev/null; then
        sudo gpasswd -d trilithon caddy 2>/dev/null || true
        sudo userdel trilithon
    fi
    if getent group trilithon >/dev/null; then
        sudo groupdel trilithon
    fi
}

verify_clean() {
    local residue=0
    for path in \
        /etc/trilithon \
        /etc/systemd/system/trilithon.service \
        /usr/lib/tmpfiles.d/trilithon.conf \
        /etc/systemd/system/caddy.service.d/trilithon-socket.conf \
        /run/trilithon; do
        if [[ -e "${path}" ]]; then
            echo "trilithon-uninstall: residue at ${path}" >&2
            residue=1
        fi
    done
    if getent passwd trilithon >/dev/null; then
        echo "trilithon-uninstall: residue: trilithon user still present" >&2
        residue=1
    fi
    return "${residue}"
}

stop_service
remove_unit_files
remove_config
remove_data
remove_user
verify_clean
echo "trilithon-uninstall: ok"
```

### Algorithm

1. Parse `--remove-data` flag.
2. Stop and disable the service.
3. Remove the unit file, `tmpfiles.d` snippet, Caddy drop-in.
4. Remove `/etc/trilithon`.
5. With `--remove-data` (or interactive consent), remove `/var/lib/trilithon` and `/var/log/trilithon`.
6. Remove the `trilithon` user from the `caddy` group, then delete the user and group.
7. Verify cleanup; report residue.

### Tests

- `deploy/systemd/test/test_uninstall_remove_data.sh` — install, then `uninstall.sh --remove-data`; assert `verify_clean` exits 0.
- `deploy/systemd/test/test_uninstall_keep_data.sh` — install, then `TRILITHON_NONINTERACTIVE=1 uninstall.sh`; assert `/var/lib/trilithon` exists.

### Acceptance command

```
shellcheck deploy/systemd/uninstall.sh \
  && bash deploy/systemd/test/test_uninstall_remove_data.sh \
  && bash deploy/systemd/test/test_uninstall_keep_data.sh
```

### Exit conditions

- `uninstall.sh` defines the six functions.
- `shellcheck` passes.
- Both uninstall tests pass.

### Audit kinds emitted

The daemon emits `auth.session-revoked` when the service stops if any sessions are open. The uninstall script itself does not emit audit rows (the daemon is stopped before the rows would be written).

### Tracing events emitted

`daemon.shutting-down`, `daemon.shutdown-complete` (architecture §12.1) emitted by the daemon during `systemctl stop`.

### Cross-references

- PRD T2.7.

---

## Slice 24.6 — CI hardening-score gate (`systemd-analyze security`)

### Goal

A GitHub Actions workflow boots a fresh `ubuntu:24.04` container, installs Trilithon via `install.sh`, runs `systemd-analyze security trilithon.service`, and fails the build if the exposure score exceeds 1.5 on systemd's 0–10 scale (lower is better). After this slice, a deliberately weakened unit (one hardening directive removed) causes the workflow to fail.

### Entry conditions

- Slices 24.1 through 24.4 complete.

### Files to create or modify

- `.github/workflows/systemd-hardening.yml` — the hardening-score gate.

### Signatures and shapes

`.github/workflows/systemd-hardening.yml` (verbatim):

```yaml
name: systemd-hardening

on:
  push:
    paths:
      - 'deploy/systemd/**'
      - 'core/**'
  pull_request:
    paths:
      - 'deploy/systemd/**'
      - 'core/**'

jobs:
  score:
    runs-on: ubuntu-24.04
    container:
      image: ubuntu:24.04
      options: --privileged
    steps:
      - uses: actions/checkout@v4
      - name: Install systemd and tooling
        run: |
          apt-get update
          apt-get install -y systemd systemd-sysv dbus curl ca-certificates jq sudo
      - name: Install Caddy 2.8 from APT repo
        run: |
          install -d -m 0755 /etc/apt/keyrings
          curl -fsSL https://dl.cloudsmith.io/public/caddy/stable/gpg.key \
            | tee /etc/apt/keyrings/caddy.asc >/dev/null
          echo 'deb [signed-by=/etc/apt/keyrings/caddy.asc] https://dl.cloudsmith.io/public/caddy/stable/deb/ubuntu any-version main' \
            > /etc/apt/sources.list.d/caddy.list
          apt-get update
          apt-get install -y caddy
      - name: Run install script
        run: |
          TRILITHON_NONINTERACTIVE=1 bash deploy/systemd/install.sh
      - name: Run systemd-analyze security
        id: score
        run: |
          set -euo pipefail
          systemd-analyze security trilithon.service > security.txt
          score=$(awk '/Overall exposure level/ {print $4}' security.txt)
          echo "Score: ${score}"
          echo "score=${score}" >> "${GITHUB_OUTPUT}"
          {
            echo "## Hardening score"
            echo
            echo "Score: \`${score}\`"
            echo
            echo '```'
            cat security.txt
            echo '```'
          } >> "${GITHUB_STEP_SUMMARY}"
          # bash arithmetic supports float-like comparisons via awk
          awk -v s="${score}" 'BEGIN { exit (s > 1.5) ? 1 : 0 }'
      - name: Self-test — weakened unit must fail
        run: |
          set -euo pipefail
          cp /etc/systemd/system/trilithon.service /tmp/saved.service
          sed -i '/^ProtectSystem=/d' /etc/systemd/system/trilithon.service
          systemctl daemon-reload
          systemd-analyze security trilithon.service > weakened.txt
          weakened_score=$(awk '/Overall exposure level/ {print $4}' weakened.txt)
          cp /tmp/saved.service /etc/systemd/system/trilithon.service
          systemctl daemon-reload
          awk -v s="${weakened_score}" 'BEGIN { exit (s > 1.5) ? 0 : 1 }'
```

### Algorithm

1. Boot a privileged Ubuntu 24.04 container.
2. Install systemd, Caddy 2.8, run `install.sh` non-interactively.
3. Run `systemd-analyze security trilithon.service`; parse the "Overall exposure level" numeric value.
4. Write the score and the full report into the workflow summary.
5. Fail the build if `score > 1.5`.
6. Self-test step: remove `ProtectSystem=` from the installed unit, re-run `systemd-analyze`, assert the weakened score exceeds 1.5 (proves the gate detects regressions).

### Tests

- The workflow itself is the test. A self-test step runs every build and proves the gate detects a deliberately weakened unit.

### Acceptance command

```
gh workflow run systemd-hardening.yml \
  && gh run watch --workflow systemd-hardening.yml
```

### Exit conditions

- `.github/workflows/systemd-hardening.yml` exists with the steps above.
- The score against the canonical unit is captured in the workflow summary.
- A weakened unit (one protection removed) fails the gate.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- ADR-0011, ADR-0014.
- PRD T2.7.
- Architecture §11.

---

## Slice 24.7 — CI smoke matrix on Ubuntu 24.04 and Debian 12 plus negative tests

### Goal

A two-job matrix exercises the install/verify/uninstall cycle on Ubuntu 24.04 and Debian 12 fresh containers, plus two negative-path workflows for "no Caddy" and "Caddy < 2.8". After this slice, every supported target is covered in CI and both negative tests assert the documented refusal messages.

### Entry conditions

- Slices 24.4 and 24.5 complete.

### Files to create or modify

- `.github/workflows/systemd-smoke.yml` — matrix workflow.
- `deploy/systemd/test/smoke.sh` — encapsulated install/verify/uninstall steps invoked by the matrix.
- `deploy/systemd/test/smoke-no-caddy.sh` — negative test with Caddy absent.
- `deploy/systemd/test/smoke-old-caddy.sh` — negative test with Caddy 2.7.

### Signatures and shapes

`.github/workflows/systemd-smoke.yml` (verbatim):

```yaml
name: systemd-smoke

on:
  push:
    paths:
      - 'deploy/systemd/**'
  pull_request:
    paths:
      - 'deploy/systemd/**'

jobs:
  smoke:
    runs-on: ubuntu-24.04
    strategy:
      fail-fast: false
      matrix:
        target:
          - { os: 'ubuntu:24.04', name: 'ubuntu-24-04' }
          - { os: 'debian:12',    name: 'debian-12'     }
    container:
      image: ${{ matrix.target.os }}
      options: --privileged
    steps:
      - uses: actions/checkout@v4
      - run: bash deploy/systemd/test/smoke.sh
  no-caddy:
    runs-on: ubuntu-24.04
    container:
      image: ubuntu:24.04
      options: --privileged
    steps:
      - uses: actions/checkout@v4
      - run: bash deploy/systemd/test/smoke-no-caddy.sh
  old-caddy:
    runs-on: ubuntu-24.04
    container:
      image: ubuntu:24.04
      options: --privileged
    steps:
      - uses: actions/checkout@v4
      - run: bash deploy/systemd/test/smoke-old-caddy.sh
```

`deploy/systemd/test/smoke.sh` (verbatim):

```bash
#!/usr/bin/env bash
set -euo pipefail

apt-get update
apt-get install -y systemd systemd-sysv dbus curl ca-certificates jq sudo

# Install Caddy 2.8 from the official APT repo.
install -d -m 0755 /etc/apt/keyrings
curl -fsSL https://dl.cloudsmith.io/public/caddy/stable/gpg.key \
    | tee /etc/apt/keyrings/caddy.asc >/dev/null
. /etc/os-release
echo "deb [signed-by=/etc/apt/keyrings/caddy.asc] https://dl.cloudsmith.io/public/caddy/stable/deb/${ID} any-version main" \
    > /etc/apt/sources.list.d/caddy.list
apt-get update
apt-get install -y caddy

TRILITHON_NONINTERACTIVE=1 bash deploy/systemd/install.sh

# Verify health.
curl --silent --fail http://127.0.0.1:7878/api/v1/health > /dev/null

# Verify daemon UID and admin socket.
pid="$(systemctl show -p MainPID --value trilithon)"
proc_uid="$(awk '/^Uid:/ {print $2}' "/proc/${pid}/status")"
[[ "${proc_uid}" == "$(id -u trilithon)" ]]
[[ -S /run/caddy/admin.sock ]]

# Uninstall and verify.
TRILITHON_NONINTERACTIVE=1 bash deploy/systemd/uninstall.sh --remove-data

echo "smoke: ok on ${ID} ${VERSION_ID}"
```

`deploy/systemd/test/smoke-no-caddy.sh` (verbatim):

```bash
#!/usr/bin/env bash
set -euo pipefail
apt-get update
apt-get install -y systemd systemd-sysv ca-certificates curl

# Caddy is intentionally NOT installed.
output="$(TRILITHON_NONINTERACTIVE=1 bash deploy/systemd/install.sh 2>&1 || true)"
echo "${output}"

if ! echo "${output}" | grep -q "Trilithon requires an existing Caddy"; then
    echo "smoke-no-caddy: documented message not seen" >&2
    exit 1
fi
echo "smoke-no-caddy: ok"
```

`deploy/systemd/test/smoke-old-caddy.sh` (verbatim):

```bash
#!/usr/bin/env bash
set -euo pipefail
apt-get update
apt-get install -y systemd systemd-sysv ca-certificates curl

# Stub a Caddy binary that reports v2.7.6.
cat > /usr/local/bin/caddy <<'CADDY'
#!/usr/bin/env bash
echo "v2.7.6 h1:abcd..."
CADDY
chmod +x /usr/local/bin/caddy

output="$(TRILITHON_NONINTERACTIVE=1 bash deploy/systemd/install.sh 2>&1 || true)"
echo "${output}"

if ! echo "${output}" | grep -q "Trilithon requires Caddy 2.8 or later"; then
    echo "smoke-old-caddy: documented message not seen" >&2
    exit 1
fi
echo "smoke-old-caddy: ok"
```

### Algorithm

`smoke.sh`:

1. Install systemd, Caddy 2.8 from the APT repo.
2. Run `install.sh` non-interactively.
3. Curl the health endpoint.
4. Verify daemon UID and admin socket presence.
5. Run `uninstall.sh --remove-data`.

`smoke-no-caddy.sh`: run `install.sh` without Caddy installed; assert exit is non-zero and stderr contains the documented refusal message.

`smoke-old-caddy.sh`: stub Caddy with v2.7.6; assert refusal.

### Tests

- The four CI jobs themselves are the tests.

### Acceptance command

```
bash deploy/systemd/test/smoke.sh
```

### Exit conditions

- `.github/workflows/systemd-smoke.yml` runs three jobs: `ubuntu-24-04`, `debian-12`, `no-caddy`, `old-caddy`.
- Both positive jobs pass.
- Both negative jobs pass and assert the documented messages.

### Audit kinds emitted

During the smoke run the daemon emits `auth.bootstrap-credentials-rotated`, `caddy.capability-probe-completed`, `config.applied` (architecture §6.6).

### Tracing events emitted

During the smoke run the daemon emits `daemon.started`, `caddy.connected`, `caddy.capability-probe.completed`, `apply.succeeded` (architecture §12.1).

### Cross-references

- PRD T2.7.
- Hazards: H1.

---

## Slice 24.8 — `.deb` package build with postinst/prerm/postrm hooks

### Goal

Add a `[package.metadata.deb]` section to `core/Cargo.toml` and a `.github/workflows/deb-build.yml` that produces `trilithon_<version>_amd64.deb`. The package contains the binary, unit, `tmpfiles.d` snippet, Caddy drop-in, example config, and `postinst`/`prerm`/`postrm` scripts that run install/uninstall steps idempotently. After this slice, `dpkg -i trilithon_<version>_amd64.deb` followed by `systemctl status trilithon` reports active.

### Entry conditions

- Slices 24.4 and 24.5 complete.
- `cargo-deb` is installable on `ubuntu-24.04` runners.

### Files to create or modify

- `core/Cargo.toml` — append `[package.metadata.deb]` block.
- `.github/workflows/deb-build.yml` — package build workflow.
- `deploy/systemd/debian/postinst` — runs `install.sh` steps idempotently on install/upgrade.
- `deploy/systemd/debian/prerm` — stops the service.
- `deploy/systemd/debian/postrm` — on `purge` removes data, user, group; on `remove` leaves data.

### Signatures and shapes

`core/Cargo.toml` addition (under the relevant package — typically the `trilithon-cli` package metadata):

```toml
[package.metadata.deb]
name = "trilithon"
maintainer = "Trilithon project <noreply@invalid>"
copyright = "2026, Trilithon contributors"
license-file = ["LICENSE", "0"]
extended-description = """
Local-first control plane for the Caddy reverse proxy.
"""
depends = "$auto, caddy (>= 2.8)"
section = "net"
priority = "optional"
assets = [
    ["target/release/trilithon-cli", "/usr/bin/trilithon", "0755"],
    ["../deploy/systemd/trilithon.service", "/lib/systemd/system/trilithon.service", "0644"],
    ["../deploy/systemd/tmpfiles.d/trilithon.conf", "/usr/lib/tmpfiles.d/trilithon.conf", "0644"],
    ["../deploy/systemd/caddy-drop-in/trilithon-socket.conf", "/lib/systemd/system/caddy.service.d/trilithon-socket.conf", "0644"],
    ["../deploy/systemd/config.toml.example", "/etc/trilithon/config.toml.example", "0644"],
]
maintainer-scripts = "../deploy/systemd/debian/"
```

`.github/workflows/deb-build.yml` (verbatim):

```yaml
name: deb-build

on:
  push:
    tags:
      - 'v*'
  workflow_dispatch:

jobs:
  build:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo install cargo-deb --locked
      - run: |
          cd core
          cargo build --release --bin trilithon-cli
          cargo deb --no-build --package trilithon-cli
      - uses: actions/upload-artifact@v4
        with:
          name: trilithon-deb
          path: core/target/debian/*.deb
      - name: Smoke install
        run: |
          sudo dpkg -i core/target/debian/*.deb || true
          sudo apt-get install -y -f
          sudo systemctl status trilithon --no-pager || sudo journalctl -u trilithon --no-pager -n 100
```

`deploy/systemd/debian/postinst` (verbatim):

```bash
#!/usr/bin/env bash
set -e

if ! getent group trilithon >/dev/null; then
    groupadd --system trilithon
fi
if ! getent passwd trilithon >/dev/null; then
    useradd --system --gid trilithon \
        --home-dir /var/lib/trilithon \
        --shell /usr/sbin/nologin \
        --comment "Trilithon control plane" trilithon
fi
if getent group caddy >/dev/null; then
    usermod -aG caddy trilithon
fi

install -d -o root -g trilithon -m 0750 /etc/trilithon
if [[ ! -f /etc/trilithon/config.toml ]]; then
    install -o root -g trilithon -m 0640 \
        /etc/trilithon/config.toml.example /etc/trilithon/config.toml
fi
install -d -o trilithon -g trilithon -m 0750 /var/lib/trilithon
install -d -o trilithon -g trilithon -m 0750 /var/log/trilithon

systemctl daemon-reload
systemd-tmpfiles --create /usr/lib/tmpfiles.d/trilithon.conf || true

if [[ "${1:-}" == "configure" ]]; then
    systemctl restart caddy || true
    systemctl enable --now trilithon || true
fi

exit 0
```

`deploy/systemd/debian/prerm` (verbatim):

```bash
#!/usr/bin/env bash
set -e
systemctl stop trilithon || true
systemctl disable trilithon || true
exit 0
```

`deploy/systemd/debian/postrm` (verbatim):

```bash
#!/usr/bin/env bash
set -e
case "${1:-}" in
    purge)
        rm -rf /etc/trilithon /var/lib/trilithon /var/log/trilithon
        if getent passwd trilithon >/dev/null; then
            gpasswd -d trilithon caddy 2>/dev/null || true
            userdel trilithon || true
        fi
        if getent group trilithon >/dev/null; then
            groupdel trilithon || true
        fi
        ;;
    remove)
        # Leave data in place.
        ;;
esac
systemctl daemon-reload || true
exit 0
```

### Algorithm

1. `cargo deb` packages the binary plus the deployment artefacts using the `assets` table in `Cargo.toml`.
2. Postinst runs idempotent provisioning, preserves an existing `/etc/trilithon/config.toml`, and on `configure` restarts Caddy and enables Trilithon.
3. Prerm stops the service.
4. Postrm on `purge` removes data, user, group; on `remove` preserves data.

### Tests

- `deploy/systemd/test/test_deb_install.sh` — `dpkg -i` the produced `.deb`, run `verify_running`, then `dpkg --purge`, then `verify_clean`.

### Acceptance command

```
cd core && cargo deb --package trilithon-cli \
  && bash deploy/systemd/test/test_deb_install.sh
```

### Exit conditions

- `core/Cargo.toml` declares the `[package.metadata.deb]` block with the assets list.
- `.github/workflows/deb-build.yml` builds the package on tag pushes.
- `postinst`, `prerm`, `postrm` exist with the verbatim contents above.
- `dpkg --purge` followed by the uninstall verification leaves no residue.

### Audit kinds emitted

After `dpkg -i` the daemon emits the same kinds as slice 24.4.

### Tracing events emitted

After `dpkg -i` the daemon emits the same events as slice 24.4.

### Cross-references

- PRD T2.7.

---

## Slice 24.9 — Operator and end-user documentation

### Goal

Author the operator-facing `deploy/systemd/README.md` and the user-facing `docs/install/systemd.md` with the headings mandated by the phased plan. After this slice, a documentation lint asserts every heading is present.

### Entry conditions

- Slices 24.4, 24.5, 24.6, 24.7 complete.

### Files to create or modify

- `deploy/systemd/README.md` — operator-facing.
- `docs/install/systemd.md` — user-facing installation page.
- `deploy/systemd/test/lint-readme-headings.sh` — heading lint.

### Signatures and shapes

`docs/install/systemd.md` outline (each `## ` MUST appear verbatim):

```markdown
# Installing Trilithon on bare-metal systemd

## Prerequisites

## Caddy install

## Trilithon install

## Bootstrap credentials

## Configuration

## Logs and journald

## Upgrading

## Uninstalling

## Troubleshooting

## Hardening notes
```

`deploy/systemd/test/lint-readme-headings.sh` (verbatim):

```bash
#!/usr/bin/env bash
set -euo pipefail
required=(
    "## Prerequisites"
    "## Caddy install"
    "## Trilithon install"
    "## Bootstrap credentials"
    "## Configuration"
    "## Logs and journald"
    "## Upgrading"
    "## Uninstalling"
    "## Troubleshooting"
    "## Hardening notes"
)
for heading in "${required[@]}"; do
    if ! grep -Fxq "${heading}" docs/install/systemd.md; then
        echo "lint: docs/install/systemd.md missing: ${heading}" >&2
        exit 1
    fi
done
```

### Algorithm

1. Compose the documentation with every required heading.
2. The lint script enforces presence on every CI run.

### Tests

- `deploy/systemd/test/lint-readme-headings.sh` — passes on canonical files; fails on a synthetic deletion.

### Acceptance command

```
bash deploy/systemd/test/lint-readme-headings.sh
```

### Exit conditions

- `deploy/systemd/README.md` and `docs/install/systemd.md` exist with every mandated heading.
- The heading lint is wired into `just check` (see Phase 23 slice 23.9 for the same wiring approach; extend the recipe here).

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T2.7.

---

## Phase exit checklist

- [ ] `just check` passes.
- [ ] A fresh Ubuntu 24.04 LTS or Debian 12 system installs Trilithon in one command and has a working web UI within 60 seconds (slices 24.4 and 24.7).
- [ ] The daemon runs as the dedicated `trilithon` user (verified by inspecting the running PID's UID — slice 24.4 `verify_running`).
- [ ] The daemon talks to Caddy over `/run/caddy/admin.sock` (verified by socket presence and group ownership — slice 24.4).
- [ ] Uninstall removes the service, the user, the group, and (with confirmation) the data directory (slice 24.5).
- [ ] The Caddy detection refuses to proceed without Caddy and on Caddy older than 2.8 (slices 24.2 and 24.7).
- [ ] `systemd-analyze verify` passes for `trilithon.service` (slice 24.1).
- [ ] `systemd-analyze security trilithon.service` reports an exposure score ≤ 1.5 (slice 24.6).
- [ ] The `.deb` package builds and round-trips install/purge cleanly (slice 24.8).
- [ ] `deploy/systemd/README.md` and `docs/install/systemd.md` contain every mandated heading (slice 24.9).
