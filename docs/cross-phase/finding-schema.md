# Foundation 0 — Finding Schema

Every finding produced by any skill (review-collect, review-aggregate, project-audit, phase-merge-review, security-audit, plan-adversarial, code-refresh) carries this YAML frontmatter at the top of its markdown file. One finding per file. File name: `<id>.md` with `:` replaced by `__`.

## Schema

```yaml
---
id: <category>:<location>:<finding_kind>          # required, identity-stable across runs
category: duplicate | reuse-miss | abstraction | layer-leak | dead-code | terminology |
          cross-cutting | contract-drift | seam-coverage | security | scope | logic
kind: structural | architectural | process | cross-cutting
location:
  file: src/foo/bar.rs        # required when kind=structural
  symbol: foo::bar::Baz       # required when kind=structural
  area: <slug>                # required when kind != structural
  multi: false
locations:                    # optional, only when location.multi=true
  - file: src/x.rs
    symbol: x::y
finding_kind: <slug>          # from finding-kinds.yaml — fixed vocabulary
phase_introduced: 12 | unknown
status: open | accepted-as-is | superseded | fixed | pending-revalidation
status_reason: <text>          # required if status != open
accepted_by: <user>            # required if status=accepted-as-is
accepted_at: <ISO-8601>
expires: <YYYY-MM-DD>          # required if status=accepted-as-is
last_verified_at: <commit-sha>
do_not_autofix: false
created_at: <ISO-8601>
created_by: <skill-name>       # which skill produced it
severity: critical | high | medium | low | info
tags: [optional, free-form]
linked_to: []                  # ids of related findings (e.g. rename target)
---

# <Title>

## Description
<plain-prose description of the issue>

## Evidence
<file paths, code excerpts, output snippets>

## Recommendation
<what to do — or `none — review-only` if do_not_autofix=true>

## Resolution log
<append-only — date | actor | action | result>
```

## ID Construction Rules

The ID must be **reproducible from structural facts** of the finding. No hashes of prose. Two LLM runs of the same audit must produce the same ID for the same problem.

```
id = <category>:<location_part>:<finding_kind>

location_part:
  if kind == structural:
    "<file_path>::<symbol>"        # e.g. "src/auth.rs::verify_token"
  else:
    "area::<area_slug>"            # e.g. "area::auth-middleware"
  if multi-location:
    "multi::<area_slug>"           # locations[] populated separately
```

Examples:
- `duplicate:src/retry.rs::RetryPolicy:duplicate-retry-policy`
- `reuse-miss:src/parser.rs::parse_token:could-call-existing-tokenizer`
- `cross-cutting:area::api-handlers:auth-middleware-missing`
- `contract-drift:src/auth.rs::verify_token:signature-changed-without-registry-update`

## `finding_kind` Vocabulary

`finding_kind` values come from `finding-kinds.yaml`. Audits proposing a new `finding_kind` must add it to the vocabulary file in the same commit (review gate).

## Lifecycle

| From | To | Triggered by |
|---|---|---|
| (new) | `open` | New finding created by any skill |
| `open` | `fixed` | `/review-remediate` records resolution |
| `open` | `accepted-as-is` | Human triage decision (must include `accepted_by`, `expires`) |
| `open` | `pending-revalidation` | Revalidation can't locate symbol (rename / unclear deletion) |
| `pending-revalidation` | `open` | Human re-anchors finding to new location |
| `pending-revalidation` | `superseded` | Human confirms underlying issue is gone |
| `accepted-as-is` | `open` | `expires` date passed |
| `open` / `pending-revalidation` | `superseded` | `git log --diff-filter=D` confirms symbol deletion at specific commit |
| `fixed` | (terminal) | — |
| `superseded` | (terminal) | — |

## Filtering Rules

- New audit output: filter out `accepted-as-is`, `superseded`, `fixed`. Surface only `open` and `pending-revalidation`.
- `/review-remediate`: skip any finding with `do_not_autofix: true` (treat as "ready for human design decision," don't generate code).
- `/where`: surface `accepted-as-is` separately as "accepted exceptions" with their `expires` dates so they don't disappear from view.
- `/coherence-audit`: validates schema for every finding in `Unfixed/`, `In_Flight_Reviews/Unfixed/`, `End_of_Phase_Reviews/Findings/`.

## Revalidation

Run before any audit (`/project-audit`, `/phase-merge-review`, `/coherence-audit`).

```
for each finding with status in {open, pending-revalidation}:
  cache_key = (id, current_main_sha)
  if cache_hit: continue

  if kind == structural:
    if symbol exists at location.file::location.symbol:
      status = open
      last_verified_at = current_main_sha
    elif git log --diff-filter=D matches symbol:
      status = superseded
      status_reason = "symbol-deleted-at-<sha>"
    elif git log --diff-filter=R matches symbol:
      status = pending-revalidation
      create_draft(linked_to = [original.id], at new symbol location)
    else:
      status = pending-revalidation
      status_reason = "symbol-not-found-cause-unclear"
  else:
    # architectural/process findings — manual revalidation only
    if last_verified_at is older than 5 phases:
      status = pending-revalidation
      status_reason = "stale-non-structural-needs-human-revalidation"
```

Cache stored at `.claude/cache/revalidation.json`, gitignored.

## Storage Locations

The schema is enforced wherever findings live:

- `End_of_Phase_Reviews/Findings/`
- `End_of_Phase_Reviews/Unfixed/`
- `End_of_Phase_Reviews/Fixed/`
- `In_Flight_Reviews/Findings/`
- `In_Flight_Reviews/Unfixed/`
- `docs/audit/project-audit-*.md` (multi-finding files: each finding section gets its own frontmatter sub-block)
- `docs/audit/code-refresh.md` (rolling — each entry has its own ID and status)
- `docs/security/audit-*.md`

## Migration

Existing findings predating this schema are migrated by `xtask migrate-findings` (see `templates/xtasks/migrate-findings.rs`). The migration assigns:
- `phase_introduced: unknown`
- `status: open`
- `created_at: <file mtime>`
- `created_by: legacy-migration`
- `last_verified_at: <baseline_sha>`
- Best-effort extraction of `category`, `kind`, `location`, `finding_kind` from existing prose.

Items the migration cannot categorize get `kind: process`, `category: scope`, `finding_kind: legacy-uncategorized` and are surfaced in `/where` as needing human triage.
