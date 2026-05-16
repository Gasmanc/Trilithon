# Phase 10 — Tagging Analysis
**Generated:** 2026-05-16
**Model:** opus (extended thinking)
**Documents read:**
- CLAUDE.md — 208 lines
- docs/architecture/architecture.md — 1124 lines
- docs/architecture/trait-signatures.md — 734 lines
- docs/planning/PRD.md — 952 lines
- docs/adr/ — 17 ADR files (0009 and 0014 read in full; 0014 = secrets-encrypted-at-rest, 0009 = immutable snapshots/audit)
- docs/todo/phase-10-secrets-vault.md — 714 lines
- docs/architecture/seams.md — 137 lines
- docs/architecture/contract-roots.toml — 37 lines
- docs/architecture/contracts.md — 17 lines (empty registry)
**Slices analysed:** 7

---

## Proposed Tags

### 10.1: `Ciphertext`, `EncryptContext`, `SecretsVault` types in core
**Proposed tag:** [cross-cutting]
**Reasoning:** This slice introduces a brand-new shared trait surface — `core::secrets::SecretsVault` — into `core`, plus the `EncryptContext`, `Ciphertext`, `OwnerKind`, `AlgorithmTag`, `CryptoError`, and `MasterKeyRotation` types that every downstream slice (10.2, 10.3, 10.5, 10.6, 10.7) and the adapter implementation depend on structurally. Although all file changes land in one crate (`core/crates/core/src/secrets/mod.rs` + `lib.rs`), the rubric's "introduces a new trait surface that other slices implement" and "shared trait that other slices implement" criteria both fire — `SecretsVault` is in `trait-signatures.md §3` and is the contract the rest of the phase is built on. It also adds a new `pub mod` to `core` exercised across the adapters↔core boundary.
**Affected seams:** PROPOSED: secrets-vault-core (SecretsVault trait ↔ adapter implementations and HTTP/mutation callers)
**Planned contract additions:** `trilithon_core::secrets::SecretsVault`, `trilithon_core::secrets::EncryptContext`, `trilithon_core::secrets::Ciphertext`, `trilithon_core::secrets::OwnerKind`, `trilithon_core::secrets::AlgorithmTag`, `trilithon_core::secrets::CryptoError`, `trilithon_core::secrets::MasterKeyRotation` (none currently in contract-roots.toml; this slice should propose adding them as a new contract root, mirroring the Phase 7 reconciler precedent)
**Confidence:** high
**If low confidence, why:** —

### 10.2: XChaCha20-Poly1305 encryptor with associated-data binding
**Proposed tag:** [standard]
**Reasoning:** All file changes are confined to one crate (`core/crates/adapters/src/secrets_local/{mod.rs,cipher.rs}` plus `adapters/Cargo.toml`). `CipherCore` is a concrete struct, not a trait, and the slice implements no shared trait — it consumes `core::secrets` types but defines only crate-internal `encrypt`/`decrypt`/`associated_data`. It adds two new dependencies (`chacha20poly1305`, `getrandom`) but introduces no I/O beyond in-process random-byte sampling and emits no audit or tracing events. Self-contained cryptographic primitive within the adapters layer.
**Affected seams:** none
**Planned contract additions:** none (`CipherCore` is an internal adapter type, not in a contract-rooted module)
**Confidence:** high
**If low confidence, why:** —

### 10.3: `KeychainBackend` (macOS Keychain, Linux Secret Service)
**Proposed tag:** [cross-cutting]
**Reasoning:** This slice introduces a new shared async trait — `MasterKeyBackend` — that slice 10.4 (`FileBackend`) also implements, so the rubric's "modifies/introduces a shared trait that other slices implement" criterion fires directly. It adds real OS-keychain I/O (`keyring` crate, IPC to `Security.framework` / Secret Service) and emits the `secrets.master-key.rotated` tracing event listed in architecture §12.1 — a tracing convention the file backend's `rotate` path must mirror. Files stay within the adapters crate, but the trait introduction plus the cross-slice dependency from 10.4 push this above [standard].
**Affected seams:** PROPOSED: master-key-backend (MasterKeyBackend trait ↔ KeychainBackend + FileBackend implementations, with vault-constructor fallback selection)
**Planned contract additions:** none in a contract-rooted module today; `MasterKeyBackend` lives in `trilithon_adapters::secrets_local` and is not yet a contract root. If the project wants the backend abstraction tracked, the slice should propose it — but absent a core-side root it is internal.
**Confidence:** medium
**If low confidence, why:** `MasterKeyBackend` is defined in adapters and not yet a `trait-signatures.md` entry, so whether it counts as a "registered" trait surface vs. an internal adapter trait is a judgement call; the cross-slice 10.4 dependency makes cross-cutting the safe tag regardless.

### 10.4: `FileBackend` fallback with mode 0600 master-key file
**Proposed tag:** [standard]
**Reasoning:** Files are confined to one crate (`core/crates/adapters/src/secrets_local/file.rs`). The slice *implements* the existing `MasterKeyBackend` trait introduced by 10.3 rather than defining a new one — implementing one trait is explicitly [standard]. It adds filesystem I/O within a single adapter (master-key file read/write/chmod). It emits `secrets.file-backend.startup` and `secrets.master-key.permissions-tightened` tracing events, but those events are local to this backend and are not a convention other slices must follow (they are flagged as new §12.1 candidates, but no other slice emits them). No layer boundary crossed.
**Affected seams:** master-key-backend (PROPOSED in 10.3 — this slice is the second implementer)
**Planned contract additions:** none
**Confidence:** high
**If low confidence, why:** —

### 10.5: `0004_secrets.sql` migration plus `secrets_metadata` writer
**Proposed tag:** [cross-cutting]
**Reasoning:** This is a schema migration (`0004_secrets.sql`) that slices 10.6 (`record_reveal`, `get_secret`) and 10.7 (`upsert_secret`) structurally depend on — the rubric's "migration that other slices structurally depend on" criterion fires. The migration also extends architecture §6.9 with `backend_kind` and `algorithm` columns, which is a schema-of-record change. It references ADR-0014 and architecture §6.9, and emits the `storage.migrations.applied` tracing event. Files stay in the adapters crate, but the cross-slice migration dependency plus the architecture §6.9 schema divergence (the TODO's schema adds columns beyond the architecture table) make this cross-cutting.
**Affected seams:** PROPOSED: secrets-metadata-store (secrets_metadata schema ↔ reveal endpoint + mutation-pipeline writers)
**Planned contract additions:** none in a contract-rooted module; `SecretRow`, `upsert_secret`, `get_secret`, `record_reveal` live in `trilithon_adapters::storage_sqlite::secrets` and are internal adapter symbols.
**Confidence:** medium
**If low confidence, why:** The §6.9 architecture table lacks the `algorithm` and `backend_kind` columns the TODO adds; this is an architecture-doc divergence the implementer must reconcile (TODO open question flags it), which is the kind of structural drift that warrants the cross-cutting tag.

### 10.6: `POST /api/v1/secrets/{secret_id}/reveal` with step-up auth
**Proposed tag:** [cross-cutting]
**Reasoning:** This slice spans a layer boundary: the HTTP handler in `adapters::http_axum` wires `core::secrets::SecretsVault::decrypt` (core) together with `AuthenticatedSession` (Phase 9 auth middleware), the `secrets_metadata` store (10.5), and the audit log writer. It emits the `secrets.revealed` audit kind (architecture §6.6, mapped to the `SecretsRevealed` Rust variant) and a failed-step-up `auth.login-failed` row — audit events that depend on the Phase 6/9 audit infrastructure. It references ADR-0014, PRD T1.15, and architecture §6.6/§11 (3+ requirement references). New HTTP endpoint stitching multiple subsystems = cross-cutting.
**Affected seams:** secrets-metadata-store (PROPOSED in 10.5); the reveal endpoint also touches the Phase 9 auth seam (step-up password verification) and the audit-writer path.
**Planned contract additions:** none in a contract-rooted module; `RevealRequest`/`RevealResponse`/`reveal` are adapter HTTP types.
**Confidence:** high
**If low confidence, why:** —

### 10.7: Wire mutation pipeline through the vault and feed redactor with ciphertext hash
**Proposed tag:** [cross-cutting]
**Reasoning:** This slice explicitly spans multiple crates: `core/crates/core/src/mutation/secrets_extract.rs` and `core/crates/core/src/audit/redactor.rs` (core) plus `core/crates/adapters/src/mutation_queue/secrets.rs` (adapters). It crosses the core↔adapters boundary, extends the Phase 6 redactor with a new `CiphertextHasher` impl (`VaultBackedHasher`) — a shared `core::audit::redactor` surface other code consumes — and changes the Phase 9 mutation handler (slice 9.7) to call `route_mutation_secrets_through_vault` before snapshot construction. It references ADR-0014, PRD T1.15, Hazard H10, and architecture §6.9 (4 references). It is the integration slice that makes secret-marked fields flow through the vault — structurally cross-cutting by every criterion.
**Affected seams:** secrets-metadata-store (PROPOSED in 10.5); also touches the Phase 6 redactor seam and the Phase 9 mutation-pipeline boundary.
**Planned contract additions:** `trilithon_core::mutation::secrets_extract::extract_secrets`, `trilithon_core::mutation::secrets_extract::substitute_secret_refs`, `trilithon_core::mutation::secrets_extract::ExtractedSecret` (new pub items in `core`; consider proposing as contract roots since they define the pure-core secret-extraction surface). `VaultBackedHasher` and `route_mutation_secrets_through_vault` are adapter symbols, not contract-rooted.
**Confidence:** high
**If low confidence, why:** —

---

## Summary
- 0 trivial
- 2 standard
- 5 cross-cutting
- 2 low-confidence (10.3, 10.5 — require human review)

## Notes

Cross-slice patterns:

1. **Trait-introduction concentration.** Two new trait surfaces appear in this phase: `core::secrets::SecretsVault` (10.1, already in `trait-signatures.md §3`) and `adapters::secrets_local::MasterKeyBackend` (10.3, *not* in `trait-signatures.md`). Per the trait-signatures.md "Stability and authority" rule, any new trait must be added to that document in the same commit. The implementer of 10.3 should either add `MasterKeyBackend` to `trait-signatures.md` or justify keeping it as a purely internal adapter trait — flag for `/phase-merge-review`.

2. **Seam staging.** This phase touches no existing registered seam (all five active seams in `seams.md` are Phase 7 apply-path seams). Every cross-cutting slice here proposes a *new* seam. Per `seams.md` rules, `/tag-phase` cannot invent free-text names into `seams.md` directly — the three proposed seams (`secrets-vault-core`, `master-key-backend`, `secrets-metadata-store`) must be written to `seams-proposed.md` for `/phase-merge-review` ratification.

3. **Contract registry is empty.** `contracts.md` has zero contracts and `contract-roots.toml` lists only Phase 7 reconciler roots. Phase 10 introduces the first `core::secrets` public surface. The phase should add `trilithon_core::secrets::*` to `contract-roots.toml` (a contract change reviewed by `/phase-merge-review`), mirroring the Phase 7 precedent — otherwise the new vault trait surface is invisible to the registry.

4. **Architecture-doc divergence (10.5).** The TODO's `0004_secrets.sql` adds `algorithm`, `backend_kind`, `key_version`, `updated_at` columns and a `key_version` index beyond the architecture §6.9 table. The TODO's own open-questions section acknowledges this. The implementer must reconcile §6.9 in the same commit (per the §6.6/§12.1 "update in the same commit" convention applied analogously), or raise an ADR.

5. **New tracing events.** Slices 10.3 and 10.4 emit `secrets.file-backend.startup` and `secrets.master-key.permissions-tightened`. These ARE already in architecture §12.1 (lines 1061–1062), so no §12.1 addition is needed — the TODO's "not yet listed" claim in its open questions is stale and should be corrected. `secrets.master-key.rotated` is also already present (§12.1 line 1063).

---

## User Decision
**Date:** 2026-05-16
**Decision:** accepted

### Modifications (if any)
None.

### Notes from user
None.
