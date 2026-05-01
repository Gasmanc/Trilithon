# Phase 6 — Audit log with secrets-aware redactor

Source of truth: [`../phases/phased-plan.md#phase-6--audit-log-with-secrets-aware-redactor`](../phases/phased-plan.md#phase-6--audit-log-with-secrets-aware-redactor).

## Pre-flight checklist

- [ ] Phase 5 complete (snapshots exist; diffs are computable).

## Tasks

### Backend / core crate

- [ ] **Define the `AuditEvent` enum for Tier 1.**
  - Acceptance: `AuditEvent` is a closed Rust enum whose variant set is in one-to-one correspondence with the dotted `kind` strings in architecture §6.6. The §6.6 table is authoritative; the variant names are an implementation convenience. Each variant's `Display` impl returns the canonical dotted string verbatim. The Tier 1 subset MUST cover (at minimum) the kinds for: `mutation.proposed`, `mutation.submitted`, `mutation.applied`, `mutation.rejected`, `mutation.conflicted`, `mutation.rejected.missing-expected-version`, `config.applied`, `config.apply-failed`, `config.rolled-back`, `config.drift-detected`, `config.drift-resolved`, `auth.login-succeeded`, `auth.login-failed`, `auth.bootstrap-credentials-created`, `auth.bootstrap-credentials-rotated`, `secrets.revealed`, `secrets.master-key-fallback-engaged`, `caddy.ownership-sentinel-conflict`, `caddy.ownership-sentinel-takeover`. Tier 2 kinds MUST be reserved as placeholders only.
  - Done when: the enum compiles and `cargo test -p trilithon-core audit::tests::event_set` asserts the closed set.
  - Feature: T1.7.
- [ ] **Define the `AuditRow` record type.**
  - Acceptance: `AuditRow` MUST carry `event_id` (ULID), `correlation_id`, `actor`, `event_type`, `subject_type`, `subject_id`, `before_snapshot_id`, `after_snapshot_id`, `redacted_diff_json`, `result`, `error_kind`, and `created_at_unix_seconds`.
  - Done when: the type compiles and serde round-trip is exercised.
  - Feature: T1.7.
- [ ] **Implement the `SecretsRedactor`.**
  - Acceptance: `SecretsRedactor` MUST walk the diff, identify schema-marked secret fields, and replace their values with `"***"` plus a stable hash prefix derived from the encrypted-at-rest ciphertext. Plaintext secrets MUST NOT reach the writer, satisfying H10.
  - Done when: a unit test corpus over every schema-marked secret field asserts the redactor never emits plaintext bytes.
  - Feature: T1.7 (mitigates H10).
- [ ] **Make table-write functions private to the audit module.**
  - Acceptance: The function that writes to `audit_log` MUST be private to the audit module; the public surface MUST only expose `AuditWriter::record(event)`.
  - Done when: a compile-fail test asserts external crates cannot reach the raw writer.
  - Feature: T1.7.

### Backend / adapters crate

- [ ] **Implement `AuditWriter::record` over `Storage`.**
  - Acceptance: `AuditWriter::record` MUST persist a single audit row in a transaction, invoking the redactor on the diff before storage.
  - Done when: integration tests cover happy path, redactor invocation, and storage failure surfacing.
  - Feature: T1.7.
- [ ] **Implement the audit query API.**
  - Acceptance: The query API MUST support range by time, range by correlation identifier, range by actor, and range by event type. Queries MUST be paginated with default 100, maximum 1000.
  - Done when: integration tests cover pagination bounds and every filter combination.
  - Feature: T1.7.
- [ ] **Tracing layer propagates correlation identifier.**
  - Acceptance: A `tracing` layer MUST inject a ULID correlation identifier into every span at the entry point (HTTP request, scheduler tick, signal handler) and read it back when an audit row is written.
  - Done when: a unit test asserts the correlation identifier is non-null on every audit row.
  - Feature: T1.7.

### Database migrations

- [ ] **Author migration `0003_audit_immutable.sql`.**
  - Acceptance: Migration `0003_audit_immutable.sql` MUST add SQLite triggers blocking `UPDATE` and `DELETE` on `audit_log`.
  - Done when: an integration test attempting `UPDATE` and `DELETE` observes a database-level error.
  - Feature: T1.7.

### Tests

- [ ] **Redactor corpus covers every secret-marked schema field.**
  - Acceptance: The redactor test corpus MUST exercise every schema field marked secret and assert no plaintext byte appears in `redacted_diff_json`.
  - Done when: `cargo test -p trilithon-core audit::redactor::corpus` passes.
  - Feature: T1.7 (mitigates H10).
- [ ] **Naïve-diff corpus.**
  - Acceptance: A corpus of "naïve diff" inputs MUST be verified to produce only redacted output through the writer.
  - Done when: `cargo test -p trilithon-adapters audit::naive_corpus` passes.
  - Feature: T1.7.
- [ ] **Every audit row carries a non-null correlation identifier.**
  - Acceptance: An integration test MUST scan the `audit_log` table and assert no row has a null `correlation_id`.
  - Done when: the integration test passes.
  - Feature: T1.7.

### Documentation

- [ ] **Document the audit pipeline and the redactor invariant.**
  - Acceptance: `core/README.md` MUST add an "Audit log" section stating that no code path may bypass the redactor.
  - Done when: the section is present and references ADR-0009.
  - Feature: T1.7.

## Cross-references

- ADR-0009 (immutable content-addressed snapshots and audit log).
- ADR-0014 (secrets encrypted at rest with keychain master key — redactor pairs with the vault in Phase 10).
- PRD T1.7 (audit log with correlation identifiers).
- Architecture: "Audit log — invariants," "Secrets redactor," "Tracing — correlation identifier."

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] No code path writes to `audit_log` without going through `AuditWriter::record(event)`.
- [ ] Every diff written to `redacted_diff_json` passes the redactor; the corpus covers every schema field marked secret.
- [ ] Every audit row carries a non-null correlation identifier.
- [ ] Any attempt to `UPDATE` or `DELETE` an `audit_log` row fails at the database layer.
