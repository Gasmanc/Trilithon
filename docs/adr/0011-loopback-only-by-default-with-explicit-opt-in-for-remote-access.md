# ADR-0011: Bind to loopback by default; require explicit opt-in for remote access

## Status

Accepted — 2026-04-30.

## Context

Trilithon's daemon serves two HTTP surfaces: the human web UI (T1.13)
and the typed tool gateway (T1.6, ADR-0008). It also speaks to Caddy's
admin endpoint (constraint 11, hazard H1). Each binding decision is a
security decision.

The binding prompt fixes the defaults explicitly:

- T1.13 acceptance: "Loopback-only is the default. Binding to
  `0.0.0.0` requires an explicit configuration flag and prints an
  authentication-required warning at startup."
- Constraint 11: "Caddy admin endpoint is never exposed. Trilithon's
  daemon talks to Caddy over a Unix domain socket or `localhost` with
  mutual TLS."
- Constraint 14: "The local user is sovereign. Trilithon runs on the
  user's hardware, owns its own data, and never phones home."
- Hazard H1: "Specifications that mention the admin endpoint MUST
  state the binding and the authentication posture."

Forces:

1. **Discoverability is harm.** A daemon listening on `0.0.0.0` is
   discoverable by network scanners on the same broadcast domain.
   Trilithon's web UI before authentication is configured is a
   route-creating tool; an attacker reaching it pre-bootstrap can
   create routes through Caddy.
2. **Authentication is necessary but not sufficient.** Even with
   T1.14's Argon2id-hashed local accounts, exposing the surface to
   the public internet adds a brute-force attack surface that
   loopback binding eliminates.
3. **Local-first is the deployment shape.** Constraint 14 says
   Trilithon runs on the user's hardware. Headless deployments
   (T2.7, T2.8) reach the UI through SSH port-forwarding, a VPN,
   or a reverse-proxy (Caddy itself, ironically). This shape is
   compatible with loopback default.
4. **Explicit opt-in is the right knob.** Users who want remote
   access exist and should be served, but the act of opening up
   should be deliberate, configured, and accompanied by a
   prominent warning that authentication is the only thing standing
   between an attacker and the user's reverse proxy configuration.

## Decision

**Web UI binding.** The Trilithon daemon SHALL bind the web UI to
`127.0.0.1` by default. The default port SHALL be 7878 (T1.13). The
binding interface SHALL be configurable through a single, explicit
configuration field: `web_ui.bind_address`. The configuration field
SHALL accept any IP literal or `0.0.0.0`. There SHALL NOT be a
shorthand flag (`--public`, `--remote`) that hides the binding choice.

**Tool gateway binding.** The typed tool gateway SHALL bind to the
same interface as the web UI by default. It MAY be configured to
bind separately through `tool_gateway.bind_address`. If
`tool_gateway.bind_address` is not loopback, the daemon SHALL refuse
to start unless `tool_gateway.require_token = true` (default true)
and a non-empty token list is configured.

**Caddy admin binding.** Trilithon SHALL talk to Caddy's admin
endpoint exclusively over a Unix domain socket on a shared volume
between the Trilithon daemon and Caddy (ADR-0010), or over
`localhost` (loopback only). The canonical Unix-socket path is
`/run/caddy/admin.sock` on Linux deployments. On macOS or Windows
development setups the fallback is loopback TCP `127.0.0.1:2019`
with mutual TLS. Both transports are acceptable; the binary choice
lives in `config.toml` under `[caddy] admin_endpoint = "..."`.
Trilithon SHALL refuse to connect to a Caddy admin endpoint
reachable on a non-loopback interface and SHALL surface the situation
as a startup error, instructing the user how to relocate the admin
endpoint to a loopback or Unix-socket binding (constraint 11,
hazard H1). Trilithon SHALL NOT include any code path or
configuration option that binds Caddy's admin endpoint to a
non-loopback interface on the user's behalf.

**Startup warnings.** When `web_ui.bind_address` is non-loopback,
the daemon SHALL emit a stark, multi-line warning at startup
through the tracing subscriber (and to stderr) explaining:

- The web UI is now reachable from the network on the configured
  interface and port.
- Authentication (T1.14) is the only safeguard.
- The user SHALL ensure that Trilithon is reachable only over a
  trusted transport (a VPN, a properly configured reverse proxy,
  SSH tunneling).

The warning SHALL print every startup, not only on first non-loopback
boot. Operators who configure non-loopback intentionally accept the
log noise.

**Bootstrap authentication file.** When the daemon first starts, the
bootstrap account credentials (T1.14) SHALL be written to a
permission-restricted file (`0600`) under the daemon's data
directory. The credentials SHALL NOT appear in process arguments,
environment variables, or logs (hazard H13). The user SHALL be
prompted to change them on first login.

**Transport for remote access.** When non-loopback binding is
configured, the daemon SHALL serve over TLS. The TLS certificate
SHALL be configurable; in the absence of a configured certificate,
the daemon SHALL refuse to bind to a non-loopback interface. There
SHALL NOT be an "I know what I'm doing" plaintext-on-public-interface
mode.

**Federation, telemetry, and outbound calls.** Trilithon SHALL NOT
make outbound calls to any third-party service. Telemetry, if added
in the future, SHALL be opt-in and off by default (constraint 14).
Update checks SHALL be opt-in and off by default. There SHALL NOT
be a "phone home" code path.

## Consequences

**Positive.**

- The default deployment is safe: a user installing Trilithon and
  doing nothing further has a daemon reachable only from
  `localhost`. The bootstrap account file at `0600` is the only
  pre-shared secret.
- Remote access is reachable but deliberate. Users who configure
  non-loopback see the warning each startup, which is the correct
  signal-to-noise for a security-relevant configuration.
- Caddy's admin endpoint is never on a non-loopback interface,
  honouring constraint 11 and hazard H1 by construction.
- Constraint 14's "never phones home" is implemented as the
  absence of outbound code paths, not as an opt-out toggle.

**Negative.**

- Headless users reaching the UI from a different machine must
  use SSH port-forwarding, a VPN, or place Trilithon behind
  Caddy itself. This is a documentation burden. The deployment
  guides SHALL spell out the supported approaches.
- Some users will resent the startup warning when they have made
  an informed remote-access decision. The trade is acceptable;
  the warning is one of the cheapest, most legible safety nets
  available.
- TLS termination on the daemon's surface complicates the
  deployment story for users who would prefer plaintext on a
  trusted LAN. The product accepts the friction; encrypted
  transport for credentials is non-negotiable on non-loopback.

**Neutral.**

- mutual TLS for the Caddy admin connection (constraint 11
  alternative) is an option for users who run Caddy on a different
  host. V1's primary path is the Unix-socket model (ADR-0010);
  mutual-TLS-over-localhost is documented but not the default.
- The configuration schema reserves a `tool_gateway.allowed_origins`
  field for V1.1 Tauri compatibility (ADR-0005); V1 hard-codes the
  expected origin to the loopback web UI.

## Alternatives considered

**Default to `0.0.0.0` and rely on authentication.** Bind to all
interfaces by default and trust T1.14's local accounts. Rejected
because pre-bootstrap (before the user has changed the bootstrap
password) the surface is reachable from the network with credentials
in a file the attacker may also be able to read on a misconfigured
host, and because the cost of loopback default is one configuration
field for the user who actually wants remote access.

**Bind to all interfaces but require a "first-run unlock" via
console.** Print a one-time token to the daemon's stdout and require
its entry through the network UI before any other action. Rejected
because the model is more brittle than loopback-by-default and
because users in headless deployments (T2.7, T2.8) cannot
necessarily see stdout.

**Default to a Unix domain socket for the web UI.** Bind the web UI
to a Unix socket and require an SSH-tunnel-equivalent for remote
access. Rejected because browsers do not natively connect to Unix
sockets without a forwarding helper, which adds friction even for
the local-only case.

**Mandatory mutual TLS on every interface, including loopback.**
Generate self-signed client certificates on first run and require
them for the web UI. Rejected as gold-plating for V1; the
mutual-TLS posture is appropriate for the Caddy admin link
(constraint 11) but loopback-bound web UI traffic is already on a
trusted channel.

## References

- Binding prompt: `../prompts/PROMPT-spec-generation.md#2-non-negotiable-constraints`,
  items 11, 14; section 4 features T1.13, T1.14; section 7 hazards
  H1, H13.
- ADR-0008 (Bounded typed tool gateway for language models).
- ADR-0010 (Two-container deployment with unmodified official Caddy).
- ADR-0014 (Secrets encrypted at rest with keychain master key).
