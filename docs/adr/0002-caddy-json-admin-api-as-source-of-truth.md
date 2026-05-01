# ADR-0002: Treat Caddy's JSON Admin API as the source of truth

## Status

Accepted — 2026-04-30.

## Context

Caddy accepts configuration through three surfaces: the JSON Admin API
(`POST /load`, `PATCH /config/...`, `GET /config/`), the Caddyfile
(loaded once at startup or via `caddy reload`), and adapter conversions
between the two. A control plane must commit to one canonical
representation; round-tripping between formats produces lossy diffs,
spurious drift, and unreviewable changes.

The binding prompt fixes this choice (section 2, item 2): Caddy's JSON
Admin API is the source of truth, and the Caddyfile is accepted only as
a one-way import. Tier 1 feature T1.5 specifies the Caddyfile import
behaviour. Tier 2 feature T2.9 permits a best-effort, lossy export to
Caddyfile for users who choose to walk away (hazard H7), but no
internal code path round-trips through Caddyfile.

Forces:

1. **Caddyfile is lossy.** Comments, ordering, and adapter-specific
   shorthands do not survive a round trip through Caddy's adapter.
   Trilithon's snapshot model (T1.2) relies on byte-stable canonical
   serialisation; lossy formats break content addressing.
2. **JSON is structurally complete.** Every Caddy module that has a
   Caddyfile expression also has a JSON expression. The reverse is not
   true: some module configurations have no Caddyfile shorthand.
3. **The mutation API operates on JSON.** `PATCH /config/...` accepts a
   JSON document at a JSON pointer. There is no Caddyfile equivalent.
4. **Drift detection compares running state to desired state (T1.4).**
   Caddy's `GET /config/` returns JSON. Comparing JSON to JSON is
   straightforward; comparing JSON to Caddyfile is not.
5. **The hazard register notes (H7)** that users must be able to walk
   away with a working configuration. A best-effort Caddyfile export
   satisfies this without contradicting the canonical-JSON rule.

## Decision

Trilithon's canonical desired-state representation SHALL be Caddy JSON
as defined by Caddy 2.8 or later. All snapshots (T1.2), mutations
(T1.6), diffs, and applies (T1.1) SHALL operate on JSON.

Caddyfile input SHALL be accepted exactly once per import (T1.5),
converted to JSON via Caddy's `caddy adapt` adapter or an equivalent
in-process call, and the original Caddyfile bytes SHALL be retained as
an attachment on the resulting import snapshot. After import, the
Caddyfile SHALL NOT be consulted, edited, or written by Trilithon.

Trilithon MUST NOT contain any code path that converts JSON back to
Caddyfile and then re-reads the result. The export feature (T2.9) MAY
emit a Caddyfile for the user's benefit, with explicit warnings that the
emission is best-effort and lossy, but the export MUST NOT be used as
input to any subsequent Trilithon operation.

The dual-pane editor (T1.12) MAY render desired state in a
Caddyfile-style legible form on the left pane for human comprehension,
but the underlying model MUST remain JSON, and validation MUST occur
against the JSON schema, not the Caddyfile grammar.

## Consequences

**Positive.**

- Snapshot content addressing is well-defined: canonical JSON
  serialisation produces a stable SHA-256 (T1.2).
- Drift detection is a pure JSON diff against `GET /config/`, with no
  format-translation step that could mask differences (T1.4, hazard H1
  is moot here because the comparison happens on Trilithon's loopback
  channel to Caddy).
- The mutation API (T1.6) maps directly onto `PATCH /config/...`
  semantics with no impedance mismatch.
- The export-to-Caddyfile escape hatch (T2.9) is honest about its
  loss, which respects users without creating internal lock-in
  (hazard H7).

**Negative.**

- Users who think in Caddyfile must learn that the dual-pane editor's
  left pane is a rendering, not a source. Documentation must address
  this directly.
- A Caddyfile feature that has no JSON adapter representation cannot
  be imported. Imports that lose information must surface a
  structured warning (T1.5 acceptance criterion).
- A user who edits Caddyfile manually outside Trilithon will see drift
  on the next reconciliation cycle. This is correct behaviour but may
  surprise users migrating from Caddyfile-centric workflows.

**Neutral.**

- Backups (T2.12) and exports (T2.9) include the JSON form as the
  canonical artefact. The Caddyfile export is a convenience.
- Trilithon's documentation SHALL refer to "Caddy JSON" rather than
  "Caddy configuration" in any context where the format matters.

## Alternatives considered

**Caddyfile as canonical, JSON as derived.** Render the user's intent
in Caddyfile, write Caddyfile to disk, and call `caddy adapt` for the
admin API. Rejected because the Caddyfile is lossy, because content
addressing on a lossy format produces unstable snapshot identifiers,
and because not every Caddy module has a Caddyfile expression.

**Both formats as canonical, kept in sync.** Maintain JSON and
Caddyfile in parallel and reconcile on every mutation. Rejected
because the reconciliation logic is the same as round-tripping (the
prohibited operation) and because two sources of truth produce
deterministic divergence over time.

**A Trilithon-native intermediate format.** Define a third format,
canonical to Trilithon, and translate to and from Caddy JSON at the
edges. Rejected because it duplicates Caddy's schema work, locks
Trilithon out of new Caddy modules until the intermediate format
catches up, and provides no benefit over operating directly on Caddy
JSON.

**Read-only JSON, write-only Caddyfile.** Read state via the admin
API but apply changes by writing a Caddyfile and triggering a reload.
Rejected because `POST /load` and `PATCH /config/...` are atomic and
queryable; Caddyfile reload is neither.

## References

- Binding prompt: `../prompts/PROMPT-spec-generation.md#2-non-negotiable-constraints`,
  item 2; section 4 features T1.1, T1.2, T1.4, T1.5, T1.6, T1.12;
  section 7 hazard H7.
- ADR-0001 (Caddy as the only supported reverse proxy).
- ADR-0009 (Immutable content-addressed snapshots and audit log).
- Caddy documentation: "JSON Config Structure" and `caddy adapt`,
  Caddy 2.8.
