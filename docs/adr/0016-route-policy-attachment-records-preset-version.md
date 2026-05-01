# ADR-0016: Route policy attachments record the preset version they were bound against

## Status

Accepted — 2026-04-30.

## Context

Policy presets (T2.2) are reusable bundles of route configuration. A
preset has a name, a version, and a body. Routes attach to a preset
by reference. The architecture document, before this ADR, modelled
the attachment as a row in `route_policy_attachments` keyed on
`(route_id, preset_id)` with no version column.

Policy preset bodies are not frozen. Operators MUST be able to edit
a preset over time as defaults shift (a stricter `Strict-Transport-Security`
header, a tighter compression list, a new default upstream timeout).
Without versioning the attachment, an edit to a preset body would
silently change the effective configuration of every route already
attached to that preset. That contradicts T2.2's acceptance
criterion that "a preset edit MUST NOT silently alter the effective
configuration of any already-attached route" and contradicts the
principle in ADR-0009 that the historical record (the desired state
a snapshot encodes) is not rewritten under a user.

The attachment row is the join between two independently-versioned
things. It MUST record which version it joined against.

Forces:

1. **Preset edits are routine.** Operators tighten or relax presets
   as their understanding of their environment matures. The product
   intent is that this is a low-friction operation.

2. **Attached routes outlive preset edits.** A route attached today
   may run for years. The configuration the operator approved on
   attachment day is the contract; later edits to the preset body
   are a separate event the operator MUST opt into.

3. **Snapshots already capture point-in-time desired state.** ADR-0009
   makes the rendered Caddy configuration immutable per snapshot.
   But the attachment-graph metadata is what the snapshot writer
   resolves *against* when it renders. If the attachment graph
   silently re-resolves to a new preset body, snapshots taken before
   and after the preset edit diverge for reasons the operator never
   approved.

4. **Two levels of opt-in are needed.** The operator who edits a
   preset opts into a new version. The operator who maintains a
   route opts into upgrading that route's attachment to the new
   version. Conflating the two strips the second consent.

## Decision

`route_policy_attachments` carries a `preset_version` column. The
column is `NOT NULL`. The composite foreign key is to
`policy_preset_versions(preset_id, version)`, the table that holds
each immutable preset version. Editing a preset produces a new
`policy_preset_versions` row; it does not modify any existing row,
and it does not touch any `route_policy_attachments` row.

When the daemon renders the desired state for a route, it reads the
attached `preset_version` and resolves the body from that exact
version's row. A preset edit is therefore invisible to existing
attachments until an operator explicitly upgrades each one.

The web UI surfaces an "upgrade available" affordance per route when
a newer version of an attached preset exists. Upgrading is a normal
mutation: it produces a new snapshot, an audit event of kind
`policy-preset.upgraded`, and goes through the standard apply path.
Detaching uses `policy-preset.detached`; attaching uses
`policy-preset.attached`. Each carries the `preset_version` involved.

The schema appears in `architecture.md` §6.12. The index
`idx_rpa_preset_version` on `(preset_id, preset_version)` exists so
the upgrade-prompt path can answer "which routes are still on
version N of this preset?" cheaply.

## Consequences

- A preset edit is a metadata operation; it never causes a Caddy
  reload by itself. Reload happens only on explicit per-route
  upgrade. This satisfies T2.2 and aligns with ADR-0009's principle
  that the historical record is not rewritten retroactively.
- Operators see an explicit upgrade list per preset edit. This adds
  a step compared to silent propagation; that is the intended
  trade-off.
- The storage model has one extra integer per attachment row. The
  index on `(preset_id, preset_version)` is small (preset count is
  bounded; route count per preset is bounded by deployment size).
- Deletion of a `policy_preset_versions` row is forbidden while any
  attachment references it. Old preset versions are retained
  indefinitely; pruning is OUT OF SCOPE FOR V1, matching the
  retention stance for snapshots and audit rows.

## Alternatives considered

- **Keep the attachment unversioned, and propagate preset edits.**
  Rejected: violates T2.2 and ADR-0009. Operators lose the consent
  step on every route at once.
- **Snapshot-only resolution, no version column on the attachment.**
  Rejected: snapshots resolve at render time; if the attachment
  table holds no version, render-time resolution either picks
  "latest" (silent propagation) or "first" (silent staleness). Both
  fail the T2.2 acceptance criterion.
- **Copy the preset body into the attachment row.** Rejected:
  duplicates the body across every attached route, defeats the
  point of having a preset abstraction, and complicates the upgrade
  prompt.

## References

- T2.2 (binding PRD): policy preset behaviour and acceptance criterion.
- ADR-0009: immutable, content-addressed snapshots and audit log.
- `architecture.md` §6.11 (`policy_presets` / `policy_preset_versions`)
  and §6.12 (`route_policy_attachments`).
