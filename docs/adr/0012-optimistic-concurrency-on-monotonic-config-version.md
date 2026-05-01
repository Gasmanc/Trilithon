# ADR-0012: Use optimistic concurrency on a monotonic config_version for every mutation

## Status

Accepted — 2026-04-30.

## Context

Trilithon's mutation pipeline serves at least three categories of
actor: a single human operator at the web UI, multiple human operators
at the same web UI from different sessions, and language model agents
acting through the typed tool gateway (ADR-0008). Tier 2 feature
T2.10 names the failure mode directly: when two actors attempt
mutations against the same desired state simultaneously,
last-write-wins is unacceptable. Hazard H8 reinforces the point:
"Two actors mutating the same desired state simultaneously without
optimistic concurrency control produce silent data loss."

The binding prompt's T1.1 acceptance criteria fix the mechanism:
"All applies are wrapped in optimistic concurrency control on a
monotonically increasing `config_version` integer. A stale apply is
rejected with a typed conflict error."

Forces:

1. **The single-writer SQLite model (ADR-0006) does not by itself
   prevent stale-input mutations.** A user can fetch desired-state
   v100, edit a route in the UI, take coffee, and submit the edit
   while another actor has already advanced the state to v105. The
   submitting user's intent was relative to v100; applying it to
   v105 silently overwrites work.
2. **Last-write-wins is not auditable.** The audit log (T1.7)
   records what happened but cannot recover the lost intent. A
   typed conflict surfaces the situation while both intents still
   exist.
3. **Pessimistic locking does not fit the workflow.** A human at
   a UI may take minutes to compose an edit. Holding a lock for
   minutes blocks every other actor and turns transient
   concurrency into a coordination problem.
4. **The conflict resolution surface should be uniform.** The
   typed tool gateway (ADR-0008) and the web UI ride the same
   mutation primitives (T1.6). Both must surface the conflict
   the same way.

## Decision

**The version.** Trilithon SHALL maintain a single
`config_version: u64` integer per Caddy instance (T3.1 reservation:
per `caddy_instance_id`, hard-coded to `local` in V1). The version
SHALL be monotonically increasing. Every successful mutation SHALL
increment the version by exactly one. The version SHALL be persisted
in SQLite in the same transaction as the snapshot insert and the
audit row append.

**Mutation contract.** Every typed mutation (T1.6) SHALL carry an
`expected_version: u64` field. The mutation SHALL be evaluated as
follows:

1. Read the current `config_version` from storage.
2. If `expected_version != current_version`, fail with a typed
   `ConflictError { expected_version, current_version,
   conflicting_snapshot_id }`. Do not advance state. Do not write a
   snapshot. Do write an audit row recording the rejected attempt
   with its actor, intent, correlation identifier, and the conflict
   detail.
3. If versions match, validate the mutation against the current
   desired state, compute the resulting desired state, persist the
   snapshot (ADR-0009), append the audit row, and increment
   `config_version` to `current_version + 1`. The increment, the
   snapshot insert, and the audit append SHALL execute in one
   SQLite transaction.

**Apply contract.** An apply (writing the snapshot to Caddy via
`POST /load` or `PATCH /config/...`) SHALL also be guarded by
`expected_version`. Apply MAY succeed against a higher
`current_version` if the snapshot identifier matches the current
desired-state pointer; this allows reapply of an unchanged state.
Apply against a stale `expected_version` whose snapshot does not
match the current pointer SHALL fail with the same typed
`ConflictError`.

**Web UI behaviour.** When the web UI submits a mutation, it SHALL
include the `config_version` it observed when the user opened the
edit. On `ConflictError`, the UI SHALL display the conflict, show
the diff between the user's intended change and the current desired
state, and present a path forward (the binding prompt names this
"rebase your changes onto v123"). The UI SHALL NOT silently retry
with the new version.

**Tool gateway behaviour.** A language model agent (ADR-0008)
calling a typed mutation SHALL pass `expected_version`. On
`ConflictError`, the gateway SHALL return the typed error to the
agent, which is then responsible for re-fetching state and
resubmitting if appropriate. The gateway SHALL NOT silently retry
on the agent's behalf, because the conflicting state may have
changed the meaning of the proposed mutation.

**Idempotency.** Each mutation SHALL declare its idempotency story
(T1.6). A retry of the same mutation with the same
`expected_version` against an unchanged state SHALL produce the
same outcome (same snapshot identifier through content addressing,
ADR-0009).

**Drift versus conflict.** Drift (T1.4) is a divergence between
desired state and Caddy's running state. Conflict is a divergence
between an actor's `expected_version` and `current_version`. The
two are distinct. A drift event does not change `config_version`;
it produces an audit row of type `drift_detected` and surfaces
remediation choices (adopt, re-apply, or open the editor). Adopting
running state as desired state IS a mutation and DOES advance
`config_version`.

## Consequences

**Positive.**

- Concurrent mutations cannot silently overwrite each other.
  Hazard H8 is addressed by construction.
- The conflict path is the same surface for humans and language
  models. The web UI and the tool gateway both see
  `ConflictError` with the same fields.
- The audit log records both successful and rejected attempts,
  preserving forensic traceability of contested intents.

**Negative.**

- Long-lived UI sessions accumulate stale-version conflicts when
  another actor is busy. The UI must offer a clear rebase
  experience; without one, users may experience the conflict as
  flakiness.
- Bulk operations (apply twenty mutations) must either compose
  into a single typed mutation that takes a single
  `expected_version`, or accept that the second through twentieth
  may conflict if any of them depend on the first having landed.
  The mutation set design (T1.6) SHALL include compound mutations
  where they are useful.
- Cross-restart version persistence is a SQLite contract: a
  database corruption event (hazard H14) that loses
  `config_version` is a recovery scenario the architecture
  document SHALL specify.

**Neutral.**

- The version is per-instance. Multi-instance fleet management
  (T3.1) will introduce per-instance versions; the schema already
  reserves `caddy_instance_id`.
- The `ConflictError` payload includes the conflicting snapshot
  identifier, allowing UIs to render a three-way diff (user's
  base, user's intent, current state) where useful. Whether the
  V1 UI implements three-way diffs is a UX question recorded in
  the architecture document.

## Alternatives considered

**Pessimistic locking.** Acquire a lock when the user begins
editing and release it on save or timeout. Rejected because human
edit sessions are minutes-to-tens-of-minutes long; locks of that
duration block every other actor. The model also fails open for
language model agents that cannot meaningfully "hold a lock."

**Last-write-wins with audit.** Allow concurrent writes and
rely on the audit log to surface what happened. Rejected because
hazard H8 names this as "silent data loss" and because the audit
log is forensic, not preventive.

**Three-way merge on every mutation.** Detect overlapping changes
and attempt automatic merge. Rejected for V1 because automatic
merge of configuration objects is unsafe (a route's order matters,
a policy's ruleset is order-dependent), and because surfacing the
conflict to a human is the right answer for V1 scale.

**Compare-and-swap on full configuration JSON instead of integer
version.** Pass the previous configuration's content hash as the
guard. Rejected because the integer version is cheaper to compare
and easier to display to the user ("you are working from v122,
current is v125"); the snapshot's content hash already exists in
ADR-0009 and is included in the conflict payload.

## References

- Binding prompt: `../prompts/PROMPT-spec-generation.md#4-tier-1`,
  feature T1.1 acceptance; section 5 feature T2.10; section 7
  hazard H8.
- ADR-0006 (SQLite as V1 persistence layer).
- ADR-0008 (Bounded typed tool gateway for language models).
- ADR-0009 (Immutable content-addressed snapshots and audit log).
