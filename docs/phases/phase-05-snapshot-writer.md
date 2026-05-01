# Phase 5 — Snapshot writer and content addressing

Source of truth: [`../phases/phased-plan.md#phase-5--snapshot-writer-and-content-addressing`](../phases/phased-plan.md#phase-5--snapshot-writer-and-content-addressing).

## Pre-flight checklist

- [ ] Phase 2 complete (storage available).
- [ ] Phase 4 complete (mutation algebra defined).

## Tasks

### Backend / core crate

- [ ] **Implement the canonical JSON serialiser for `DesiredState`.**
  - Acceptance: The serialiser MUST sort map keys lexicographically, MUST normalise numeric representation, and MUST produce byte-identical output for byte-identical desired states.
  - Done when: `cargo test -p trilithon-core canonical_json::tests::byte_identical` passes for 50 fixture states.
  - Feature: T1.2.
- [ ] **Version the canonicalisation format.**
  - Acceptance: The serialiser MUST expose a `CANONICAL_JSON_VERSION` integer, and every snapshot row MUST record the version used.
  - Done when: a unit test asserts the constant and an integration test asserts the persisted column.
  - Feature: T1.2.
- [ ] **Define the `Snapshot` record type.**
  - Acceptance: `Snapshot` MUST carry `snapshot_id` (SHA-256 hex of canonical JSON), `parent_id`, `config_version`, `actor`, `intent` (length-bounded at 4 KiB), `correlation_id` (ULID), `caddy_version`, `trilithon_version`, `created_at_unix_seconds`, `created_at_monotonic_nanos`, and `desired_state_json`.
  - Done when: the type compiles and a unit test asserts the byte layout of every field.
  - Feature: T1.2.
- [ ] **Implement `MutationId` content-addressing helper.**
  - Acceptance: A `content_address` helper MUST hash the canonical JSON via SHA-256 and return a hex-encoded string.
  - Done when: `cargo test -p trilithon-core mutation::tests::content_address_is_stable` passes.
  - Feature: T1.2.

### Backend / adapters crate

- [ ] **Implement the `SnapshotWriter` adapter.**
  - Acceptance: `SnapshotWriter` MUST compute the canonical hash, deduplicate against existing rows, enforce parent linkage (parent MUST exist), enforce strict monotonic increase of `config_version`, and persist the row in a single SQLite transaction.
  - Done when: integration tests cover deduplication, parent enforcement, and monotonicity.
  - Feature: T1.2.
- [ ] **Verify body equality on identifier match before deduplication.**
  - Acceptance: Even on a hash match the writer MUST verify byte-equal canonical JSON before treating a write as a duplicate.
  - Done when: a unit test that injects a forced collision asserts the verify-then-dedupe path.
  - Feature: T1.2.
- [ ] **Provide snapshot fetch operations.**
  - Acceptance: The writer's adapter MUST expose fetch by identifier, by `config_version`, by parent (for tree traversal), and by date range.
  - Done when: integration tests in `crates/adapters/tests/snapshot.rs` cover every fetch shape.
  - Feature: T1.2.

### Database migrations

- [ ] **Author migration `0004_snapshots_immutable.sql`.**
  - Acceptance: Migration `0004_snapshots_immutable.sql` MUST add a SQLite trigger blocking `UPDATE` and `DELETE` on `snapshots`.
  - Done when: an integration test attempting `UPDATE` and `DELETE` observes a database-level error.
  - Feature: T1.2 (mitigates ADR-0009 invariant).

### Tests

- [ ] **Canonicalisation corpus.**
  - Acceptance: A unit-test corpus of 50 desired states with semantically equivalent JSON variants MUST hash to identical snapshot identifiers.
  - Done when: `cargo test -p trilithon-core canonical_json::corpus` passes.
  - Feature: T1.2.
- [ ] **Strict monotonicity of `config_version` per `caddy_instance_id`.**
  - Acceptance: A property test MUST assert strict monotonic increase per instance across interleaved writes.
  - Done when: `cargo test -p trilithon-adapters snapshot::props::monotonic_version` passes.
  - Feature: T1.2 (substrate for T2.10).
- [ ] **Root snapshot has NULL parent.**
  - Acceptance: An integration test MUST assert that the very first snapshot for an instance has `parent_id IS NULL` and that subsequent snapshots have a non-null parent.
  - Done when: the integration test passes.
  - Feature: T1.2.

### Documentation

- [ ] **Document the snapshot format and immutability guarantee.**
  - Acceptance: `core/README.md` MUST add a "Snapshots" section describing canonical JSON, content addressing, parent linkage, and the `UPDATE`/`DELETE` ban.
  - Done when: the section is present and references ADR-0009.
  - Feature: T1.2.

## Cross-references

- ADR-0009 (immutable content-addressed snapshots and audit log).
- ADR-0012 (optimistic concurrency on monotonic `config_version`).
- PRD T1.2 (snapshot history with content addressing).
- Architecture: "Snapshots — content addressing," "Data model — snapshots table," "Concurrency — monotonic `config_version`."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] Two snapshots with byte-identical canonical JSON share an identifier and deduplicate at the row level.
- [ ] Any attempt to `UPDATE` or `DELETE` a `snapshots` row fails at the database layer.
- [ ] `config_version` is strictly monotonically increasing per `caddy_instance_id`.
- [ ] A snapshot records its parent identifier; the root snapshot's parent identifier is `NULL`.
