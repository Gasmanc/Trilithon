# ADR-0007: Surface Docker discovery as proposals, never auto-applied

## Status

Accepted — 2026-04-30.

## Context

Container-aware reverse proxies (Traefik most prominently) auto-apply
configuration changes derived from container labels: a container starts
with a `traefik.http.routers.foo.rule=Host(\`example.com\`)` label and
the proxy starts serving `example.com` to that container's port within
seconds. The model is convenient and has shipped many production
deployments.

It is also a class of foot-gun. A misconfigured container, a
copy-pasted compose file, or a malicious image with the right labels
can publish a hostname through the proxy without human review. The
hazard register documents this directly: hazard H3 (wildcard
certificate over-match), hazard H11 (Docker socket trust boundary), and
hazard H12 (multi-instance leak via fat-finger) all worsen when label
detection becomes silent application.

Trilithon's binding prompt encodes the safer model. Tier 2 feature T2.1
specifies that Docker container discovery emits **proposals**, not
applied configurations. The user (or, where policy permits, a language
model in propose mode under T2.4) approves or rejects each proposal.
Wildcard-certificate matches are flagged as security events (T2.11).

Forces:

1. **Auditability.** Every change to running configuration must be
   traceable to an actor with intent (T1.7). A label changed by
   another team's container does not have an actor in the Trilithon
   sense.
2. **Wildcard exposure.** A container labelled `subdomain.example.com`
   that matches an existing `*.example.com` certificate publishes
   that certificate's coverage to a new endpoint silently. Hazard
   H3 names this; T2.11 requires explicit acknowledgement.
3. **The Docker socket is privileged.** Mounting the Docker socket
   into a container grants effective root on the host (hazard H11).
   Trusting label content from arbitrary containers is the path that
   converts that grant from a deployment concern to an exploitation
   vector.
4. **Drift detection assumes one source of truth.** If Docker labels
   were a second writer to desired state, every container restart
   would manifest as drift, breaking T1.4's signal-to-noise ratio.
5. **Operator agency.** The product owner has explicit goals that
   the human (or an authorised agent) remain in the approval loop
   for configuration changes. The proposal model encodes this
   without ruling out automation later (T3.12 autopilot mode is the
   future place for opt-in automated approval, post-V1).

## Decision

Trilithon's Docker discovery subsystem SHALL emit proposals, not
mutations. A discovered configuration SHALL appear in a proposal queue
and SHALL NOT modify desired state until an authenticated actor
approves it.

The discovery loop SHALL behave as follows:

- **Container start with valid Caddy labels.** Trilithon SHALL emit
  a proposal within five seconds (T2.1 acceptance criterion). The
  proposal SHALL record the container identifier, the labels read,
  the resulting typed mutation, the discovery time, and the
  correlation identifier.
- **Container destruction.** Trilithon SHALL emit a "remove route"
  proposal. Trilithon SHALL NOT silently remove a route on container
  loss; the user may have reasons (planned restart, failover) the
  control plane cannot infer.
- **Label conflict.** Two containers claiming the same hostname
  SHALL produce a single conflict proposal naming both candidates
  (T2.1 acceptance criterion). Trilithon SHALL NOT emit two
  competing proposals.
- **Wildcard certificate match.** A proposal that would route a new
  hostname under an existing wildcard certificate SHALL carry a
  prominent security callout (T2.11). The callout SHALL require
  explicit acknowledgement before approval. The acknowledgement
  SHALL be recorded in the audit log (hazard H3).

Proposals SHALL expire after a configurable window (default 24 hours
per T2.4). Expired proposals SHALL NOT be auto-applied.

The Docker socket SHALL be accessible only to the Trilithon daemon
container (hazard H11, ADR-0010). The daemon SHALL emit a stark
first-run warning explaining the trust grant, per hazard H11.

Trilithon SHALL NOT include a configuration flag, a hidden command-line
option, or an environment variable that switches Docker discovery from
proposal mode to auto-apply mode. The proposal model is non-negotiable
for V1. Any future automated-approval path SHALL land through T3.12
under its own ADR.

## Consequences

**Positive.**

- A misconfigured or malicious container cannot publish a hostname
  without human (or authorised language model) approval.
- The audit log carries an explicit approval record for every
  Docker-derived change, satisfying T1.7's "who did what, when,
  with what intent" requirement.
- The proposal queue surface is shared with language-model proposals
  (T2.4). Discovery and language-model agents converge on the same
  approval UI, reducing the human's mental model.
- Wildcard exposure is treated as the security event it is (T2.11).

**Negative.**

- Trilithon is not drop-in for users migrating from Traefik who
  expect labels to be authoritative. The migration story SHALL
  document the difference clearly.
- A user running a development environment with frequent container
  churn will see proposal queue churn. The expiry window mitigates
  this; bulk-approval UX may need iteration in Tier 2.
- A label change that the user wants to take effect immediately
  requires a click. This is the cost of correctness; the product
  owner has accepted it.

**Neutral.**

- Podman support uses the same code path as Docker; both expose a
  Docker-API-compatible socket. The discovery subsystem SHALL
  abstract over both behind the `adapters` layer (ADR-0003).
- Auto-apply for narrowly defined low-risk classes is preserved as
  a Tier 3 sketch (T3.12). V1 architecture does not preclude it
  but does not implement it.

## Alternatives considered

**Auto-apply with audit log.** Auto-apply Docker labels and rely on
the audit log to surface what happened. Rejected because the audit
log is a forensic tool, not a control surface; by the time the user
sees the entry, the misconfiguration is already serving traffic.

**Auto-apply for "trusted" containers, propose for others.** Define
a trust list (image signature, label namespace, source registry) and
auto-apply for trusted containers. Rejected because the trust list is
itself a configuration surface that requires human approval, and
because the V1 product owner's stated preference is human-in-the-loop
for all configuration changes.

**Reject Docker discovery entirely for V1.** Defer container
discovery to V2 and require users to write routes by hand. Rejected
because container-aware operation is a real V1 use case (the home-lab
target audience runs containers), and because the proposal model
delivers the value with the safety property.

**Pull-based discovery (user clicks "scan now").** Skip the
continuous Docker watch and require an explicit user action.
Rejected because container churn is naturally event-driven and the
proposal queue is well-shaped to absorb the events; pull-based
scanning would surface stale information.

## References

- Binding prompt: `../prompts/PROMPT-spec-generation.md#5-tier-2`,
  features T2.1, T2.4, T2.11; section 7 hazards H3, H11, H12;
  section 6 feature T3.12.
- ADR-0008 (Bounded typed tool gateway for language models).
- ADR-0010 (Two-container deployment with unmodified official Caddy).
- ADR-0015 (Instance ownership sentinel in Caddy config).
