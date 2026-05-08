# Foundation 2 — Proposed Seams (Staging)

`/tag-phase` writes proposed seams here when a phase introduces a genuinely new architectural boundary. Entries are ratified by `/phase-merge-review` before merge — at that point they are moved into `seams.md` and removed from this file.

## Rules

- A phase cannot merge while its proposed seams are still here.
- `/phase-merge-review` either ratifies (move to `seams.md`) or rejects (writes a finding, phase must rework).
- This file is normally empty.

## Format

Same schema as `seams.md`, plus:

```yaml
proposed_seams:
  - id: <slug>
    name: "<name>"
    contracts: [...]
    test_file: tests/cross_phase/<slug>.rs
    proposed_in_phase: <N>
    proposed_at: <ISO-8601>
    proposed_by: <user>
    rationale: "<why this is genuinely new>"
```

## Proposed

```yaml
proposed_seams: []
```
