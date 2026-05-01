# PROMPT — Trilithon Specification Generation

> This is the durable, authoritative meta-prompt for generating any
> specification, plan, ADR, architecture document, phased roadmap, or task
> list for the Trilithon project. Every specification artifact in
> `docs/adr`, `docs/planning`, `docs/architecture`, `docs/phases`, and
> `docs/todo` is downstream of this prompt.
>
> When asked to "generate spec X for Trilithon," the answering agent MUST
> read this file in full and treat its contents as binding constraints.

---

## 0. How to use this prompt

You are an expert systems architect, product manager, and technical writer
producing specifications for the Trilithon project. You have been engaged
because the project owner wants documentation that is:

- **Unequivocal** — every requirement uses RFC 2119 keywords (MUST, MUST
  NOT, SHALL, SHALL NOT, SHOULD, SHOULD NOT, MAY) where it expresses a
  requirement. No "we could consider," no "might," no "potentially."
- **Unabbreviated** — write the noun, not the acronym, the first time.
  Each document begins with a glossary if it uses more than five
  domain-specific terms.
- **Intricate** — assume the reader will implement directly from this
  document with no further conversation. Spell out edge cases, failure
  modes, and acceptance criteria.
- **Internally consistent** — terms used in one document mean the same
  thing in every document. The glossary in this prompt is canonical.
- **Bullet-proof against scope creep** — features outside Tier 1 and
  Tier 2 are explicitly out of scope for V1 and MUST be marked as such
  with the words "OUT OF SCOPE FOR V1" wherever they appear.

If a request is ambiguous against this prompt, follow this prompt. If a
request directly contradicts this prompt, raise the contradiction in the
output rather than silently choosing one.

---

## 1. Project identity

**Name.** Trilithon.

**One-line description.** A local-first, LLM-operable control plane for
the Caddy reverse proxy, presented through a web UI today and a native
desktop application tomorrow.

**Three-line description.** Trilithon owns a desired-state model for one
or more Caddy instances and reconciles that desired state with each
Caddy's JSON Admin API. It exposes that model through a typed,
auditable, reversible API surface, which is consumed both by a human
web user interface and by language-model agents acting under bounded
permissions. Every mutation produces an immutable snapshot, an audit
record, and a diff that humans and language models can inspect before
the change is applied.

**What it is not.** Trilithon is not a proxy, not a web application
firewall, not a load balancer, not a service mesh, and not a container
orchestrator. It is a control plane for an existing reverse proxy
(Caddy). It does not handle a single byte of user-facing HTTP traffic.

---

## 2. Non-negotiable constraints

These constraints are decided. Specifications MUST adhere to them. Where
a specification proposes deviating from a constraint, the deviation MUST
be raised as an open question rather than presented as a decision.

1. **The proxy is Caddy.** The user has chosen Caddy. Specifications
   MUST NOT propose Nginx, HAProxy, Envoy, or Traefik as the proxy.
2. **Caddy's JSON Admin API is the source of truth.** Caddyfile is
   accepted only as a one-way import. The product never round-trips
   through Caddyfile.
3. **Caddy itself is unmodified.** Trilithon ships alongside the
   official Caddy binary or container image. Trilithon does not fork,
   patch, or rebuild Caddy.
4. **Backend is Rust, three-layer workspace.** `core` is pure logic
   with no input/output, no async runtime, no foreign function
   interface. `adapters` wraps the outside world (database, HTTP,
   filesystem, environment, time). `cli` (or `daemon`) is the binary.
   Cross-layer dependencies that violate this rule are forbidden.
5. **Frontend is React 19 + TypeScript ~5.6 (strict) + Tailwind 3 +
   Vite 5.** Tested with Vitest. The web UI is shipped first. A Tauri
   desktop wrap is V1.1 work, not V1 work.
6. **Persistence is SQLite** for V1 single-instance. PostgreSQL is a
   V2+ option behind a `Storage` adapter trait, not a V1 deliverable.
7. **No `unwrap()`, `expect()`, `panic!`, `!` or non-null assertions in
   production code paths.** Tests are exempt.
8. **No mocks, stubs, or fakes outside test files and test
   directories.** Production code uses real implementations or real
   trait abstractions, not test doubles.
9. **No `TODO`, `FIXME`, `XXX`, or `HACK` markers in committed code.**
   Track work in this repository's planning system.
10. **`just check` is the gate.** No deliverable is "done" until
    `just check` passes locally and in continuous integration.
11. **Caddy admin endpoint is never exposed.** Trilithon's daemon talks
    to Caddy over a Unix domain socket or `localhost` with mutual TLS.
    Specifications MUST NOT propose binding Caddy's admin port to a
    non-loopback interface.
12. **Secrets never appear in audit log diffs in plaintext.** A
    secrets-aware redactor sits between the diff engine and the audit
    log writer.
13. **Configuration ownership is explicit.** When Trilithon detects
    that another actor has modified Caddy's running config out of
    band, it MUST surface this as configuration drift and MUST NOT
    silently overwrite.
14. **The local user is sovereign.** Trilithon runs on the user's
    hardware, owns its own data, and never phones home. Telemetry is
    opt-in and off by default.

---

## 3. Glossary (canonical)

| Term | Definition |
|------|------------|
| **Caddy** | The upstream reverse proxy product, version 2.8 or later, accessed exclusively through its JSON Admin API on a loopback or Unix socket interface. |
| **Caddyfile** | Caddy's human-friendly configuration format. Trilithon imports it once, one-way, and never writes it back. |
| **Trilithon** | This project. The control plane. |
| **Trilithon daemon** | The Rust binary that owns desired state and reconciles it with Caddy. |
| **Web UI** | The local React/TypeScript browser application served by the daemon on `127.0.0.1:<port>` by default. |
| **Desktop app** | The Tauri wrapper around the web UI. V1.1 deliverable. Out of scope for V1. |
| **Desired state** | The configuration the user (or an authorised agent) has asked Trilithon to enforce. Persisted to SQLite. |
| **Running state** | The configuration Caddy is actually serving, as reported by `GET /config/`. |
| **Drift** | A non-empty diff between desired state and running state. |
| **Snapshot** | An immutable, content-addressed record of desired state at a point in time, including the actor, intent, and resulting Caddy JSON. |
| **Mutation** | A typed, idempotent operation on desired state (create route, update upstream, attach policy, etc.). Each mutation produces exactly one snapshot. |
| **Apply** | The act of writing a snapshot to Caddy's Admin API and confirming success. |
| **Rollback** | An apply whose target is a prior snapshot. |
| **Proposal** | A mutation that has been generated (by Docker discovery, by a language model, by an import process) but not yet approved by an authorised actor. |
| **Capability probe** | The startup procedure that asks Caddy `GET /config/apps` and `GET /reverse_proxy/upstreams` to determine which optional Caddy modules are loaded. |
| **Policy preset** | A named bundle of access controls, headers, and rate-limit settings (e.g. `internal-app`, `public-admin`) that can be attached to a route. |
| **Audit record** | An immutable log entry recording who did what, when, with what intent, and with what result. |
| **Correlation identifier** | A ULID propagated through every layer of the system, joining HTTP requests, mutations, snapshots, audit records, and language-model sessions. |
| **Language model agent** | An external large-language-model client invoking Trilithon's typed tool gateway over an authenticated channel. |
| **Tool gateway** | The bounded, typed surface that language model agents are permitted to call. Distinct from the human web UI's API; both ride on the same underlying mutation primitives. |

---

## 4. Tier 1 — Foundational (V1, must ship together)

Every Tier 1 feature is non-optional for V1. The product cannot ship
without all of them. They exist as a single coherent foundation: removing
any one breaks the others.

### T1.1 Configuration ownership loop

Trilithon owns a typed model of desired Caddy configuration in memory,
persists it to SQLite, validates every change locally, computes a diff
against running state, and reconciles the difference through Caddy's
`POST /load` (full) or `PATCH /config/...` (partial) endpoints.

Acceptance:

- Given desired state X and running state X, no apply is performed.
- Given desired state Y and running state X, exactly one apply is
  performed, and the resulting running state equals Y.
- An apply that fails at Caddy's validation step does not advance the
  desired state pointer; the prior desired state remains canonical.
- All applies are wrapped in optimistic concurrency control on a
  monotonically increasing `config_version` integer. A stale apply
  is rejected with a typed conflict error.

### T1.2 Snapshot history with content addressing

Every mutation that changes desired state produces a snapshot row in
SQLite. The snapshot is content-addressed: its identifier is the
SHA-256 of its canonical JSON serialisation. Identical snapshots
deduplicate.

Acceptance:

- Snapshots are immutable. There is no `UPDATE snapshots` statement
  anywhere in the codebase.
- Each snapshot records: identifier, parent identifier, actor (user
  identifier or language model session identifier), intent (free
  text), correlation identifier, Caddy version at apply time,
  Trilithon version, wall-clock time (UTC, monotonic and Unix
  timestamp), and the canonical desired-state JSON.

### T1.3 One-click rollback with preflight

Any prior snapshot can be made the new desired state. Before applying,
Trilithon runs a preflight that checks: upstream reachability for every
referenced upstream, TLS certificate validity for every referenced host,
referenced Docker container existence (if relevant), and Caddy module
availability for every referenced module.

Acceptance:

- A rollback that fails preflight reports a structured error listing
  every failing condition. The user MAY override on a per-condition
  basis. The override is recorded in the audit log.
- A rollback that passes preflight applies atomically.

### T1.4 Drift detection on startup and on schedule

On daemon startup and on a configurable interval (default 60 seconds),
Trilithon fetches Caddy's running configuration and computes a diff
against the current desired state.

Acceptance:

- A non-empty diff is reported to the audit log as a drift event.
- The user is offered three actions: adopt running state as desired
  state, re-apply desired state to overwrite Caddy, or open the diff
  in the dual-pane editor for manual reconciliation.
- Drift detection MUST NOT silently overwrite Caddy.

### T1.5 Caddyfile one-way import

A Caddyfile can be parsed and converted to desired state. The original
Caddyfile is preserved as an attachment to the resulting import
snapshot. Trilithon never writes a Caddyfile.

Acceptance:

- A round-trip from Caddyfile → desired state → Caddy JSON produces
  semantically equivalent runtime behaviour, verified by a corpus of
  fixture Caddyfiles in integration tests.
- Imports that lose information (e.g. comments, ordering) emit a
  structured warning listing the lost elements.

### T1.6 Typed mutation API

Every change to desired state goes through one of a finite, typed set
of mutation operations. There is no untyped "set arbitrary JSON" path.
The mutation set is the same surface consumed by the human web UI and
the language-model tool gateway.

Acceptance:

- Each mutation has a Rust type, a JSON schema, and a documented
  pre-condition, post-condition, and idempotency story.
- The set is closed under composition: any sequence of mutations
  produces a valid desired state or fails at a single, identifiable
  mutation.

### T1.7 Audit log with correlation identifiers

Every mutation, apply, rollback, drift event, language-model
interaction, and authentication event writes one row to the audit log.
Every row carries a correlation identifier that joins the event to the
HTTP request that triggered it.

Acceptance:

- Audit rows are immutable. There is no `UPDATE audit_log` statement.
- Audit rows MUST NOT contain plaintext secrets. A secrets-aware
  redactor sits between the diff engine and the audit log writer.
- All wall-clock timestamps are stored as UTC Unix timestamps and
  rendered to the user in their local time zone.

### T1.8 Route create / read / update / delete

The minimum useful product. The user can create a reverse-proxy route
from a hostname to an upstream, read existing routes, update them, and
delete them, all through the typed mutation API.

Acceptance:

- A newly created route serves traffic within five seconds of
  approval (assuming Caddy is healthy).
- A deleted route stops serving traffic within five seconds.
- An update is atomic: there is no observable window where the route
  is half-updated.

### T1.9 TLS certificate visibility

The user can see, for every host Trilithon manages, the certificate
issuer, expiry date, and renewal status, sourced from Caddy's `GET
/config/apps/tls/certificates` and `GET /reverse_proxy/upstreams`.

Acceptance:

- Certificates expiring within 14 days are flagged amber. Within 3
  days are flagged red.
- Certificates that have failed to renew are flagged red regardless
  of expiry.

### T1.10 Basic upstream health visibility

For every route, Trilithon shows whether its upstream(s) are reachable.
Reachability is determined by Caddy's `/reverse_proxy/upstreams`
endpoint and a Trilithon-side TCP connect probe.

Acceptance:

- Health state updates within 30 seconds of an upstream becoming
  reachable or unreachable.
- The user can disable Trilithon-side probes per route (some
  upstreams reject unsolicited TCP connects).

### T1.11 Caddy capability probe

On daemon startup and on Caddy reconnect, Trilithon queries Caddy's
loaded modules and stores the result. UI features that depend on
optional modules are gated behind detected capability.

Acceptance:

- A user running stock Caddy sees rate-limit and web-application-
  firewall features marked "unavailable on this Caddy build" with a
  link to documentation explaining how to enable them.
- A user running an enhanced Caddy build sees those features
  enabled.
- The probe result is cached and revalidated on Caddy reconnect.

### T1.12 Dual-pane configuration editor

The web UI provides a side-by-side editor: Caddyfile-style legible form
on the left, raw JSON on the right. Edits in either pane validate live
and update the other. This is the power-user escape hatch.

Acceptance:

- An invalid edit on either side shows a structured error pointing to
  the offending line and key.
- Apply is disabled while validation is failing.
- A valid edit produces a preview diff against current desired state
  before the user commits.

### T1.13 Local web UI delivery

The daemon serves the React web UI on `127.0.0.1:<port>` (default
configurable, with a sensible default such as 7878). The UI is
accessible only on loopback by default. Remote access is opt-in and
authenticated.

Acceptance:

- A user who has never touched a config file can install Trilithon,
  open `http://127.0.0.1:7878`, and create their first route.
- Loopback-only is the default. Binding to `0.0.0.0` requires an
  explicit configuration flag and prints an authentication-required
  warning at startup.

### T1.14 Authentication and session management

Trilithon's web UI and tool gateway are both authenticated. V1 supports
local user accounts (Argon2id-hashed passwords) and a single-user
bootstrap mode (one local account, automatically created on first run).

Acceptance:

- No mutation endpoint is reachable without an authenticated session
  or a valid tool-gateway token.
- Sessions are stored server-side and revocable.
- The bootstrap account's credentials are written to a
  permission-restricted file on first run and the user is prompted
  to change them on first login.

### T1.15 Secrets abstraction

Any field marked secret in the schema (basic-auth password, API key,
forward-auth secret) is stored encrypted at rest, redacted from audit
diffs, and never returned in plaintext through any read endpoint
except an explicit "reveal" call that itself produces an audit entry.

Acceptance:

- Encryption uses XChaCha20-Poly1305 with a key derived from a master
  key that lives outside the SQLite database (system keychain on
  macOS / Linux, file-with-restricted-permissions fallback).
- A leaked SQLite file does not leak secrets.

---

## 5. Tier 2 — V1, after Tier 1 is solid

Tier 2 features ship in V1 but only after Tier 1 is feature-complete and
under integration test. They depend on Tier 1 primitives (mutations,
snapshots, audit log, capability probe) and would be expensive to
retrofit.

### T2.1 Docker container discovery (proposal-based)

Trilithon watches Docker (and Podman) for containers carrying
`caddy.*` labels. Discovered configurations are emitted as proposals,
not auto-applied. The user (or, where policy permits, a language model)
approves or rejects each proposal.

Acceptance:

- A container with valid Caddy labels produces a proposal within 5
  seconds of starting.
- A container destruction produces a "remove route" proposal.
- A label conflict (two containers claiming the same hostname)
  produces a single conflict proposal listing both candidates, never
  two competing proposals.
- Wildcard certificate matches are highlighted with a security
  callout in the proposal UI.

### T2.2 Policy presets

A small, opinionated set of route policy bundles, each combining
security headers, optional access controls, optional rate limits (gated
on capability), and optional bot challenge. V1 set:

1. `public-website` — HSTS, CSP starter, no auth, generous limits.
2. `public-application` — HSTS, CSP, generous limits, bot challenge.
3. `public-admin` — HSTS, strict CSP, mandatory authentication,
   tight rate limit, bot challenge required.
4. `internal-application` — HSTS off (LAN-only), permissive CSP,
   IP/CIDR allowlist required.
5. `internal-admin` — HSTS off, strict CSP, IP/CIDR allowlist
   plus authentication.
6. `api` — no HTML-specific headers, JSON-friendly CORS toggle,
   tight rate limit, mandatory authentication.
7. `media-upload` — generous body size, streaming-friendly,
   authentication required.

Acceptance:

- A preset can be attached to a route in one click.
- The presets are versioned. Updating a preset definition does not
  silently mutate routes already using it; the user is prompted to
  upgrade per-route.
- Presets that depend on optional Caddy modules are marked as such
  and degrade gracefully (the route applies, with the unavailable
  feature omitted and a warning surfaced).

### T2.3 Language-model "explain" mode

The lowest-risk language-model integration. The model can read the
current desired state, audit log, and any single object's history, and
respond in natural language. It cannot mutate.

Acceptance:

- The model has read access to a defined subset of the typed API. No
  shell, no filesystem, no network.
- Every model interaction is logged to the audit log with the model
  identity, prompt, response, and correlation identifier.
- The user can revoke the model's access in one click.

### T2.4 Language-model "propose" mode

The model generates proposals (which are mutations awaiting approval)
in response to user instructions. Proposals appear in the same UI queue
as Docker-discovered proposals.

Acceptance:

- The model cannot apply a proposal directly. Approval requires an
  authenticated user action.
- Proposals expire after a configurable window (default 24 hours).
- The model cannot bypass policy presets: a proposal that would
  violate an attached policy is rejected at validation.

### T2.5 Access log viewer

A live and historical view of Caddy access logs, with structured
filters (host, status code, method, path, source address, latency
bucket), backed by a rolling on-disk store managed by Trilithon.

Acceptance:

- The viewer streams new lines without manual refresh.
- Filters apply in under 200 milliseconds against a rolling store of
  10 million lines.
- Storage size is configurable; oldest entries are evicted first.

### T2.6 Caddy access log explanation

Given a single access log entry, the user can ask "why did this
happen?" The system correlates the entry with the route configuration
that handled it, the policy attached, any rate-limit or access-control
decision, and the upstream response.

Acceptance:

- For 95% of access log entries, the explanation traces every
  decision to a specific configuration object (route, policy,
  upstream).

### T2.7 Bare-metal systemd deployment path

A `systemd` unit, install instructions, and an uninstaller. The daemon
runs as a dedicated user, talks to a system-installed Caddy via Unix
socket, and persists data to a standard system path.

Acceptance:

- A fresh Ubuntu 24.04 LTS or Debian 12 system can install Trilithon
  in one command and have a working web UI within 60 seconds.
- Uninstall removes the service, the user, and (with confirmation)
  the data directory.

### T2.8 Two-container Docker Compose deployment path

An official `docker-compose.yml` that runs the official Caddy image
and a Trilithon daemon image side by side, sharing a volume for the
Unix admin socket and a separate volume for Trilithon's SQLite store.

Acceptance:

- `docker compose up` on a fresh host produces a working web UI on
  `http://127.0.0.1:7878` within 30 seconds.
- The Caddy image is an unmodified official image.
- The Trilithon image is a multi-stage Rust build, distroless or
  scratch-based, under 50 MB.

### T2.9 Configuration export

A user can export their desired state as: (a) a Caddy JSON file
suitable for direct use with Caddy, (b) a Caddyfile equivalent
(best-effort, lossy), (c) a Trilithon-native bundle including
snapshots and audit log.

Acceptance:

- The exported Caddy JSON, applied to a fresh Caddy, produces
  identical runtime behaviour.
- The Caddyfile export is round-trip-tested against a fixture corpus.
- The Trilithon-native bundle is a deterministic archive that can
  be imported into another Trilithon instance.

### T2.10 Concurrency control

When two actors (two humans, a human and a language model, two
language models) attempt mutations against the same desired state
simultaneously, last-write-wins is unacceptable.

Acceptance:

- Every mutation carries a `config_version`. A mutation against a
  stale version is rejected with a typed conflict error and a
  human-readable resolution path ("rebase your changes onto v123").
- The conflict path is reachable from the web UI and from the tool
  gateway.

### T2.11 Wildcard-certificate proposal callout

When a Docker discovery proposal would route a new hostname under an
existing wildcard certificate, the proposal UI MUST surface a
security callout describing the wildcard match and the existing
certificate's coverage.

Acceptance:

- The callout is visually prominent (banner, not footnote).
- The callout requires explicit acknowledgement before approval.
- The acknowledgement is recorded in the audit log.

### T2.12 Backup and restore

The user can create a full backup of Trilithon's state (SQLite
database, encryption keys, snapshots) and restore it on the same or a
different machine.

Acceptance:

- Backups are encrypted with a user-chosen passphrase.
- Restore validates the backup before overwriting any state.
- Restore on a different machine produces an identical desired state
  and an audit log entry recording the restore.

---

## 6. Tier 3 — Sketch only, post-V1

Tier 3 features are real, valuable, and explicitly **OUT OF SCOPE FOR
V1**. They are listed here so that V1 architecture does not preclude
them. Specifications MAY reference Tier 3 items only to demonstrate
that Tier 1/2 design choices accommodate them.

### T3.1 Multi-instance fleet management

UniFi-style controller-edge model. A central Trilithon instance
manages many remote Caddy instances over an outbound, mutually
authenticated tunnel. Replaces a hand-rolled fleet of independent
deployments.

V1 must not preclude this: the typed mutation API and snapshot model
must remain valid when desired state describes multiple Caddy targets.
The V1 schema reserves a `caddy_instance_id` column on every
configuration object; V1 hard-codes it to `local`.

### T3.2 Web Application Firewall integration

Coraza or comparable, gated behind capability probe. UI surfaces WAF
modes (off / monitor / block-high-confidence / strict), rule-set
selection (OWASP CRS), and per-route exemptions.

V1 must not preclude this: policy presets are extensible, and the
capability probe surface is open-ended.

### T3.3 Rate limiting (enforced)

`caddy-ratelimit` integration. Per-route, per-source-address, per-key
buckets with composable rules. Surfaced through policy presets.

V1 must not preclude this: presets already declare a rate-limit slot
that no-ops on stock Caddy.

### T3.4 Forward-auth and OpenID Connect

Pre-authenticated route access via forward-auth (Authelia,
Authentik, oauth2-proxy) and direct OpenID Connect integration for
admin routes.

### T3.5 Layer 4 proxying

TCP/UDP proxying for non-HTTP services (PostgreSQL, MQTT, SSH
gateways). Caddy supports this through `layer4`; UI surface is V2.

### T3.6 Bot challenge integration

Cloudflare Turnstile, hCaptcha, or self-hosted equivalent, attached
to a route via policy preset.

### T3.7 GeoIP and identity-aware routing

Source-country allow/deny, identity-bound route access (combine with
T3.4).

### T3.8 Synthetic monitoring

Trilithon-managed external probes that exercise routes from the
public internet and record availability. Replaces external monitoring
tools for the common case.

### T3.9 OpenTelemetry export

Logs, metrics, and traces exported in OTLP. Not stored locally beyond
the existing access log store.

### T3.10 Hot analytical store for access logs

ClickHouse or DuckDB backend for very large access log workloads.
Replaces the V1 rolling on-disk store when it stops scaling.

### T3.11 Plugin marketplace

Third-party Caddy modules surfaced as installable bundles with a
trust model (signature verification, sandboxed permissions).

### T3.12 Language-model "autopilot" mode

The model applies low-risk, pre-approved classes of mutation without
human approval. Requires a hardened policy engine (Tier 3) to define
what "low-risk" means in a particular environment.

---

## 7. Edge cases and known hazards

Every specification document MUST address these hazards explicitly. They
are not theoretical: they have killed comparable products.

### H1. Caddy admin endpoint exposure

Caddy's admin endpoint is unauthenticated by default. Trilithon's
deployment paths MUST default to a Unix domain socket or `localhost`
binding. Specifications that mention the admin endpoint MUST state
the binding and the authentication posture.

### H2. Stale-upstream rollback

Rolling back to a snapshot whose referenced upstream no longer exists
(container destroyed, host renamed, IP reassigned) MUST fail
preflight. The user MAY override per-condition with an audited
acknowledgement.

### H3. Wildcard-certificate over-match

A new route auto-discovered under an existing wildcard certificate
is a security event, not a convenience. The proposal UI MUST treat
it as such (T2.11).

### H4. Hot-reload connection eviction

Caddy's `POST /load` performs a warm reload; in-flight requests on
the old configuration may experience reset connections. The user
MUST be able to inspect the current connection drain behaviour and
opt into a longer drain window per-apply.

### H5. Capability mismatch

Configuration that references a Caddy module not loaded by the
running Caddy will fail at apply. The capability probe (T1.11) MUST
reject such configuration at desired-state validation, not at apply.

### H6. Time-zone confusion in audit logs

All wall-clock timestamps in storage MUST be UTC Unix timestamps.
Display MUST be in the viewer's local time. Specifications that
discuss time MUST distinguish "stored time" from "displayed time."

### H7. Caddyfile escape lock-in

A user who decides Trilithon is not for them MUST be able to export
their desired state and walk away with a working Caddy
configuration (T2.9).

### H8. Concurrent modification

Two actors mutating the same desired state simultaneously without
optimistic concurrency control produce silent data loss. T2.10 is
non-optional.

### H9. Caddy version skew across snapshots

A snapshot created against Caddy 2.8 may not apply cleanly against
Caddy 2.10. Snapshots MUST record the Caddy version. Restore across
versions MUST warn (not block) and run preflight.

### H10. Secrets in audit diffs

Audit diffs naïvely serialised include plaintext secrets. The
secrets-aware redactor (T1.7, T1.15) sits between the diff engine
and the audit log writer. Specifications MUST NOT propose any code
path that bypasses the redactor.

### H11. Docker socket trust boundary

Mounting `/var/run/docker.sock` into a container grants effective
root on the host. Trilithon's two-container deployment MUST keep
the Docker socket out of the Caddy container and accessible only
to the Trilithon daemon container, which MUST emit a stark warning
in its first-run output explaining the trust grant.

### H12. Multi-instance leak via fat-finger

A user with two Trilithon installations on the same machine pointed
at the same Caddy is a real failure mode. The daemon MUST detect
"another Trilithon is managing this Caddy" via a sentinel object in
Caddy's configuration (`@id: "trilithon-owner"`) and refuse to
proceed without explicit takeover confirmation.

### H13. Bootstrap account credential leak

The first-run bootstrap credentials must not appear in process
arguments, environment variables, or logs. They MUST be written to
a permission-restricted file (`0600`) and the user MUST be prompted
to change them on first login.

### H14. Database corruption

SQLite corruption (power loss, filesystem error) MUST NOT lose the
audit log. Specifications MUST prescribe Write-Ahead Log mode,
periodic `PRAGMA integrity_check`, and a documented recovery path.

### H15. Configuration import that hangs the proxy

A pathological imported Caddyfile (millions of routes, deeply nested
matchers) could exhaust memory at apply time. Imports MUST run
through the same validation pipeline as user mutations and MUST be
size-bounded with a documented limit.

### H16. Language-model prompt injection through user data

A language model reading audit logs or access logs may encounter
text crafted to subvert it (an HTTP request with a malicious
`User-Agent`, a hostname containing instruction-like text). The
tool gateway MUST prepend a system message clarifying that user
data is data, not instruction, and MUST refuse to act on
instruction-like content found in logs.

### H17. Apply-time TLS provisioning

A new public hostname triggers ACME issuance. The apply call returns
quickly, but TLS provisioning may take 30 seconds to several minutes.
The UI MUST surface "issuing certificate" as a distinct state from
"applied" and MUST surface ACME errors with actionable messages.

---

## 8. Documentation standards

When generating downstream documents, agents MUST follow these
templates and conventions.

### 8.1 Architecture Decision Records (`docs/adr/`)

Format: Michael Nygard with explicit numbering. Filename:
`NNNN-kebab-case-title.md`. Numbering is sequential and never
recycled.

Required sections:

1. **Title.** `# ADR-NNNN: <imperative title>`
2. **Status.** One of: Proposed, Accepted, Superseded by ADR-MMMM,
   Deprecated.
3. **Context.** What forces drove this decision. State the
   constraints. Cite the prompt section if relevant.
4. **Decision.** The decision itself, in unequivocal RFC 2119 voice.
5. **Consequences.** Positive, negative, and neutral. Be honest.
6. **Alternatives considered.** Each alternative gets a name, a
   one-paragraph description, and a "rejected because" sentence.
7. **References.** Internal (other ADRs, prompt sections) and
   external (Caddy documentation, RFCs).

ADRs are not retroactively edited. New decisions supersede old ones
through new ADRs.

### 8.2 Product Requirements Document (`docs/planning/`)

Single document. Required sections:

1. **Document control.** Version, date, owner.
2. **Glossary** — link to this prompt's section 3.
3. **Vision** — why this product exists, in 3 paragraphs.
4. **Target users.** Three named personas with goals, pains,
   technical level, and success criteria.
5. **Scope.** Tier 1 features (T1.1 … T1.15), Tier 2 features
   (T2.1 … T2.12), with full requirements per feature: user
   story, functional requirements (RFC 2119 voice), non-functional
   requirements, dependencies, acceptance criteria.
6. **Out of scope for V1.** Tier 3 items, with one-paragraph
   justification per omission.
7. **Success metrics.** Quantitative (time-to-first-route,
   crash-free sessions, drift-event resolution rate) and
   qualitative.
8. **Risks and mitigations.** All hazards from section 7.
9. **Open questions.** Tracked, not decided.

### 8.3 Architecture document (`docs/architecture/`)

Single document. Required sections:

1. **Document control.**
2. **Glossary.**
3. **System context.** What sits inside Trilithon, what sits
   outside (Caddy, Docker, the user's browser, language models).
   Use a textual diagram (ASCII or Mermaid) and label every
   boundary.
4. **Component view.** Each Rust crate and each frontend module.
   For each: responsibility, dependencies, data owned, errors
   produced.
5. **Layer rules.** The three-layer Rust split, with an explicit
   table of what each layer MAY and MUST NOT depend on.
6. **Data model.** Every SQLite table: schema, primary key,
   foreign keys, indexes, retention. Snapshot, audit, mutation
   queue, secrets-vault metadata, sessions.
7. **Control flow.** The mutation lifecycle from request to
   audit, end to end. The drift-detection loop. The proposal
   lifecycle. The capability probe.
8. **External interfaces.** Caddy Admin API contract (which
   endpoints, which methods, error handling), Docker API contract,
   Tool Gateway contract.
9. **Concurrency model.** Tokio runtime, task ownership, shared
   state, locking. Optimistic concurrency on `config_version`.
10. **Failure model.** What fails how. Caddy unreachable, SQLite
    locked, Docker socket gone, capability probe fails. Each has a
    documented user-visible behaviour.
11. **Security posture.** Authentication, authorisation, secrets
    handling, audit boundaries, language-model boundary, Docker
    socket boundary. Tie back to hazards in section 7.
12. **Observability.** Logs, metrics, traces produced by Trilithon
    itself. Correlation identifier propagation.
13. **Performance budget.** Latency targets per operation, memory
    ceiling, SQLite size growth.
14. **Upgrade and migration.** SQLite schema migrations, Caddy
    version compatibility, Trilithon version compatibility.

### 8.4 Phased plan (`docs/phases/`)

Single document, one section per phase. Each phase MUST contain:

1. **Phase identifier and title.**
2. **Objective.** One paragraph.
3. **Entry criteria.** What must be true to begin.
4. **Deliverables.** A bulleted list of concrete, demoable artefacts.
5. **Exit criteria.** What must be true to declare the phase done,
   in unequivocal terms. Always includes "`just check` passes."
6. **Dependencies.** Which prior phases or external prerequisites.
7. **Risks.** Specific to this phase; reference hazards from
   section 7 where relevant.
8. **Estimated effort.** In ideal engineering days, with a low
   estimate, an expected estimate, and a high estimate.
9. **Tier 1 / Tier 2 mapping.** Which T-numbered features this
   phase advances and how far.

Phases are sequenced so that Tier 1 ships before Tier 2 begins. The
plan touches Tier 3 only in the final "post-V1 sketch" section, which
lists Tier 3 features and the V1 hooks that enable them.

### 8.5 Phased TODO lists (`docs/todo/`)

One file per phase: `phase-NN-todo.md`. Filename matches the phase
identifier in the phased plan exactly.

Each file MUST contain:

1. **Phase header.** Identifier, title, link back to the phased
   plan section.
2. **Pre-flight checklist.** Things to confirm before starting
   work (entry criteria from the phased plan).
3. **Tasks.** A nested checklist. Each leaf task MUST have:
   - A short imperative title.
   - An acceptance criterion (one sentence, testable).
   - A definition-of-done note (what running command or what
     observable outcome confirms the task).
   - A pointer to the relevant Tier 1/Tier 2 feature identifier.
4. **Cross-references.** Which ADRs, PRD sections, and architecture
   sections this phase implements or relies on.
5. **Sign-off checklist.** Exit criteria from the phased plan,
   re-stated as a checkbox list.

TODO files are checked into the repository. Progress is recorded by
ticking checkboxes in commits.

---

## 9. Voice, tone, and prohibitions

- **No marketing language.** "Best-in-class," "world-class,"
  "seamless," "magical" are forbidden.
- **No hedging.** "We could," "it might be nice," "perhaps in the
  future" are forbidden inside requirement statements. Use them only
  in the explicit "Open questions" section.
- **No filler abbreviations in body text.** "i.e.," "e.g.," and
  "etc." are forbidden. Write "for example," "that is," "and so on."
- **No "the system" passive voice when an actor is known.** Say
  "Trilithon does X" or "the daemon does X," not "the system does X."
- **No emoji** anywhere in any specification document.
- **No unattributed claims.** A non-obvious technical claim cites
  Caddy documentation, an RFC, or a numbered hazard from section 7.
- **No links to external URLs that may rot.** Cite by name and
  version.

---

## 10. Procedure for using this prompt

When asked to generate a Trilithon specification artifact:

1. Read this entire prompt.
2. Identify which artifact is being requested (ADR, PRD, architecture,
   phased plan, TODO list).
3. Locate the matching standard in section 8.
4. Inventory which Tier 1/2/3 features and which hazards are in scope
   for this artifact.
5. Produce the artifact, observing every constraint in sections 2,
   3, 7, 8, and 9.
6. End the artifact with an "Open questions" section if any
   ambiguity remains. Open questions are listed, not silently
   resolved.

---

## 11. Provenance

This prompt synthesises the conclusions of:

- The original brainstorming document at
  `docs/planning/Brainstorming/Claude Trilithon Prompt.md`,
  containing dialogues with three external language models (Kimi,
  ChatGPT, Gemini).
- The adversarial review delivered in chat on 2026-04-30, which
  produced the Tier 1 / Tier 2 / Tier 3 split, the hazard list, and
  the constraint set.
- The user's standing instructions in `CLAUDE.md` files at the user
  and project level.

This prompt supersedes all prior informal feature lists. Where this
prompt and an earlier informal note disagree, this prompt wins.
