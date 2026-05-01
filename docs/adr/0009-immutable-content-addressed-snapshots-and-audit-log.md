# ADR-0009: Make snapshots and the audit log immutable and content-addressed

## Status

Accepted — 2026-04-30.

## Context

Two of Trilithon's foundational features are snapshot history (T1.2)
and the audit log (T1.7). Both are referenced by rollback (T1.3),
drift detection (T1.4), language-model interactions (T2.3, T2.4,
ADR-0008), and the export/backup features (T2.9, T2.12). Together
they are the historical record on which every higher-order feature
depends.

A historical record that can be rewritten silently is not a record.
The integrity property the rest of Trilithon needs is: once a
snapshot or audit row is written, it is not modified, not deleted in
place, and not silently re-derived. Rollback is the application of an
existing snapshot, not a synthesised approximation.

The binding prompt formalises the rule:

- T1.2: "Snapshots are immutable. There is no `UPDATE snapshots`
  statement anywhere in the codebase."
- T1.7: "Audit rows are immutable. There is no `UPDATE audit_log`
  statement."
- T1.7 and T1.15: "Audit rows MUST NOT contain plaintext secrets. A
  secrets-aware redactor sits between the diff engine and the audit
  log writer."
- Hazard H10: "Specifications MUST NOT propose any code path that
  bypasses the redactor."

Forces:

1. **Content addressing produces stable identifiers.** A snapshot's
   identifier is the SHA-256 of its canonical JSON serialisation
   (T1.2). Identical snapshots deduplicate by identifier. References
   to snapshots from audit rows, parent pointers, and rollback
   targets remain valid as long as the bytes are reachable.
2. **Append-only persistence simplifies recovery.** Hazard H14 names
   SQLite corruption as a real concern. An append-only log can be
   replayed in order and validated; an updateable log invites
   "last-known-good" guessing during recovery.
3. **The redactor is non-bypassable by construction.** Putting the
   redactor between the diff engine and the audit log writer means
   no caller can produce an unredacted audit row. The interface to
   the audit log writer SHALL accept already-redacted input only.
4. **Cryptographic integrity is cheap.** A SHA-256 hash chain across
   audit rows allows offline verification of log integrity. V1 does
   not require external attestation, but the hash chain costs little
   to add.

## Decision

**Snapshot identifiers.** A snapshot's identifier SHALL be the SHA-256
hex digest of its canonical JSON serialisation. Canonical
serialisation SHALL sort object keys lexicographically, SHALL produce
no whitespace between tokens, and SHALL emit numbers in their shortest
unambiguous form. The serialisation function SHALL live in
`crates/core` and SHALL be deterministic.

**Snapshot immutability.** The schema SHALL declare the `snapshots`
table without an `UPDATE`-able primary key. Application code SHALL
NOT issue `UPDATE` or `DELETE` against the `snapshots` table. A
snapshot row that already exists SHALL NOT be re-inserted; identical
snapshots deduplicate by identifier (T1.2).

**Snapshot fields.** Each snapshot SHALL record: the content-addressed
identifier, the parent identifier (nullable for the root), the actor
(user identifier or language-model session identifier), the intent
(free-text rationale), the correlation identifier (a ULID), the Caddy
version at apply time, the Trilithon version, the wall-clock UTC Unix
timestamp, the monotonic timestamp at creation, and the canonical
desired-state JSON.

**Audit log immutability.** Every mutation, apply, rollback, drift
event, language-model interaction, and authentication event SHALL
write exactly one row to the audit log. Application code SHALL NOT
issue `UPDATE` or `DELETE` against the `audit_log` table.

**Audit log integrity chain.** Each audit row SHALL include a
`prev_hash` column containing the SHA-256 of the previous row's
canonical serialisation (or the all-zero digest for the first row).
A verification routine SHALL be available through `crates/core` to
walk the chain and detect tampering. The chain check SHALL run on
daemon startup and SHALL log the result through the tracing
subscriber.

**Redactor placement.** The secrets-aware redactor SHALL be
positioned between the diff engine and the audit log writer. The
audit log writer SHALL accept only redacted input; a type-level
distinction (a `RedactedDiff` newtype in `crates/core`) SHALL prevent
unredacted diffs from reaching the writer. This is the structural
implementation of hazard H10's "MUST NOT propose any code path that
bypasses the redactor."

**Time fields.** Wall-clock timestamps SHALL be stored as UTC Unix
timestamps in seconds (with a separate sub-second column where
ordering matters). Display of these timestamps SHALL convert to the
viewer's local time zone (hazard H6). Snapshots and audit rows SHALL
record both the wall-clock timestamp and a monotonic timestamp; the
monotonic timestamp orders events within a single daemon run, the
wall-clock timestamp orders events across runs.

**Rollback semantics.** Rollback (T1.3) SHALL set the desired-state
pointer to an existing snapshot. Rollback SHALL NOT create a new
snapshot row through copy; it SHALL emit a new audit row recording
the pointer change with reference to the existing snapshot
identifier.

## Consequences

**Positive.**

- The historical record is verifiable. A user (or an auditor)
  can compare hash-chain endpoints across backups to confirm that
  the log has not been tampered with.
- Content-addressed snapshots deduplicate naturally. A no-op
  mutation (apply the current desired state again) produces no
  new snapshot row, only an apply audit entry.
- The redactor cannot be accidentally bypassed. The type system
  in `crates/core` makes "write an unredacted diff to audit_log" a
  compile error.

**Negative.**

- The SQLite database grows monotonically. T2.5 access logs
  rotate; snapshots and audit rows do not. Operationally, T2.12
  backup-and-restore is the long-term answer; the architecture
  document carries the storage budget.
- A mistake captured in a snapshot or audit row cannot be edited
  away. A leaked secret that escapes the redactor SHALL be
  handled by re-keying (ADR-0014) and recording the incident in a
  new audit row, not by rewriting history. This is correct
  behaviour, but it requires an incident-response procedure.
- Large desired-state documents produce large snapshot rows.
  Compression (zstd at the storage layer) is a follow-up
  optimisation, not a V1 deliverable.

**Neutral.**

- The hash chain is a transparency property, not an authentication
  property. It detects tampering by anyone who has the database
  but does not bind the log to a specific actor. Cryptographic
  signing of audit rows (with a key in the keychain or an
  external HSM) is a Tier 3 sketch.
- Open question for the architecture document: how often the
  daemon re-runs the chain check and what happens when it fails
  in production. Recorded as an open question in the architecture
  document, not in this ADR.

## Alternatives considered

**Mutable history with an `is_deleted` flag.** Allow soft-deletes on
snapshots and audit rows. Rejected because soft-delete is mutable
state in disguise; the writer code path that flips the flag is
exactly the path that can lose data through bugs.

**Sequence-number identifiers instead of content addresses.** Use
auto-incrementing primary keys for snapshots. Rejected because
content addressing deduplicates identical snapshots and because the
hash digest is also a deterministic check that the snapshot's bytes
are what they claim to be.

**External append-only log (Kafka, Loki, an event store).** Send
audit rows to an external log. Rejected for V1 because constraint
14 makes the user sovereign over their data; an external log
contradicts the local-first ethos. Tier 3 OpenTelemetry export
(T3.9) is the future place for opt-in external observability.

**Periodic compaction of snapshot history.** Drop snapshots older
than N days to bound storage. Rejected because rollback is one of
the foundational features (T1.3) and compaction defeats it; T2.12
backup-and-restore is the right knob for storage growth, not
compaction.

## References

- Binding prompt: `../prompts/PROMPT-spec-generation.md#4-tier-1`,
  features T1.1, T1.2, T1.3, T1.4, T1.7, T1.15; section 5 features
  T2.9, T2.12; section 7 hazards H6, H10, H14.
- ADR-0006 (SQLite as V1 persistence layer).
- ADR-0008 (Bounded typed tool gateway for language models).
- ADR-0014 (Secrets encrypted at rest with keychain master key).
