# Trilithon — Product Requirements Document

## 1. Document control

- **Document:** Product Requirements Document for Trilithon V1
- **Version:** 1.0.0
- **Date:** 2026-04-30
- **Owner:** Project lead
- **Status:** Accepted as the V1 product baseline. Supersedes informal feature lists in `docs/planning/Brainstorming/`.
- **Source authority:** This document is downstream of `docs/prompts/PROMPT-spec-generation.md`. Where the prompt and this document disagree, the prompt wins and the disagreement MUST be raised as an open question in section 10.

## 2. Glossary

The canonical glossary for Trilithon lives in section 3 of `docs/prompts/PROMPT-spec-generation.md`. That glossary is reproduced verbatim below; no new terms are introduced by this document.

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
| **Mutation** | A typed, idempotent operation on desired state (create route, update upstream, attach policy, and similar). Each mutation produces exactly one snapshot. |
| **Apply** | The act of writing a snapshot to Caddy's Admin API and confirming success. |
| **Rollback** | An apply whose target is a prior snapshot. |
| **Proposal** | A mutation that has been generated (by Docker discovery, by a language model, by an import process) but not yet approved by an authorised actor. |
| **Capability probe** | The startup procedure that asks Caddy `GET /config/apps` and `GET /reverse_proxy/upstreams` to determine which optional Caddy modules are loaded. |
| **Policy preset** | A named bundle of access controls, headers, and rate-limit settings (such as `internal-app` or `public-admin`) that can be attached to a route. |
| **Audit record** | An immutable log entry recording who did what, when, with what intent, and with what result. |
| **Correlation identifier** | A ULID propagated through every layer of the system, joining HTTP requests, mutations, snapshots, audit records, and language-model sessions. |
| **Language model agent** | An external large-language-model client invoking Trilithon's typed tool gateway over an authenticated channel. |
| **Tool gateway** | The bounded, typed surface that language model agents are permitted to call. Distinct from the human web UI's API; both ride on the same underlying mutation primitives. |

## 3. Vision

Trilithon exists because operating Caddy well is a coordination problem, not a configuration problem. Caddy itself is excellent. Its JSON Admin API is unusually clean, its reverse-proxy behaviour is correct by default, and its automatic certificate management has solved a generation of production embarrassments. What Caddy lacks, by design, is opinion about how a small team or a single operator manages change over time. Today the Caddy operator is left to choose between hand-edited Caddyfiles in version control (legible, lossy, and out of sync with running state the moment anything goes wrong), raw `curl` calls against the Admin API (powerful, dangerous, and silent), and various third-party wrappers that each invent their own truth model. Trilithon takes a clear position: desired state is a typed, persistent, auditable model owned by Trilithon, and Caddy's running state is a downstream consequence of that model.

The world Trilithon creates is one where every change to a reverse proxy is named, attributed, reversible, and explainable. A home-lab operator can add a route, see exactly what JSON Caddy received, and roll the change back without reading documentation. A small DevOps team can let a language model propose configuration changes from natural-language requests, knowing those proposals queue for human approval and cannot bypass policy. A small business operator can hand the audit log to an auditor and have it tell the truth. Drift, when it occurs, surfaces as an event with three named resolutions rather than as a silent overwrite. Secrets stay encrypted at rest and never appear in diffs. The user runs Trilithon on hardware they own, against data they own, with no telephone home.

Trilithon is explicitly not a proxy, not a web application firewall, not a load balancer, not a service mesh, and not a container orchestrator. Trilithon does not handle a single byte of user-facing HTTP traffic; Caddy does. Trilithon does not replace Caddy's automatic HTTPS, its module system, or its configuration semantics; Trilithon orchestrates them. Trilithon is not a cloud product, a hosted service, or a multi-tenant platform. Trilithon is not an agent framework: language-model integration in V1 is bounded, audited, and explicitly excludes any path by which a model can apply a change without authenticated human approval. Anything in section 6 of the binding prompt is OUT OF SCOPE FOR V1.

## 4. Target users

Trilithon's V1 design centres on three named personas. Each persona reflects a real deployment shape derived from the brainstorming corpus and the adversarial review of 2026-04-30. The technical-level scale used below is: **Beginner** (comfortable installing software, uncomfortable editing configuration files), **Intermediate** (comfortable editing configuration files and reading documentation, uncomfortable debugging a network failure under time pressure), **Advanced** (comfortable reading vendor source code, debugging under time pressure, and writing scripts to automate operational work).

### 4.1 Persona A — Sam, the home-lab self-hoster

- **Role:** Hobbyist. Operates a personal home lab serving roughly 15 self-hosted applications (media server, photo library, note-taking, recipes, dashboards) over a single public hostname using a wildcard certificate from Let's Encrypt.
- **Technical level:** Intermediate.
- **Goals:**
  - Add a new self-hosted application behind a new subdomain in under five minutes without rereading the Caddy documentation.
  - See, at a glance, which routes are healthy and which are broken.
  - Roll back a misconfiguration immediately when an application stops responding.
  - Avoid accidentally exposing an internal-only service to the public internet.
- **Pains:**
  - Hand-edited Caddyfiles drift between version control and running state when changes are made in a hurry.
  - When a route stops working, narrowing the cause to "Caddy", "DNS", "the application", or "the certificate" requires four different command-line tools.
  - Wildcard certificates make it easy to publish a new hostname by accident with no warning.
  - The Caddy admin endpoint is unauthenticated by default, and exposing it on a home network is a known foot-gun.
- **Success criteria:**
  - Sam adds a route through the Trilithon web UI in under five minutes on the first attempt.
  - Sam recovers from a broken apply without consulting external documentation.
  - When Sam adds a hostname that falls under an existing wildcard certificate, Trilithon surfaces the implication before apply.

### 4.2 Persona B — Devi, the small-team DevOps engineer

- **Role:** Solo or small-team engineer at a 10-to-50-person company. Owns roughly 40 internal and external routes across a Docker-based deployment on two virtual machines. Reports to a non-technical manager.
- **Technical level:** Advanced.
- **Goals:**
  - Treat reverse-proxy configuration as code that survives a laptop wipe and a colleague's review.
  - Allow a language model to draft routine configuration changes (rename a route, update an upstream port, attach a standard policy preset) without granting it apply authority.
  - Produce an audit log that satisfies a "who changed what, when" question from a security review.
  - Detect when a teammate has poked Caddy's admin port directly and surface the deviation rather than overwrite it.
- **Pains:**
  - "Last write wins" between two engineers editing the same configuration is a real source of outages.
  - Existing Caddy management tools either round-trip through Caddyfile (lossy) or expose the raw Admin API (unsafe).
  - Justifying an LLM-assisted workflow to a security-conscious manager requires demonstrable boundaries, not promises.
  - Backup, restore, and disaster recovery for Caddy configuration are typically ad-hoc.
- **Success criteria:**
  - Devi configures a language-model agent in `propose` mode and demonstrates that it cannot apply a change directly.
  - Two simultaneous edits produce a typed conflict error, not silent data loss.
  - A backup taken on Monday restores cleanly to a different host on Friday.
  - An auditor reading the audit log can answer "what changed, by whom, with what intent, and what was the result" for any 30-day window.

### 4.3 Persona C — Priya, the small-business operator

- **Role:** Owner-operator of a small services business with one application server and one marketing website. Hires a contractor twice a year for technical work but otherwise operates the system alone.
- **Technical level:** Beginner.
- **Goals:**
  - Keep the website and the customer portal reachable.
  - Receive a clear warning before a certificate expires.
  - Understand, in plain language, why a request was blocked when a customer reports an error.
  - Hand the contractor a credential that can read configuration but not change it.
- **Pains:**
  - Configuration files are opaque, and small typos cause outages that block paying customers.
  - Certificate renewal failures are silent until a customer calls.
  - "Why was my request blocked?" requires correlating multiple logs by hand.
  - Sharing credentials with a contractor means sharing everything; there is no graceful read-only mode.
- **Success criteria:**
  - Priya creates her first route through the web UI within 10 minutes of installation, without contractor help.
  - A failing certificate renewal produces a flag in the web UI before the certificate expires.
  - For a representative customer-reported access failure, Priya can read the explanation in the access log viewer and identify the cause without external help.

## 5. Scope — Tier 1 features

Each Tier 1 feature is non-optional for V1. Removing any one feature breaks the others. The order here matches the prompt; ordering does not imply implementation sequence (the phased plan owns sequence).

### 5.1 T1.1 — Configuration ownership loop

**User story.** As an operator, I want Trilithon to own a typed model of desired Caddy configuration and reconcile that model with Caddy automatically, so that my intent and Caddy's running state never drift silently.

**Functional requirements.**

1. Trilithon MUST persist a typed model of desired Caddy configuration to SQLite.
2. Trilithon MUST validate every desired-state change locally before any network call to Caddy.
3. Trilithon MUST compute a diff between desired state and running state before each apply.
4. Trilithon MUST reconcile differences through Caddy's `POST /load` (full) or `PATCH /config/...` (partial) endpoints, choosing the partial path when the diff is local to a single Caddy module path.
5. Trilithon MUST attach a monotonically increasing `config_version` integer to every desired state.
6. Trilithon MUST reject an apply whose `config_version` is stale relative to the current desired state and MUST return a typed conflict error.
7. Trilithon MUST NOT advance the desired-state pointer when Caddy's validation step fails at apply.

**Non-functional requirements.**

- The end-to-end apply path from mutation receipt to Caddy acknowledgement SHOULD complete with p95 latency under 2 seconds for routine route changes against a healthy local Caddy.
- The reconciler MUST NOT issue redundant applies when desired state equals running state.
- All Caddy admin traffic MUST traverse a Unix domain socket or `localhost` interface; the daemon MUST refuse to start if its configured Caddy admin endpoint is non-loopback (hazard H1).

**Dependencies.** External: Caddy 2.8 or later with admin interface bound to loopback or Unix socket. Internal: T1.6 (typed mutations), T1.7 (audit log), T1.11 (capability probe).

**Acceptance criteria.**

1. Given desired state X equal to running state X, no apply call is issued.
2. Given desired state Y differing from running state X, exactly one apply is issued and the resulting running state equals Y on the next read.
3. An apply rejected by Caddy validation leaves desired state and `config_version` unchanged and produces a structured error visible in the web UI and in the audit log.
4. A second mutation against a stale `config_version` is rejected with a typed `STALE_CONFIG_VERSION` error referencing the current version.

### 5.2 T1.2 — Snapshot history with content addressing

**User story.** As an operator, I want every mutation to produce an immutable, content-addressed snapshot, so that I can identify, reference, and roll back to any prior desired state with confidence that history has not been rewritten.

**Functional requirements.**

1. Trilithon MUST write exactly one snapshot row to SQLite for every successful mutation.
2. Snapshot identifiers MUST be computed as the SHA-256 hash of the canonical JSON serialisation of the snapshot body.
3. Trilithon MUST NOT issue any `UPDATE` statement against the snapshot table.
4. Each snapshot MUST record: snapshot identifier, parent snapshot identifier, actor identifier, intent (free text supplied by the actor), correlation identifier, Caddy version at apply time, Trilithon version, UTC Unix timestamp, monotonic timestamp, and the canonical desired-state JSON.
5. Identical snapshot bodies MUST deduplicate to a single row referenced by all parents.

**Non-functional requirements.**

- Snapshot insertion MUST complete in under 50 milliseconds at the 95th percentile against a database under 1 GiB.
- Canonical JSON serialisation MUST be deterministic across runs and across platforms.

**Dependencies.** Internal: T1.6, T1.7, T1.15 (secrets redaction prior to hashing where applicable).

**Acceptance criteria.**

1. Two semantically identical mutations produce snapshots with identical identifiers.
2. The snapshot table contains no `UPDATE` statement in the codebase, verified by static check in `just check`.
3. A power-loss event during a mutation never produces a partial snapshot row (Write-Ahead Log mode, hazard H14).
4. The Caddy version recorded in a snapshot matches the version reported by `GET /` on Caddy at the time of apply.

### 5.3 T1.3 — One-click rollback with preflight

**User story.** As an operator, I want to roll back to any prior snapshot with a single action, with Trilithon checking my upstreams, certificates, and modules before applying, so that I do not roll forward into a broken environment.

**Functional requirements.**

1. Trilithon MUST allow any prior snapshot to be designated the new desired state.
2. Before applying a rollback, Trilithon MUST run a preflight that checks: TCP reachability of every referenced upstream, validity of every referenced TLS certificate, existence of every referenced Docker container (where Docker discovery is configured), and presence of every referenced Caddy module per the latest capability probe.
3. A preflight failure MUST produce a structured error listing every failing condition with a stable identifier per condition.
4. The user MAY override a preflight failure on a per-condition basis; each override MUST be recorded in the audit log with the overriding actor and a free-text justification.
5. A rollback that passes preflight MUST apply atomically through the same path as a forward apply (T1.1).

**Non-functional requirements.**

- Preflight against a desired state of up to 200 routes MUST complete in under 5 seconds at the 95th percentile.
- Preflight MUST be cancellable by the user.

**Dependencies.** T1.1, T1.2, T1.10, T1.11.

**Acceptance criteria.**

1. A rollback whose referenced upstream is unreachable fails preflight with `UPSTREAM_UNREACHABLE` and the upstream identifier.
2. A rollback whose referenced module is absent fails preflight with `MODULE_UNAVAILABLE` and the module name.
3. A user-supplied override for a single failing condition allows the apply to proceed; remaining failing conditions still block the apply.
4. The override and its justification appear in the audit log keyed by correlation identifier.

### 5.4 T1.4 — Drift detection on startup and on schedule

**User story.** As an operator, I want Trilithon to notice when Caddy's running state has diverged from my desired state, so that I am informed before a surprising change becomes a permanent surprise.

**Functional requirements.**

1. Trilithon MUST fetch Caddy's running configuration on daemon startup and compute a diff against current desired state.
2. Trilithon MUST repeat this check on a configurable interval with a default of 60 seconds.
3. A non-empty diff MUST be recorded in the audit log as a drift event with the full diff body redacted by the secrets redactor.
4. Trilithon MUST surface drift in the web UI with three named resolutions: **adopt** (running state becomes the new desired state), **reapply** (desired state overwrites running state), **reconcile** (open the diff in the dual-pane editor for manual resolution).
5. Trilithon MUST NOT silently overwrite Caddy on detection of drift.

**Non-functional requirements.**

- A drift check MUST NOT block a concurrent mutation request for longer than 100 milliseconds.
- The drift check interval MUST be configurable down to 5 seconds and up to 24 hours.

**Dependencies.** T1.1, T1.2, T1.7, T1.12.

**Acceptance criteria.**

1. A direct `PATCH` to Caddy's admin API by an external actor produces a drift event in the audit log within one check interval.
2. Each of the three resolutions is reachable from the drift event in the web UI in one click.
3. No automatic reapply occurs without explicit user selection.
4. The drift event records the actor as `external` and identifies the diff scope (which Caddy paths changed).

### 5.5 T1.5 — Caddyfile one-way import

**User story.** As an operator with an existing Caddyfile, I want to import that Caddyfile into Trilithon once, so that I can adopt Trilithon without rewriting my configuration by hand.

**Functional requirements.**

1. Trilithon MUST accept a Caddyfile as input through both the web UI (file upload) and the command-line interface (path argument).
2. Trilithon MUST parse the Caddyfile, validate it through the same pipeline used for user mutations (T1.1), and convert it to a desired-state object.
3. Trilithon MUST persist the original Caddyfile bytes as an attachment to the resulting import snapshot.
4. Trilithon MUST NOT, anywhere in the codebase, write a Caddyfile.
5. Imports that lose information (comments, ordering, unsupported directives) MUST emit structured warnings listing each lost element with line and column.

**Non-functional requirements.**

- Imports MUST be size-bounded (default 10 MiB Caddyfile or 50,000 directives, whichever is reached first), with a documented configuration override (hazard H15).
- Import parse plus validation MUST complete in under 10 seconds for the size bound at the 95th percentile.

**Dependencies.** T1.1, T1.2, T1.6, T1.11.

**Acceptance criteria.**

1. A fixture corpus of representative Caddyfiles imports without error and produces semantically equivalent runtime behaviour, verified by integration tests that compare Caddy's response on a stable request set.
2. An import that exceeds the size bound is rejected with `IMPORT_SIZE_BOUND` and the bound value.
3. An import containing a directive Trilithon does not understand is rejected with `IMPORT_UNSUPPORTED_DIRECTIVE` and the directive name; the user MAY proceed by manually translating that section.
4. The import snapshot's attachment contains the original Caddyfile bytes, byte-for-byte.

### 5.6 T1.6 — Typed mutation API

**User story.** As an operator and as a language-model integrator, I want every change to desired state to flow through a finite set of typed operations, so that the surface is auditable, testable, and shared between human and machine actors.

**Functional requirements.**

1. Every change to desired state MUST occur through a member of a finite, versioned set of typed mutation operations.
2. Each mutation MUST have a Rust type, a JSON schema published to the tool gateway, a stated pre-condition, a stated post-condition, and a documented idempotency story (idempotent, retryable, or single-shot).
3. The mutation set MUST be closed under composition: any sequence of valid mutations against a valid desired state either yields a valid desired state or fails at a single, identifiable mutation.
4. There MUST NOT be an "apply arbitrary JSON" mutation. The dual-pane editor (T1.12) commits through a special `replace_desired_state` mutation that itself runs full validation.
5. The mutation surface MUST be the single API consumed by the web UI and the tool gateway. The web UI MUST NOT have private mutations.

**Non-functional requirements.**

- The published JSON schemas MUST validate cleanly under JSON Schema 2020-12.
- Adding a new mutation MUST require a schema version bump and a documented migration story for older clients.

**Dependencies.** T1.1, T1.2, T1.7, T1.14.

**Acceptance criteria.**

1. The codebase contains no path through which desired state can be mutated other than the typed mutation handlers.
2. Each mutation has unit tests covering its pre-condition, post-condition, and idempotency claim.
3. The tool gateway publishes the same schemas the web UI consumes; a schema-equality test guards this in `just check`.

### 5.7 T1.7 — Audit log with correlation identifiers

**User story.** As an operator and as an auditor, I want every consequential event in Trilithon to produce an immutable, correlated audit record, so that I can reconstruct any past decision path without ambiguity.

**Functional requirements.**

1. Trilithon MUST write exactly one audit row for each of: mutation request, apply, rollback, drift event, language-model interaction, authentication event (login, logout, failed login, token issuance, token revocation).
2. Each audit row MUST carry a correlation identifier (ULID) propagated from the originating HTTP request through every component that handled it.
3. Trilithon MUST NOT issue any `UPDATE` statement against the audit log table.
4. Audit rows MUST NOT contain plaintext secrets. A secrets-aware redactor (T1.15) MUST sit between the diff engine and the audit log writer; no code path may bypass it (hazard H10).
5. All wall-clock timestamps stored in audit rows MUST be UTC Unix timestamps. All wall-clock timestamps displayed to users MUST be rendered in the viewer's local time zone (hazard H6).

**Non-functional requirements.**

- Audit log writes MUST be durable: a successful mutation response implies the audit row has reached `fsync`-confirmed disk.
- Audit log retention MUST be configurable, with a default of 365 days. Pruning MUST itself emit an audit row recording the prune.

**Dependencies.** T1.2, T1.6, T1.15.

**Acceptance criteria.**

1. The codebase contains no `UPDATE audit_log` statement, verified by static check.
2. A diff containing a known-secret field (basic-auth password fixture) is redacted in the resulting audit row; a regression test asserts the redaction.
3. A correlation identifier visible in the web UI for a given mutation appears unchanged on the apply, audit row, and (where applicable) language-model session record.
4. Stored timestamps survive a daylight-saving-time transition without ambiguity; displayed timestamps reflect the viewer's current zone.

### 5.8 T1.8 — Route create, read, update, delete

**User story.** As an operator, I want to create, view, update, and delete reverse-proxy routes through the typed mutation API, so that I can perform the most common operational task without leaving Trilithon.

**Functional requirements.**

1. Trilithon MUST expose four typed mutations for routes: `create_route`, `update_route`, `delete_route`, and `read_route` (the read is a query, not a mutation, but is served by the same surface).
2. A `create_route` mutation MUST accept at minimum: hostname (or hostname pattern), upstream address (host plus port), and an optional policy preset reference.
3. An `update_route` mutation MUST be atomic from the perspective of the public client: there MUST NOT be an observable window in which the route serves a half-updated configuration.
4. A `delete_route` mutation MUST remove the route from desired state and from running state in a single apply.
5. Route reads MUST return all fields including the resolved policy preset (with version) and the current health status.

**Non-functional requirements.**

- Newly created routes SHOULD begin serving traffic within 5 seconds of approval against a healthy local Caddy.
- Deleted routes SHOULD stop serving traffic within 5 seconds of approval against a healthy local Caddy.
- Apply latency p95 for a single route mutation MUST remain under 2 seconds end to end (hazard H4 acknowledged: TLS provisioning latency is reported separately per T1.9 and section 8).

**Dependencies.** T1.1, T1.2, T1.6, T1.7, T1.9, T1.10.

**Acceptance criteria.**

1. A `create_route` integration test issues a request immediately after approval and observes a response from the route's upstream within 5 seconds.
2. An `update_route` test concurrently issues requests during the apply and observes either old or new behaviour, never a malformed response.
3. A `delete_route` integration test confirms the route returns Caddy's configured "no matching route" response within 5 seconds.
4. A route referencing a hostname for which TLS provisioning is in progress is reported with a distinct `ISSUING_CERTIFICATE` status (hazard H17).

### 5.9 T1.9 — TLS certificate visibility

**User story.** As an operator, I want to see, for every host Trilithon manages, its certificate issuer, expiry, and renewal status, so that I am never surprised by an expiring or failing certificate.

**Functional requirements.**

1. Trilithon MUST poll Caddy's `GET /config/apps/tls/certificates` and `GET /reverse_proxy/upstreams` endpoints on a configurable interval (default 5 minutes) and surface the result per host in the web UI.
2. Trilithon MUST flag certificates expiring within 14 days as **amber**.
3. Trilithon MUST flag certificates expiring within 3 days, or whose most recent renewal attempt failed, as **red**.
4. Trilithon MUST surface "issuing certificate" as a distinct status from "applied" for routes with hosts whose certificate has not yet been provisioned (hazard H17).
5. Trilithon MUST surface ACME error messages, when present in Caddy's reported certificate state, with the message text, the upstream ACME directory, and a "retry now" action that issues a Caddy reload.

**Non-functional requirements.**

- Certificate visibility data MUST update in the web UI within 30 seconds of a Caddy state change.
- Certificate state queries MUST NOT contact any non-loopback service from Trilithon directly; all certificate information flows through Caddy.

**Dependencies.** T1.1, T1.7, T1.8, T1.11.

**Acceptance criteria.**

1. A fixture certificate with 13 days of remaining validity displays as amber.
2. A fixture certificate with 2 days of remaining validity displays as red.
3. A simulated ACME failure (forced via a Caddy fixture) appears as a red flag with the failure message and a working retry action.
4. A newly-created public route displays `ISSUING_CERTIFICATE` until Caddy reports a valid certificate, after which it transitions to `APPLIED`.

### 5.10 T1.10 — Basic upstream health visibility

**User story.** As an operator, I want to know whether the upstream behind each route is reachable, so that I can distinguish a Caddy problem from an application problem at a glance.

**Functional requirements.**

1. Trilithon MUST surface, per route, a reachability status sourced from two signals: Caddy's `/reverse_proxy/upstreams` endpoint and a Trilithon-side TCP connect probe.
2. Trilithon MUST allow per-route disabling of the Trilithon-side probe (some upstreams reject unsolicited TCP connects).
3. Reachability state in the web UI MUST update within 30 seconds of an upstream becoming reachable or unreachable.
4. Trilithon MUST emit a `health_change` audit row each time a route transitions between reachability states.

**Non-functional requirements.**

- Probe traffic from Trilithon MUST identify itself in any TCP-level metadata available, and SHOULD be rate-limited to no more than one probe per upstream per 10 seconds by default.
- Probes MUST honour a configurable timeout (default 2 seconds) and MUST NOT block the reconciler.

**Dependencies.** T1.1, T1.7, T1.8.

**Acceptance criteria.**

1. An upstream brought up after being down transitions to reachable in the UI within 30 seconds.
2. A user-disabled probe results in reachability status sourced exclusively from Caddy's reporting, with a UI label clarifying the source.
3. A health transition produces exactly one audit row per transition (no flapping amplification beyond Caddy's own reporting cadence).

### 5.11 T1.11 — Caddy capability probe

**User story.** As an operator, I want Trilithon to know which Caddy modules are loaded, so that I am never offered a feature that will fail at apply.

**Functional requirements.**

1. Trilithon MUST query `GET /config/apps` and any module-discovery endpoints exposed by the running Caddy on daemon startup and on every Caddy reconnect.
2. Trilithon MUST cache the result and revalidate on reconnect; the cached result MUST be reset whenever the daemon detects that Caddy's process identity has changed.
3. Trilithon MUST gate UI features that depend on optional modules (rate limiting, web application firewall, layer-4 proxying, bot challenge) on the probe result and MUST mark unavailable features as "unavailable on this Caddy build" with documentation pointing to enablement instructions.
4. Trilithon MUST reject, at desired-state validation, any mutation that references a module the probe reports as absent (hazard H5). This rejection MUST happen before apply, not at apply.

**Non-functional requirements.**

- The probe MUST complete in under 1 second at the 95th percentile against a healthy local Caddy.
- Probe failures MUST NOT crash the daemon; they MUST surface as a degraded-mode banner in the web UI and a structured warning in the audit log.

**Dependencies.** T1.1, T1.6.

**Acceptance criteria.**

1. A stock Caddy build (without `caddy-ratelimit`) results in rate-limit features being disabled in the web UI with a documentation link.
2. A custom Caddy build with the relevant modules unlocks those features automatically on next reconnect.
3. A mutation referencing an absent module is rejected with `MODULE_UNAVAILABLE` and the module name, with no Caddy admin call attempted.

### 5.12 T1.12 — Dual-pane configuration editor

**User story.** As an advanced operator, I want a side-by-side editor that shows my configuration as legible structured form on one side and raw Caddy JSON on the other, so that I can use Trilithon's typed surface without losing access to Caddy's full expressiveness.

**Functional requirements.**

1. The web UI MUST provide a side-by-side editor: legible structured form on the left, raw Caddy JSON on the right.
2. Edits in either pane MUST validate live (debounced, under 300 milliseconds) and MUST update the other pane on each successful validation.
3. Validation errors MUST point to the offending line and key with a stable error identifier and human-readable message.
4. The "apply" action MUST be disabled while either pane fails validation.
5. A successful edit MUST display a preview diff against current desired state before the user commits.

**Non-functional requirements.**

- The editor MUST be usable on a 1280-pixel-wide display without horizontal scrolling on either pane.
- The editor MUST meet WCAG 2.1 Level AA contrast and keyboard-navigation requirements.

**Dependencies.** T1.1, T1.2, T1.6, T1.13.

**Acceptance criteria.**

1. A typo in a JSON key produces a structured error pointing to the line and key within 300 milliseconds.
2. The "apply" button is disabled while either pane is in an error state.
3. A round-trip edit (legible form → JSON → legible form) preserves the original semantics and produces an empty diff against the starting state.

### 5.13 T1.13 — Local web UI delivery

**User story.** As an operator, I want to install Trilithon, open a local URL, and reach the web UI without further configuration, so that I can begin work immediately.

**Functional requirements.**

1. The Trilithon daemon MUST serve the web UI on `127.0.0.1:<port>`, with a sensible default port of 7878, configurable.
2. The web UI MUST be reachable only on the loopback interface by default. Binding to a non-loopback interface MUST require an explicit configuration flag.
3. When a non-loopback bind is configured, the daemon MUST print a stark "authentication required" warning at startup and the web UI MUST refuse to serve unauthenticated routes regardless of bootstrap mode.
4. The daemon MUST serve the React build as static assets with appropriate cache headers.
5. The web UI MUST function in current major versions of Chromium-based browsers, Firefox, and Safari.

**Non-functional requirements.**

- First contentful paint of the web UI on a fresh load against the local daemon SHOULD complete in under 1 second on a modern laptop.
- The daemon's own static-asset serving MUST NOT exceed 50 milliseconds p95 for any single asset under 1 MiB.

**Dependencies.** T1.14.

**Acceptance criteria.**

1. A user with no prior configuration installs Trilithon, opens `http://127.0.0.1:7878`, and reaches the bootstrap login screen.
2. A daemon configured to bind `0.0.0.0` prints the security warning at startup and refuses to serve any unauthenticated route.
3. The web UI passes a Lighthouse accessibility audit at score 90 or higher.

### 5.14 T1.14 — Authentication and session management

**User story.** As an operator, I want the web UI and tool gateway to be authenticated, so that I am the only actor (or set of actors) able to mutate desired state.

**Functional requirements.**

1. Trilithon MUST authenticate every web UI mutation route and every tool gateway request.
2. V1 MUST support local user accounts with passwords hashed using Argon2id with parameters at or above OWASP 2024 recommendations.
3. V1 MUST support a single-user bootstrap mode in which one local account is created automatically on first run.
4. The bootstrap account's credentials MUST be written to a permission-restricted file (`0600`) outside the SQLite database, MUST NOT appear in process arguments, environment variables, or logs, and the user MUST be prompted to change the password on first login (hazard H13).
5. Sessions MUST be stored server-side, MUST expire after a configurable idle window (default 12 hours), and MUST be revocable by an authenticated user.
6. Tool gateway tokens MUST be distinct from user sessions, MUST carry a name and a scope, MUST expire on a configurable lifetime (default 30 days), and MUST be individually revocable.

**Non-functional requirements.**

- Failed authentication attempts MUST be rate-limited per source address.
- Successful and failed authentication events MUST appear in the audit log with the source address and user agent.

**Dependencies.** T1.7, T1.13, T1.15.

**Acceptance criteria.**

1. No mutation endpoint returns success without a valid session or token; an integration test asserts this against the full mutation surface.
2. The bootstrap credentials file is created with mode `0600` and is removed on first password change.
3. A revoked token is rejected on its next use with a typed `TOKEN_REVOKED` error.
4. Five consecutive failed logins from one source address result in a configurable backoff before the next attempt is accepted.

### 5.15 T1.15 — Secrets abstraction

**User story.** As an operator, I want fields marked secret in the schema to be encrypted at rest and never appear in plaintext in audit diffs or read endpoints, so that a stolen database file does not leak my credentials.

**Functional requirements.**

1. Every field marked secret in the mutation schema MUST be encrypted at rest using XChaCha20-Poly1305 with a per-field nonce.
2. The data-encryption key MUST be derived from a master key that lives outside the SQLite database. The master key MUST be stored in the operating system keychain (macOS Keychain on macOS, the Secret Service API on Linux where available) with a permission-restricted file fallback (`0600`) on systems without a keychain.
3. Read endpoints MUST return secret fields as opaque references, never plaintext, except through an explicit `reveal_secret` operation that itself produces an audit row identifying the actor, the field, and the correlation identifier.
4. The secrets redactor (T1.7) MUST replace secret values in audit diffs with a stable opaque token before write.
5. Backup and restore (T2.12) MUST encrypt secret material with a passphrase distinct from the master key.

**Non-functional requirements.**

- A leaked SQLite file MUST NOT permit secret recovery without the master key.
- Secret encryption and decryption MUST add no more than 5 milliseconds p95 to any single field operation.

**Dependencies.** T1.6, T1.7, T1.14.

**Acceptance criteria.**

1. A test extracts the SQLite file and the codebase, omitting the master key, and demonstrates that secret fields cannot be decrypted.
2. An audit diff containing a basic-auth password contains the redaction token, not the password value.
3. A `reveal_secret` call produces an audit row referencing the field and actor.
4. The keychain integration test on macOS, the Secret Service test on Linux, and the file-fallback test on a minimal container all pass.

## 6. Scope — Tier 2 features

Tier 2 features ship in V1 but only after Tier 1 is feature-complete and under integration test. Each Tier 2 feature depends on Tier 1 primitives; retrofitting them later would be expensive. Each is required for V1 release.

### 6.1 T2.1 — Docker container discovery (proposal-based)

**User story.** As an operator running services in containers, I want Trilithon to notice containers carrying Caddy labels and emit proposals I can review, so that I add new services to my proxy without writing the same configuration twice.

**Functional requirements.**

1. Trilithon MUST watch a configured Docker (or Podman) endpoint for container lifecycle events.
2. Containers carrying labels under the `caddy.*` namespace MUST produce proposals (T1.6 mutations awaiting approval), not auto-applied changes.
3. Trilithon MUST emit a proposal within 5 seconds of a labelled container starting and within 5 seconds of a labelled container being destroyed.
4. A label conflict (two containers claiming the same hostname) MUST produce a single conflict proposal listing both candidates; Trilithon MUST NOT emit two competing proposals.
5. Wildcard certificate matches MUST be highlighted in the proposal UI per T2.11.
6. The Docker socket MUST be accessible only to the Trilithon daemon, never to the Caddy container, in any official deployment artefact (hazard H11).
7. On first run with Docker discovery enabled, Trilithon MUST emit a stark warning explaining that mounting the Docker socket grants effective root on the host.

**Non-functional requirements.**

- Discovery MUST tolerate Docker daemon restarts without losing previously-emitted proposals.
- Discovery MUST be disabled by default; enabling it MUST be an explicit configuration step.

**Dependencies.** T1.6, T1.7, T1.11, T2.11.

**Acceptance criteria.**

1. Starting a labelled container produces a proposal with the expected route fields within 5 seconds.
2. Destroying a labelled container produces a "remove route" proposal within 5 seconds.
3. Two containers with the same hostname label produce one conflict proposal naming both.
4. A first-run with discovery enabled produces the trust-grant warning in stdout and in the audit log.

### 6.2 T2.2 — Policy presets

**User story.** As an operator, I want to attach a named, opinionated policy bundle to a route, so that I configure security headers, access controls, and rate limits consistently without rebuilding them per route.

**Functional requirements.**

1. Trilithon MUST ship the seven V1 presets named in the prompt: `public-website`, `public-application`, `public-admin`, `internal-application`, `internal-admin`, `api`, `media-upload`.
2. Each preset MUST be versioned. A preset version MUST be stored on every route that uses it.
3. A change to a preset definition MUST NOT silently mutate routes already using a prior version. The web UI MUST surface a per-route "upgrade preset" prompt with a diff.
4. Presets that depend on optional Caddy modules MUST be marked as such; on a Caddy build lacking those modules, the route MUST still apply with the optional features omitted, and a structured warning MUST appear in the audit log and the route detail view.
5. Attaching a preset MUST be a single-mutation, single-click operation in the web UI.

**Non-functional requirements.**

- Preset content MUST be reviewed for changes only via committed code; runtime modification of preset definitions is forbidden.
- The preset upgrade UI MUST be reachable in three or fewer clicks from any route.

**Dependencies.** T1.6, T1.7, T1.11.

**Acceptance criteria.**

1. Each named preset has a fixture test asserting its produced Caddy JSON for a representative route.
2. Bumping a preset version does not change running state for any existing route until that route is explicitly upgraded.
3. A preset that requires `caddy-ratelimit` applies cleanly on a build without it, with a warning recorded in the audit log naming the omitted feature.

### 6.3 T2.3 — Language-model "explain" mode

**User story.** As an operator, I want to grant a language model read-only access to my desired state, audit log, and per-object history, so that the model can answer questions in natural language without being able to mutate anything.

**Functional requirements.**

1. The tool gateway MUST expose a defined read-only subset of the typed API to language-model agents in `explain` mode.
2. Models in `explain` mode MUST NOT have access to any mutation operation, any shell, any filesystem access, and any non-Trilithon network endpoint.
3. Every model interaction MUST produce an audit row recording the model identity, the prompt, the response, the correlation identifier, and the bytes read.
4. The tool gateway MUST prepend a system message on every model interaction clarifying that user data (log lines, hostnames, free-text intent) is data, not instruction, and MUST refuse to act on instruction-like content found in logs (hazard H16).
5. The user MUST be able to revoke a model's access in one click, immediately invalidating its token.

**Non-functional requirements.**

- Tool gateway response latency for read operations MUST remain under 500 milliseconds p95 against a database under 1 GiB.
- Audit rows for model interactions MUST redact secret material identically to other audit rows (hazard H10).

**Dependencies.** T1.6, T1.7, T1.14, T1.15.

**Acceptance criteria.**

1. A model in `explain` mode attempting a mutation receives a typed `OPERATION_NOT_PERMITTED` error and an audit row records the attempt.
2. Revoking a model token immediately rejects the next request with `TOKEN_REVOKED`.
3. A prompt-injection probe (a hostname containing instruction-like text) does not cause the model wrapper to take any action beyond returning the requested information.

### 6.4 T2.4 — Language-model "propose" mode

**User story.** As an operator, I want a language model to draft mutations as proposals I can approve, so that I take advantage of model assistance without surrendering apply authority.

**Functional requirements.**

1. The tool gateway MUST expose a `propose_mutation` operation in `propose` mode that creates a proposal record without applying.
2. Proposals generated by language models MUST appear in the same UI queue as Docker-discovered proposals (T2.1).
3. Approving a proposal MUST require an authenticated user action; the model MUST NOT be able to approve its own proposal or any other proposal.
4. Proposals MUST expire after a configurable window (default 24 hours); expired proposals MUST be marked expired and MUST NOT be approvable.
5. A proposal that would violate an attached policy preset (T2.2) MUST be rejected at validation, before reaching the queue.

**Non-functional requirements.**

- The proposal queue MUST surface the model identity and the originating prompt for each proposal.
- The audit log MUST record the proposal creation, the approval (or rejection), and the resulting apply (if any) under a single correlation identifier.

**Dependencies.** T1.6, T1.7, T1.14, T2.2, T2.3.

**Acceptance criteria.**

1. A model-issued mutation never reaches Caddy without an authenticated user approval; an integration test asserts this for the full mutation surface.
2. An expired proposal returns `PROPOSAL_EXPIRED` on approval attempt.
3. A proposal violating an attached policy is rejected at validation with `POLICY_VIOLATION` and the violated preset identifier.

### 6.5 T2.5 — Access log viewer

**User story.** As an operator, I want a live and historical view of Caddy access logs with structured filters, so that I can answer operational questions without leaving Trilithon.

**Functional requirements.**

1. Trilithon MUST capture Caddy access logs (via Caddy's structured-log output) and persist them to a rolling on-disk store managed by Trilithon.
2. The web UI MUST stream new lines without manual refresh.
3. The viewer MUST support structured filters on host, status code, method, path, source address, and latency bucket.
4. Storage size MUST be configurable; oldest entries MUST be evicted first when the size bound is reached.

**Non-functional requirements.**

- Filters MUST apply in under 200 milliseconds at the 95th percentile against a rolling store of 10 million lines.
- The viewer MUST be paginated; no single network response MUST exceed 5 MiB.

**Dependencies.** T1.13, T2.6.

**Acceptance criteria.**

1. A 10-million-line fixture store returns filtered results in under 200 milliseconds p95.
2. A live tail keeps pace with 1,000 lines per second on a modern laptop without UI stalling.
3. Eviction at the size bound preserves chronological ordering of remaining entries.

### 6.6 T2.6 — Caddy access log explanation

**User story.** As an operator, I want to ask "why did this happen?" of a single access log entry, so that I understand which configuration object handled the request.

**Functional requirements.**

1. Trilithon MUST correlate every access log entry with the route configuration that handled it, the policy attached at the time, any rate-limit or access-control decision recorded by Caddy, and the upstream response code.
2. The explanation MUST be a structured object referencing snapshot identifiers, route identifiers, and policy identifiers; the web UI MUST render this object as a readable narrative.
3. For at least 95% of access log entries, the explanation MUST trace every decision to a specific configuration object.

**Non-functional requirements.**

- An explanation request MUST return in under 1 second at the 95th percentile against a store of 10 million lines.

**Dependencies.** T1.2, T1.7, T2.2, T2.5.

**Acceptance criteria.**

1. For a fixture set of 1,000 access log entries spanning routes, policies, and access decisions, at least 950 produce a complete explanation.
2. Explanations referencing past configuration correctly cite the snapshot active at the time of the request.

### 6.7 T2.7 — Bare-metal systemd deployment path

**User story.** As an operator on a Linux host, I want a systemd unit and an installer that gets Trilithon running against a system-installed Caddy in one command, so that I am operational within a minute.

**Functional requirements.**

1. Trilithon MUST ship a `systemd` unit file that runs the daemon as a dedicated, unprivileged user.
2. The daemon MUST connect to a system-installed Caddy via Unix domain socket by default.
3. Persistent data MUST live in a standard system path (`/var/lib/trilithon` or similar, configurable).
4. The installer MUST be a single command and MUST be idempotent.
5. An uninstaller MUST remove the service unit and the dedicated user, and MUST remove the data directory only with explicit confirmation.

**Non-functional requirements.**

- A fresh Ubuntu 24.04 LTS or Debian 12 system MUST reach a working web UI within 60 seconds of installer completion.
- The installer MUST refuse to overwrite an existing Trilithon installation without an explicit upgrade flag.

**Dependencies.** T1.13, T1.14, T1.15.

**Acceptance criteria.**

1. An automated test on Ubuntu 24.04 LTS and Debian 12 cloud images verifies install-to-web-UI in under 60 seconds.
2. Uninstall without confirmation preserves the data directory; uninstall with confirmation removes it.
3. The systemd unit restarts the daemon on crash with a back-off bounded at 5 minutes.

### 6.8 T2.8 — Two-container Docker Compose deployment path

**User story.** As an operator preferring containers, I want an official Docker Compose file that runs Caddy and Trilithon as two containers sharing a Unix admin socket, so that I deploy with one command.

**Functional requirements.**

1. Trilithon MUST ship an official `docker-compose.yml` running the unmodified official Caddy image and a Trilithon daemon image.
2. The two containers MUST share a Unix admin socket via a Docker volume.
3. Trilithon's SQLite store MUST live on a separate, persistent volume.
4. The Trilithon image MUST be a multi-stage Rust build, distroless or scratch-based, under 50 MB compressed.
5. The Docker socket MUST be mounted only into the Trilithon container, not the Caddy container (hazard H11).

**Non-functional requirements.**

- `docker compose up` on a fresh host MUST produce a working web UI on `http://127.0.0.1:7878` within 30 seconds.
- The image MUST run as a non-root user inside the container.

**Dependencies.** T1.13, T1.14, T2.1.

**Acceptance criteria.**

1. An automated test brings up the Compose stack on a fresh runner and validates the web UI reachability within 30 seconds.
2. The Trilithon image size is reported in CI; a regression above 50 MB compressed fails the build.
3. The Compose file does not mount `docker.sock` into the Caddy container.

### 6.9 T2.9 — Configuration export

**User story.** As an operator, I want to export my desired state in three forms, so that I can use it outside Trilithon if I choose to walk away (hazard H7).

**Functional requirements.**

1. Trilithon MUST support exporting desired state as a Caddy JSON file directly usable by Caddy.
2. Trilithon MUST support exporting desired state as a Caddyfile, on a best-effort, lossy basis with a structured warning naming each lost element.
3. Trilithon MUST support exporting a Trilithon-native bundle including the desired-state JSON, the snapshot history, the audit log, and the encrypted secrets vault metadata.
4. The Trilithon-native bundle MUST be deterministic given identical inputs.
5. The Caddy JSON export, applied to a fresh Caddy, MUST produce identical runtime behaviour to the source Trilithon's running Caddy on a fixture request set.

**Non-functional requirements.**

- An export of 200 routes MUST complete in under 5 seconds at the 95th percentile.

**Dependencies.** T1.1, T1.2, T1.5, T1.7, T1.15.

**Acceptance criteria.**

1. The Caddy JSON export passes a behavioural-equivalence test against the source Caddy.
2. The Caddyfile export round-trips through the import pipeline without semantic loss for fixtures within the documented Caddyfile-supported subset.
3. The Trilithon-native bundle imports into another Trilithon instance via T2.12 and produces an identical desired-state hash.

### 6.10 T2.10 — Concurrency control

**User story.** As an operator working alongside other actors, I want simultaneous mutations to fail loudly rather than silently overwrite each other, so that two engineers (or a human and a model) never lose work to last-write-wins (hazard H8).

**Functional requirements.**

1. Every mutation MUST carry a `config_version` integer. A mutation against a stale version MUST be rejected with a typed `STALE_CONFIG_VERSION` error.
2. The error MUST include the current version, the stale version, and a human-readable resolution path ("rebase your changes onto v123").
3. The conflict-resolution UI in the web UI MUST present the user's pending changes alongside the changes that landed, allowing per-field rebase.
4. The tool gateway MUST surface conflicts in machine-parseable form for language-model agents in `propose` mode.

**Non-functional requirements.**

- Conflict detection MUST add no more than 10 milliseconds p95 to any mutation.

**Dependencies.** T1.1, T1.6, T2.4.

**Acceptance criteria.**

1. Two mutations issued against the same `config_version` produce one success and one `STALE_CONFIG_VERSION`.
2. The web UI conflict-resolution flow allows a per-field rebase and produces a clean apply.
3. A model in `propose` mode receiving a conflict surfaces the conflict in its proposal output.

### 6.11 T2.11 — Wildcard-certificate proposal callout

**User story.** As an operator using a wildcard certificate, I want any new hostname auto-discovered under that wildcard to be flagged as a security event, so that I never publish a hostname by accident (hazard H3).

**Functional requirements.**

1. Trilithon MUST detect when a Docker discovery proposal would route a new hostname under an existing wildcard certificate.
2. The proposal UI MUST surface a visually prominent banner (not a footnote) describing the wildcard match and the existing certificate's coverage.
3. The user MUST explicitly acknowledge the wildcard match before approval; approval without acknowledgement MUST be impossible from the UI and the tool gateway.
4. The acknowledgement, including the actor identifier and timestamp, MUST be recorded in the audit log.

**Non-functional requirements.**

- The acknowledgement control MUST be keyboard-accessible and meet WCAG 2.1 Level AA contrast.

**Dependencies.** T1.7, T1.9, T2.1.

**Acceptance criteria.**

1. A discovery event under a wildcard produces a banner; without it, no banner appears.
2. The approve action is disabled until acknowledgement is recorded.
3. The acknowledgement appears in the audit log with the actor and timestamp.

### 6.12 T2.12 — Backup and restore

**User story.** As an operator, I want to back up Trilithon state and restore it on the same or a different machine, so that disaster recovery is a single documented operation.

**Functional requirements.**

1. Trilithon MUST support creating a full backup including the SQLite database, encryption key material (re-encrypted with a passphrase), and snapshot attachments.
2. Backups MUST be encrypted with a user-chosen passphrase using a memory-hard key derivation (Argon2id) and an authenticated cipher (XChaCha20-Poly1305).
3. Restore MUST validate the backup (passphrase correctness, file integrity, schema compatibility) before overwriting any state.
4. Restore MUST be possible on a different machine and MUST produce an identical desired-state hash.
5. Every backup creation and every restore MUST produce an audit row.

**Non-functional requirements.**

- Backup of 200 routes plus 30 days of audit history MUST complete in under 30 seconds at the 95th percentile.
- Restore MUST be atomic from the perspective of the daemon: a failed restore MUST leave the prior state intact.

**Dependencies.** T1.2, T1.7, T1.15, T2.9.

**Acceptance criteria.**

1. A backup taken on host A and restored on host B produces an identical desired-state hash and a complete audit log.
2. A backup with an incorrect passphrase fails restore with `BACKUP_PASSPHRASE_INVALID` and does not modify state.
3. A backup with a schema version newer than the running daemon fails restore with `BACKUP_SCHEMA_TOO_NEW` and a human-readable upgrade instruction.

## 7. Out of scope for V1

Each item below is explicitly OUT OF SCOPE FOR V1. The paragraph names the V1 hook (typed mutation surface, capability probe, schema column, or other) that keeps the door open for the post-V1 implementation.

### 7.1 T3.1 — Multi-instance fleet management

OUT OF SCOPE FOR V1. A controller-edge model managing many remote Caddy instances is excellent product surface but introduces a tunnel protocol, agent lifecycle, and authentication topology that V1 cannot evaluate honestly without first proving the single-instance experience. The V1 hook is the `caddy_instance_id` column reserved on every configuration object in the schema, hard-coded to `local` in V1; the typed mutation surface and snapshot model already accommodate desired state spanning multiple targets.

### 7.2 T3.2 — Web Application Firewall integration

OUT OF SCOPE FOR V1. Coraza or comparable WAF integration adds a rule-tuning surface and false-positive workflow whose user research has not been done. The V1 hook is the capability probe (T1.11), which already gates optional features, and the policy preset model (T2.2), which is extensible without schema migration.

### 7.3 T3.3 — Rate limiting (enforced)

OUT OF SCOPE FOR V1. Rate-limit policies depend on `caddy-ratelimit`, which is not present in stock Caddy builds. The V1 hook is the policy preset rate-limit slot, which is declared in the schema and no-ops on stock Caddy. When the module is detected, V1 surfaces a documentation link; V2 wires the slot to a typed rate-limit configuration surface.

### 7.4 T3.4 — Forward-auth and OpenID Connect

OUT OF SCOPE FOR V1. Identity-aware route access introduces account integration, token refresh, and identity-provider lifecycle that exceeds V1's scope. The V1 hook is that the policy preset bundle is a versioned, extensible object; adding an `auth` section to existing presets in V2 requires only a preset version bump, not a schema change.

### 7.5 T3.5 — Layer 4 proxying

OUT OF SCOPE FOR V1. TCP and UDP proxying for non-HTTP services has its own configuration shape and operational concerns (especially around health checks and TLS termination) that warrant a dedicated UI surface. The V1 hook is the capability probe, which detects `caddy-l4` if present, and the typed mutation surface, which can accept a new `create_l4_route` mutation in V2 without restructuring.

### 7.6 T3.6 — Bot challenge integration

OUT OF SCOPE FOR V1. Cloudflare Turnstile, hCaptcha, and self-hosted equivalents introduce a third-party dependency surface and a per-route policy decision that V1 does not yet model. The V1 hook is the policy preset bot-challenge slot, which is declared and inert.

### 7.7 T3.7 — GeoIP and identity-aware routing

OUT OF SCOPE FOR V1. Source-country allow/deny and identity-bound route access compose with T3.4 and T3.6 and depend on a database that is not part of the V1 footprint. The V1 hook is again the policy preset model, which is extensible.

### 7.8 T3.8 — Synthetic monitoring

OUT OF SCOPE FOR V1. External probes that exercise routes from the public internet require a probe-runner topology distinct from the V1 single-host design. The V1 hook is the audit log and snapshot history, which a future probe runner can attach to without schema change.

### 7.9 T3.9 — OpenTelemetry export

OUT OF SCOPE FOR V1. OTLP export is a clear V2 candidate but introduces a dependency surface and configuration topology not justified by V1 user need. The V1 hook is that Trilithon's own observability already uses `tracing`, which has a mature OTLP layer; turning it on later is configuration, not code change.

### 7.10 T3.10 — Hot analytical store for access logs

OUT OF SCOPE FOR V1. ClickHouse or DuckDB backends are appropriate when the rolling on-disk store stops scaling, which V1 user load will not reach. The V1 hook is that the access log viewer (T2.5) is fronted by a query interface that can be re-implemented against a different store without UI change.

### 7.11 T3.11 — Plugin marketplace

OUT OF SCOPE FOR V1. A third-party Caddy module marketplace requires a trust model (signature verification, sandboxed permissions) and a distribution channel that V1 cannot deliver responsibly. The V1 hook is the capability probe, which already discovers loaded modules and could be extended to discover installable modules.

### 7.12 T3.12 — Language-model "autopilot" mode

OUT OF SCOPE FOR V1. A model that applies low-risk mutations without human approval requires a policy engine defining "low-risk" per environment. V1 deliberately constrains models to `explain` and `propose` modes (T2.3, T2.4). The V1 hook is that proposals already exist as a first-class object; an autopilot mode is a new approver identity, not a new mutation path.

## 8. Success metrics

Trilithon's V1 success is measured against quantitative targets for representative deployments and qualitative criteria for user trust. Telemetry that produces these metrics is opt-in; default-off is non-negotiable per constraint 14 of the binding prompt.

### 8.1 Quantitative

1. **Time-to-first-route** for a new user: under 10 minutes from package install to first applied route, measured in scripted onboarding tests on Ubuntu 24.04 LTS, Debian 12, and the Docker Compose deployment.
2. **Crash-free session rate** for the daemon: at least 99.9% of daemon-hours are crash-free, measured in CI long-run tests and (where the user opts in) telemetry.
3. **Drift event resolution rate**: at least 95% of drift events are resolved (adopted, reapplied, or reconciled) within 24 hours of detection, measured in opt-in telemetry.
4. **Caddy admin endpoint compromise count**: zero. Any deployment artefact or default that exposes the Caddy admin endpoint to a non-loopback interface is a release blocker.
5. **Apply latency**: end-to-end mutation-to-Caddy-acknowledgement p95 under 2 seconds against a healthy local Caddy with up to 200 routes.
6. **Language-model proposal acceptance rate**: report only. V1 does not target a number; it surfaces the rate per model identity for the operator's own evaluation.

### 8.2 Qualitative

1. After applying a route, the user can articulate, in plain language, why a representative request to that route succeeded or was blocked, using only the access log explanation surface.
2. After a misconfigured apply, the user can roll back the change without consulting external documentation, using only the rollback affordance in the web UI.
3. The user trusts the audit log to answer "who did what, when, with what intent, and what was the result" for every consequential event in the past 30 days.

## 9. Risks and mitigations

The hazard catalogue below reproduces section 7 of the binding prompt verbatim in title and adds a one-paragraph mitigation referencing the Tier 1 and Tier 2 features that address each hazard.

### 9.1 H1 — Caddy admin endpoint exposure

Mitigation. The Trilithon daemon refuses to start when its configured Caddy admin endpoint is non-loopback. T1.1 specifies Unix domain socket or `localhost` connectivity. T2.7 ships a systemd unit that points at a Unix socket by default. T2.8 ships a Docker Compose file that shares a Unix socket between containers and never exposes Caddy's admin port. Any deployment that violates this constraint is a release blocker per success metric 8.1.4.

### 9.2 H2 — Stale-upstream rollback

Mitigation. T1.3 makes preflight non-optional for rollbacks: upstream reachability, certificate validity, container existence, and module availability are all checked before apply. Failures produce structured errors per condition. The user MAY override per condition; each override is recorded in the audit log per T1.7.

### 9.3 H3 — Wildcard-certificate over-match

Mitigation. T2.11 makes the wildcard match a visually prominent banner in the proposal UI (not a footnote), requires explicit acknowledgement before approval, and records the acknowledgement in the audit log. The acknowledgement control is keyboard-accessible and meets accessibility requirements.

### 9.4 H4 — Hot-reload connection eviction

Mitigation. The dual-pane editor (T1.12) and the typed mutation surface (T1.6) preview each apply's effect before commit. The architecture document specifies the connection-drain configuration and the user-facing control to opt into a longer drain window per apply; that control is surfaced through the typed mutation surface and recorded in the audit log per T1.7.

### 9.5 H5 — Capability mismatch

Mitigation. T1.11 rejects any mutation referencing an absent module at desired-state validation, before any Caddy admin call. The probe is rerun on every Caddy reconnect and on process-identity change, so a module added to a running Caddy is picked up automatically.

### 9.6 H6 — Time-zone confusion in audit logs

Mitigation. T1.7 stores all wall-clock timestamps as UTC Unix timestamps and renders them in the viewer's local time zone. The audit log viewer in the web UI labels every timestamp with both the displayed zone and the underlying UTC value on hover.

### 9.7 H7 — Caddyfile escape lock-in

Mitigation. T2.9 supports three exports: Caddy JSON (round-trip-equivalent), Caddyfile (lossy with structured warnings), and a Trilithon-native bundle. The Caddy JSON export is verified against a behavioural-equivalence test on a fixture request set. A user choosing to walk away can do so with a working configuration.

### 9.8 H8 — Concurrent modification

Mitigation. T2.10 makes optimistic concurrency on `config_version` non-optional. Stale mutations receive a typed `STALE_CONFIG_VERSION` error with the current version and a human-readable resolution path. The conflict-resolution UI is reachable from both the web UI and the tool gateway.

### 9.9 H9 — Caddy version skew across snapshots

Mitigation. T1.2 records the Caddy version on every snapshot. T1.3's preflight checks module availability against the current capability probe (T1.11). T2.12's restore validates schema compatibility before overwriting state. A restore across Caddy major versions warns rather than blocks; the user may proceed at their discretion with the warning recorded in the audit log.

### 9.10 H10 — Secrets in audit diffs

Mitigation. T1.7 mandates a secrets-aware redactor between the diff engine and the audit log writer. T1.15 marks secret fields in the schema and replaces their values with stable opaque tokens before audit write. No code path may bypass the redactor; a static check in `just check` enforces this.

### 9.11 H11 — Docker socket trust boundary

Mitigation. T2.1 grants the Docker socket only to the Trilithon container, never to the Caddy container, in T2.8's Compose file. First-run with discovery enabled emits a stark warning explaining that mounting the socket grants effective root on the host. The warning is also recorded in the audit log.

### 9.12 H12 — Multi-instance leak via fat-finger

Mitigation. The Trilithon daemon writes a sentinel `@id: "trilithon-owner"` object to Caddy's configuration on adoption. On startup, if the sentinel is present and identifies a different Trilithon installation, the daemon refuses to proceed without explicit takeover confirmation. The takeover, when confirmed, is recorded in the audit log per T1.7.

### 9.13 H13 — Bootstrap account credential leak

Mitigation. T1.14 writes bootstrap credentials to a permission-restricted file (`0600`) outside the SQLite database, never in process arguments, environment variables, or logs. The user is prompted to change the password on first login, after which the file is removed.

### 9.14 H14 — Database corruption

Mitigation. The architecture document mandates SQLite Write-Ahead Log mode, periodic `PRAGMA integrity_check`, and a documented recovery path. T2.12 supports backup and restore with cryptographic integrity. The audit log writer uses synchronous writes such that a successful mutation response implies the audit row has reached `fsync`-confirmed disk per T1.7.

### 9.15 H15 — Configuration import that hangs the proxy

Mitigation. T1.5 size-bounds imports (default 10 MiB or 50,000 directives), validates through the same pipeline as user mutations, and rejects pathological inputs before any Caddy admin call. The bound is configurable for documented escape hatches; the configured value is recorded in the audit log on every import.

### 9.16 H16 — Language-model prompt injection through user data

Mitigation. T2.3 mandates a tool-gateway system message clarifying that user data is data, not instruction, and refuses to act on instruction-like content found in logs. T2.4 keeps the model in `propose` mode with no apply authority. The user can revoke a model's access in one click.

### 9.17 H17 — Apply-time TLS provisioning

Mitigation. T1.9 surfaces "issuing certificate" as a distinct status from "applied" and surfaces ACME error messages with actionable text and a retry action. T1.8 reports the per-route status separately so the user is not misled into believing a route is fully serving when its certificate is still being issued.

## 10. Open questions

These questions are tracked, not decided. Resolution is expected during V1 implementation; each question should be answered through an ADR.

1. **Telemetry opt-in default copy.** What exact wording does the first-run experience use to describe opt-in telemetry, and what does telemetry transmit (versions, counters, anonymised feature use)? The default is off; the question is what the prompt says when the user opens the toggle.
2. **Multi-user vs single-user UX in V1.** V1 supports local user accounts (T1.14). Does the V1 web UI surface a "users and tokens" administration page for the multi-user case, or does it default to single-user UI with a hidden multi-user mode? The constraint set permits either; the question is the default presentation.
3. **Exact CSP defaults per preset.** RESOLVED. Resolved at Phase 18; see policy preset definitions in `docs/phases/phased-plan.md` Phase 18 and `docs/todo/phase-18-policy-presets.md` for the literal directive set per preset.
4. **Tauri auto-update channel for V1.1.** The desktop wrap is V1.1, not V1, but the auto-update channel choice (stable / beta / per-arch) influences V1 distribution decisions for the web UI's bundled assets. A design note is wanted before V1.0 release.
5. **Windows support priority.** V1 ships systemd (T2.7) and Docker Compose (T2.8) deployment paths. A Windows native deployment (Windows Service plus a Caddy installation path) is feasible but unscoped. The question is whether Windows ships in V1.0, V1.1, or V1.2.
6. **Per-route override of policy preset fields.** T2.2 allows attaching a preset; the question is whether V1 surfaces a per-route override of individual preset fields (with an audited "diverged from preset" flag) or whether overrides require detaching the preset entirely. The architecture document should resolve this before T2.2 implementation begins.
7. **Audit log export format for compliance.** T2.9 exports the audit log inside the Trilithon-native bundle, but compliance use cases may want a flat export (CSV, JSON Lines) consumable by external tooling. Whether this is V1 scope or post-V1 is open.
8. **Language-model token scoping granularity.** T1.14 specifies named, scoped tokens. The question is whether scopes in V1 are a fixed set (`explain`, `propose`, `read-only`) or a free-form capability list. The fixed set is simpler; the free-form list is more powerful and harder to audit.
