# Adversarial Review — Phase 6 — Round 4

## Summary

3 critical · 1 high · 2 medium · 1 low

## Round 3 Closure

| ID | Status | Notes |
|----|--------|-------|
| F201 | Closed | Dedicated `Mutex<SqliteConnection>` + `BEGIN IMMEDIATE` correctly serialises all audit writes within a single process |
| F202 | Closed | Migration 0007 eliminated; startup query substituted |
| F203 | Closed | `ZERO_SENTINEL` vs `""` distinction specified: `""` → `ChainError::EmptyHash`, ZERO_SENTINEL → skipped |
| F204 | Closed | Moot — `Transaction` no longer used in `AuditWriter` |
| F205 | Closed | `strum::EnumCount` derive replaces manual count constant |
| F206 | Closed | `tracing::warn!` includes synth value as structured field |
| F207 | Closed | `ORDER BY rowid ASC` specified |
| F208 | Closed | `const _: () = assert!(!PHASE6_REGISTRY.0.is_empty())` specified |
| F209 | Closed | Empty `correlation_id` → `synth:` replacement at write time |

---

## New Findings

### F301 — CRITICAL — Surviving `Storage::record_audit_event` write path inserts `prev_hash = ''`, permanently breaking `chain::verify`

**Category:** Composition failure

**Attack:** After migration 0006 is applied, `prev_hash TEXT NOT NULL DEFAULT ''` is added to `audit_log`. Any code that calls `Storage::record_audit_event` — the existing trait method in `sqlite_storage.rs` that builds an `INSERT INTO audit_log` statement without a `prev_hash` bind — causes SQLite to insert `DEFAULT ''` for that column. `chain::verify` treats `prev_hash == ""` as `ChainError::EmptyHash`, not as the ZERO_SENTINEL skip case. The design introduces `AuditWriter` as a new parallel struct but is silent about the surviving `record_audit_event` path. Any call site that continues using the old path after migration produces a permanent chain error for that row — and the immutability triggers prevent correction.

**Why the design doesn't prevent it:** The design introduces `AuditWriter` without retiring `Storage::record_audit_event`. Both paths remain live after migration.

**Mitigation required:** Either (a) explicitly retire all call sites of `Storage::record_audit_event` in Phase 6 (with a compile-fail test asserting the method is private or removed), or (b) update `SqliteStorage::record_audit_event` to delegate through `AuditWriter` — acquiring the mutex and computing `prev_hash` identically. The Phase 6 TODO must add an explicit task for this. Leaving both paths live is not acceptable.

---

### F302 — CRITICAL — `AuditEvent::COUNT` is a trait associated constant; the const assertion fails without UFCS or a `use` statement

**Category:** Logic flaw

**Attack:** In strum 0.26+, `EnumCount` is a trait with associated constant `COUNT: usize`. The assertion `const _: () = assert!(AuditEvent::COUNT == AUDIT_KIND_VOCAB.len())` compiles only if `use strum::EnumCount;` is in scope. Without it, the compiler produces `no associated item named COUNT found for type AuditEvent`. If the `use` is placed inside `#[cfg(test)]`, the assertion moves into test context and loses its production compile-time guarantee. The correct form using UFCS — `assert!(<AuditEvent as strum::EnumCount>::COUNT == …)` — works without a `use` statement at any scope level.

**Why the design doesn't prevent it:** The design specifies the assertion without accounting for trait scoping rules.

**Mitigation required:** Specify the assertion as `const _: () = assert!(<AuditEvent as strum::EnumCount>::COUNT == AUDIT_KIND_VOCAB.len());` (UFCS form, no `use` required). This must appear in production code, not under `#[cfg(test)]`.

---

### F303 — CRITICAL — `PHASE6_REGISTRY` paths `/nonce` and `/ciphertext` do not exist in the desired-state diff; the registry is structurally non-empty but semantically empty

**Category:** Assumption violation

**Attack:** The design states `PHASE6_REGISTRY` must contain at minimum `/nonce` and `/ciphertext` "from `secrets_metadata`." The `RedactedDiff` is computed from the desired-state diff — the before/after difference of `DesiredState` serialised to JSON. `DesiredState` has top-level keys `version`, `routes`, `upstreams`, `policies`, `presets`, `tls`, `global`. It has no `nonce` or `ciphertext` field. `secrets_metadata` is a separate SQLite table, not part of the desired state. The compile-time non-empty assertion passes (two entries), but at runtime those entries match zero JSON nodes in every diff ever produced. The corpus test passes trivially with two cases exercising paths that never appear in production. The `SecretsRevealed` audit event — the one most in need of redaction — will store unredacted diffs.

**Why the design doesn't prevent it:** The design conflates the `secrets_metadata` storage table with the desired-state diff document. The non-empty assertion guards structure, not semantics.

**Mitigation required:** Identify which fields in the actual `DesiredState` diff carry secret-adjacent data. If none exist in Phase 6, document `PHASE6_REGISTRY` explicitly as a placeholder with a `todo!`-gated path (e.g., one that always matches the first invocation) so the corpus test is non-vacuous and fails fast if called with a real diff. Block `SecretsRevealed` events via `record` returning `Err(AuditError::NoSecretRegistry)` until Phase 10 provides real paths. The non-empty assertion is insufficient — the design must specify the actual paths.

---

### F304 — HIGH — Two vocabulary constants (`AUDIT_KIND_VOCAB` and `AUDIT_KINDS`) diverge; compile-time assertion covers only one

**Category:** Logic flaw

**Attack:** Two distinct vocabulary constants exist: `AUDIT_KIND_VOCAB` in `core/crates/core/src/audit.rs` (41 entries, the compile-time assertion target) and `AUDIT_KINDS` in `core/crates/core/src/storage/audit_vocab.rs` (47 entries, used at runtime by `record_audit_event` and the in-memory store). The lists differ — `"tool-gateway.tool-invoked"` is in `AUDIT_KIND_VOCAB` but NOT in `AUDIT_KINDS`. After Phase 6 adds three new variants and updates only `AUDIT_KIND_VOCAB`, the compile-time assertion passes, but any write via the old `record_audit_event` path for those three new kinds is rejected at runtime by the `AUDIT_KINDS` guard.

**Why the design doesn't prevent it:** The design specifies updating `AUDIT_KIND_VOCAB` but never mentions `AUDIT_KINDS`.

**Mitigation required:** Collapse the two constants into one before or as part of Phase 6. Delete `AUDIT_KINDS` and redirect all its use sites to `AUDIT_KIND_VOCAB`. The compile-time assertion should reference the same constant that all runtime checks use. Fix the pre-existing `tool-gateway.tool-invoked` gap in the same commit.

---

### F305 — MEDIUM — `AuditWriter::record` inserts `kind` as a raw string with no vocabulary validation; the typed `AuditEvent` replaces the runtime check but this is not documented

**Category:** Abuse case

**Attack:** `Storage::record_audit_event` validated the `kind` field against `AUDIT_KINDS` before writing. `AuditWriter::record` has no such check — `AuditRow.kind` is a typed `AuditEvent`, and its `Display` string is bound directly. The design removes the runtime check without documenting that the `AuditEvent` enum is the replacement gate. If the `Display` string for any variant is not in `AUDIT_KIND_VOCAB` (as with `tool-gateway.tool-invoked` today), an invalid kind is written silently.

**Why the design doesn't prevent it:** The omission of the runtime check is undocumented. Future contributors may add it back as a "defence in depth" measure against a constant that may diverge.

**Mitigation required:** Either (a) add a `CHECK (kind IN (…))` constraint to `audit_log.kind` in migration 0006 (enforced even against raw SQL writes), or (b) document explicitly in `AuditWriter::record` that the `AuditEvent` enum is the sole vocabulary gate and no runtime string check is needed. Option (a) also surfaces the pre-existing discrepancy at migration time.

---

### F306 — LOW — Startup trigger guard checks trigger name + `RAISE` substring but not the target table

**Category:** Logic flaw

**Attack:** Design item 10(a) checks that `BEFORE UPDATE`/`BEFORE DELETE` triggers on `audit_log` exist and that `sql` contains `RAISE`. A trigger named `audit_log_no_update` that fires `BEFORE UPDATE ON caddy_instances BEGIN RAISE(ABORT, …) END` passes the guard (name matches, `sql` contains `RAISE`) but leaves `audit_log` unprotected.

**Why the design doesn't prevent it:** The check inspects trigger name and DDL substring without verifying the target table.

**Mitigation required:** Extend the `sqlite_master` query to also check `tbl_name = 'audit_log'` (SQLite populates this column for triggers), or add `AND sql LIKE '%ON audit_log%'` to the DDL substring check.
