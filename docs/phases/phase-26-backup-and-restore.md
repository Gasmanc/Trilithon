# Phase 26 — Backup and restore

Source of truth: [`../phases/phased-plan.md#phase-26--backup-and-restore`](../phases/phased-plan.md#phase-26--backup-and-restore).

## Pre-flight checklist

- [ ] Phase 25 complete.

## Tasks

### HTTP endpoints

- [ ] **Implement `POST /api/v1/backup`.**
  - Acceptance: The endpoint MUST accept a passphrase and produce the same native bundle as `GET /api/v1/export/native-bundle`, additionally streaming through the access log store. Rolling logs MUST be excluded by default; an opt-in flag MUST include them.
  - Done when: integration tests cover both flags.
  - Feature: T2.12.
- [ ] **Implement `POST /api/v1/restore`.**
  - Acceptance: The endpoint MUST accept a bundle and a passphrase, and MUST execute the seven-step pipeline below.
  - Done when: integration tests cover happy path, every failure branch, and the cross-machine path.
  - Feature: T2.12.

### Restore pipeline

- [ ] **Step 1: verify the manifest against the compatibility matrix.**
  - Acceptance: The handler MUST verify the manifest against this Trilithon's compatibility matrix (schema version equal or newer-with-migrations).
  - Done when: an integration test against a future-schema bundle observes a typed rejection.
  - Feature: T2.12.
- [ ] **Step 2: decrypt the master-key wrap using the passphrase.**
  - Acceptance: The handler MUST decrypt the wrap using the passphrase; a wrong passphrase MUST be rejected at unwrap.
  - Done when: an integration test exercises both branches.
  - Feature: T2.12.
- [ ] **Step 3: validate the included audit log against content addressing.**
  - Acceptance: The handler MUST validate that every audit row's content matches its address.
  - Done when: an integration test against a tampered log observes a typed rejection.
  - Feature: T2.12.
- [ ] **Step 4: validate the snapshot tree.**
  - Acceptance: The handler MUST validate that every parent is reachable and every snapshot's hash matches its content.
  - Done when: an integration test against a corrupt snapshot tree observes a typed rejection.
  - Feature: T2.12.
- [ ] **Step 5: run preflight against the post-restore desired state.**
  - Acceptance: The handler MUST run preflight; failures MUST surface as warnings consistent with H9, not blockers.
  - Done when: an integration test exercises a Caddy-version-skew bundle and observes the warning behaviour.
  - Feature: T2.12 (mitigates H9).
- [ ] **Step 6: atomic swap on full pass.**
  - Acceptance: On full pass the handler MUST atomically swap the data directory with the restored data and record `RestoreApplied` in the new audit log. The swap MUST happen under an exclusive lock; failure MUST leave the staging directory for forensic inspection.
  - Done when: an integration test asserts the swap and the staging-on-failure behaviour.
  - Feature: T2.12.
- [ ] **Step 7: leave existing state untouched on any failure.**
  - Acceptance: On any failure the handler MUST return a typed structured error and leave the existing state untouched.
  - Done when: an integration test asserts no state change after every failure branch.
  - Feature: T2.12.

### Cross-machine restore

- [ ] **Track installation identifiers across machines.**
  - Acceptance: The bundle's manifest MUST carry the source `installation_id`; restoring on a different machine MUST produce a new `installation_id` and write a `RestoreCrossMachine` audit row recording both.
  - Done when: an integration test on a different machine asserts both rows.
  - Feature: T2.12.

### Frontend

- [ ] **Implement the Backup and restore page.**
  - Acceptance: The page MUST host a "Create backup" form (passphrase, optional include-logs flag) and a "Restore from bundle" form (file upload, passphrase, explicit confirmation).
  - Done when: Vitest tests cover both flows.
  - Feature: T2.12.

### Failure-mode tests

- [ ] **Tampered bundle rejected at audit-log validation.**
  - Acceptance: A tampered bundle MUST be rejected at audit-log validation.
  - Done when: the integration test passes.
  - Feature: T2.12.
- [ ] **Wrong passphrase rejected at master-key unwrap.**
  - Acceptance: A wrong passphrase MUST be rejected at master-key unwrap.
  - Done when: the integration test passes.
  - Feature: T2.12.

## Cross-references

- ADR-0014 (secrets vault — master-key wrap).
- ADR-0009 (audit log).
- PRD T2.12 (backup and restore).
- Architecture: "Backup and restore," "Atomic swap," "Cross-machine identifiers."
- `docs/architecture/bundle-format-v1.md` (authoritative spec for the manifest, snapshots/, audit-log.ndjson, secrets-vault.encrypted layout consumed by the restore pipeline).
- `docs/architecture/trait-signatures.md` (Storage and SecretsVault traits consumed by the restore handler).

## Sign-off checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] Backups are encrypted with a user-chosen passphrase.
- [ ] Restore validates the backup before overwriting any state.
- [ ] Restore on a different machine produces an identical desired state and an audit log entry recording the restore.
- [ ] A tampered or wrong-passphrase bundle is rejected without state change.
