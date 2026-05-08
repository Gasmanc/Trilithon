# Adversarial Review — Phase 6 — Round 1

## Summary

4 critical · 5 high · 3 medium · 1 low

---

## Findings

### F001 — CRITICAL — ADR-0009 `prev_hash` chain omitted entirely

**Category:** assumption violation

**Attack:** ADR-0009 requires each audit row to carry a `prev_hash` column (SHA-256 of the previous row's canonical serialisation) and a chain-integrity check on daemon startup. The `audit_log` schema in `0001_init.sql` does not include `prev_hash`, and the Phase 6 TODO does not mention adding it. Migration `0003_audit_immutable.sql` is scoped only to `UPDATE`/`DELETE` triggers. No task covers the chain-hash column, the canonical serialisation function, or the startup verification pass. The entire tamper-evidence guarantee of ADR-0009 will be absent at the end of Phase 6 with no failing test to catch it, because the acceptance criteria make no reference to it.

**Why the design doesn't prevent it:** The TODO lists `AuditRow` fields that match `0001_init.sql` columns verbatim — `before_snapshot_id`, `after_snapshot_id`, etc. — but `prev_hash` appears in neither the schema nor the `AuditRow` acceptance criteria. The design has silently dropped an ADR requirement.

**Mitigation required:** Add `prev_hash TEXT NOT NULL` to the `audit_log` schema (via an additive migration in Phase 6, or acknowledge it is deferred with an explicit ADR amendment). Add `prev_hash` to `AuditRow`. Specify the canonical serialisation format. Add a startup chain-check task. Make the chain-integrity check an acceptance criterion with a dedicated test.

---

### F002 — CRITICAL — `RedactedDiff` newtype absent; unredacted diffs can reach the writer at compile time

**Category:** assumption violation

**Attack:** ADR-0009 specifies a `RedactedDiff` newtype in `crates/core` that makes it a compile-time error to pass an unredacted diff to the writer. The Phase 6 design instead writes the redactor as a plain `SecretsRedactor` struct whose output is `redacted_diff_json: String` (or equivalent) stored directly on `AuditRow`. `AuditWriter::record` accepts an `AuditRow` or `AuditEvent`, not a `RedactedDiff`. Any call site that constructs an `AuditRow` manually — including future code, tests constructing rows for query tests, or any path that short-circuits the redactor — can store plaintext secret data. The compiler will never catch it. The entire point of the newtype is to make the invariant enforced by the type system, not by convention.

**Why the design doesn't prevent it:** The `AuditRow` acceptance criteria list `redacted_diff_json` as a plain field. The `SecretsRedactor` acceptance criteria describe a function, not a type boundary. No task says "the only way to produce a value storable in `redacted_diff_json` is to pass through `SecretsRedactor`."

**Mitigation required:** Define `RedactedDiff(String)` as a `pub struct` in `crates/core/src/audit.rs` with the constructor `pub fn from_raw(raw: &Diff, redactor: &SecretsRedactor) -> RedactedDiff` (or equivalent). Make the `redacted_diff_json` field of `AuditRow` take `RedactedDiff`, not `String`. `AuditWriter::record` must accept a type that carries `RedactedDiff` so the compiler enforces the invariant.

---

### F003 — CRITICAL — Redaction hash source contradicts ADR-0014; creates permanent audit corruption

**Category:** assumption violation

**Attack:** The TODO states the redactor replaces secret field values with `"***"` plus a hash prefix derived from "the encrypted-at-rest ciphertext." ADR-0014 states the stable placeholder is SHA-256 of the plaintext prefixed with `secret:`. These are irreconcilable: (1) Phase 10 (secrets vault) has not been implemented yet, so there is no encrypted-at-rest ciphertext to hash — the Phase 6 redactor will have no input to derive the hash from and cannot implement its own spec. (2) If Phase 6 ships with plaintext-hash and Phase 10 later changes it to ciphertext-hash, existing rows have placeholders computed one way and new rows another way, breaking any cross-row correlation of secret field values. (3) `"***"` as a prefix is not `secret:` — downstream tooling that parses placeholders by prefix will misparse one or the other. Audit rows written under Phase 6 carry a redaction format that either cannot be implemented (no ciphertext exists) or permanently diverges from the ADR.

**Why the design doesn't prevent it:** The TODO was written against an assumption that Phase 10 is in scope. The design does not resolve the conflict or provide a fallback spec for the pre-vault period.

**Mitigation required:** Resolve the contradiction before implementation begins. Either: (a) adopt ADR-0014 verbatim (`secret:<sha256-of-plaintext>`) for Phase 6 and amend ADR-0014 to note the vault-hash variant arrives in Phase 10 with a migration, or (b) amend the TODO to use a deterministic placeholder (`secret:REDACTED` with no hash) for Phase 6, deferring the hash to Phase 10. The format chosen must be documented in `redaction_sites` semantics and must be stable across migrations.

---

### F004 — CRITICAL — Migration numbering collision; `0003_audit_immutable.sql` already occupied

**Category:** assumption violation

**Attack:** The design calls for authoring migration `0003_audit_immutable.sql`. Existing migrations include `0001_init.sql`, `0002_capability_probe.sql`, `0003_seed_local_instance.sql`, `0004_snapshots_immutable.sql`, and `0005_canonical_json_version.sql`. Migration `0003` is already `0003_seed_local_instance.sql`. If `sqlx` or the project's migration runner uses filename-prefix ordering, the new file either collides (two `0003_*` files; runner behaviour is undefined or panics) or silently shadows the existing seed. Either outcome is a hard runtime failure or silent data loss — the local instance seed row is never inserted, breaking all subsequent foreign-key constraints that reference `caddy_instance_id = 'local'`.

**Why the design doesn't prevent it:** The TODO names the migration `0003_audit_immutable.sql` without acknowledging that `0003_seed_local_instance.sql` exists. No task verifies migration sequence before authoring.

**Mitigation required:** Rename the new migration to `0006_audit_immutable.sql` (the next available slot). Add a CI step or `just` recipe that asserts migration filenames are unique by prefix number before `just check` passes.

---

### F005 — HIGH — `AuditRow` fields do not map to `audit_log` schema columns; writer will fail at runtime

**Category:** composition failure

**Attack:** The `AuditRow` acceptance criteria list fields `before_snapshot_id`, `after_snapshot_id`, `result`, `error_kind`, and `created_at_unix_seconds`. The `audit_log` schema (already in `0001_init.sql`) has `snapshot_id` (single, not before/after), `outcome` (not `result`), `occurred_at` (not `created_at_unix_seconds`), and `occurred_at_ms`. There is no `before_snapshot_id` or `after_snapshot_id` column in the schema. `AuditWriter::record` will either attempt to bind non-existent columns (SQLite runtime error on every insert) or silently drop fields (snapshot linkage lost). The acceptance criterion says "persists a single audit row in a transaction" but does not name the column mapping, so the mismatch will survive code review and only surface at integration test time.

**Why the design doesn't prevent it:** The `AuditRow` spec was written against an assumed schema, not the actual schema already committed in `0001_init.sql`. No task says "verify `AuditRow` fields are a subset of `audit_log` columns."

**Mitigation required:** Align `AuditRow` field names to `audit_log` column names verbatim. Replace `before_snapshot_id`/`after_snapshot_id` with `snapshot_id` (or add two columns via migration if both are genuinely needed). Replace `result` with `outcome`. Replace `created_at_unix_seconds` with `occurred_at`/`occurred_at_ms`. Make the column mapping explicit in the `AuditWriter::record` acceptance criterion.

---

### F006 — HIGH — Three missing `AuditEvent` variants; variant-count test will fail and block `just check`

**Category:** assumption violation

**Attack:** The TODO mandates Tier 1 MUST include `auth.bootstrap-credentials-created`, `caddy.ownership-sentinel-takeover`, and `secrets.master-key-fallback-engaged`. The existing `core/src/audit.rs` has 41 variants and does not include these three. `AUDIT_EVENT_VARIANT_COUNT = 41`. Adding three variants raises the count to 44, breaking the existing count test immediately. The design does not assign the task of updating the enum, vocab constant, or count guard to Phase 6 — it only says Tier 1 "MUST cover" them, implying they should exist, while the supporting context confirms they do not.

**Why the design doesn't prevent it:** The TODO simultaneously asserts these variants are required and the supporting context simultaneously states they are absent, but no explicit task is listed to add them.

**Mitigation required:** Add an explicit task: "Add `AuthBootstrapCredentialsCreated`, `CaddyOwnershipSentinelTakeover`, and `SecretsMasterKeyFallbackEngaged` to `AuditEvent`; update `AUDIT_KIND_VOCAB` and `AUDIT_EVENT_VARIANT_COUNT`."

---

### F007 — HIGH — Correlation ID injection is in `adapters` but `core` constructs `AuditRow`; threading undefined

**Category:** composition failure

**Attack:** The design places correlation ID propagation in a `tracing` layer in `adapters` (injected at HTTP request / scheduler tick / signal handler entry points). `AuditRow.correlation_id` must be non-null. But `AuditRow` is a `core` type with no I/O. The only way for `AuditWriter::record` (in `adapters`) to populate `correlation_id` is to extract it from the current tracing span at write time. If any call site constructs an `AuditRow` before the tracing layer has injected the ULID — in a unit test, in a background task spawned before layer attachment, or in core-layer code that creates a row directly — the field will be empty. The test "every audit row carries a non-null correlation identifier" will pass in integration tests but silently fail in unit tests or paths that bypass the layer.

**Why the design doesn't prevent it:** No task specifies how `correlation_id` flows from the tracing span into `AuditRow`. The design decomposes the problem across two crates without specifying the handoff.

**Mitigation required:** Specify explicitly: either (a) `AuditWriter::record` in `adapters` extracts the current span's correlation ID and injects it (removing it from `AuditRow`'s caller-provided fields), or (b) callers pass a `CorrelationId` value from the entry point explicitly. Define which path is canonical and add a test verifying the writer rejects a row with an empty correlation ID.

---

### F008 — HIGH — Immutability triggers added post-creation; rows written before migration are unprotected

**Category:** cascade failure

**Attack:** `0006_audit_immutable.sql` adds `UPDATE`/`DELETE` blocking triggers to `audit_log`. But the table already exists from `0001_init.sql`. Any audit rows written between first-run and migration application — in any environment where Phase 6 is deployed as an upgrade, in development with a persistent test database, or in a staged rollout — are not retroactively protected and can be mutated or deleted without triggering any error. The immutability guarantee is only as strong as the migration being applied atomically with the first write, which it is not.

**Why the design doesn't prevent it:** The design treats the trigger migration as straightforward schema work without acknowledging the temporal gap between table creation and trigger installation.

**Mitigation required:** Add an acceptance criterion: the daemon startup sequence MUST verify that the immutability triggers are present on `audit_log` before accepting any write; if absent, refuse to start. Alternatively add the triggers to `0001_init.sql` via an additive re-migration, but the startup guard is the safer path for existing deployments.

---

### F009 — HIGH — `AuditWriter::record` transaction scope undefined; business rollbacks leave orphaned audit rows

**Category:** cascade failure

**Attack:** The acceptance criterion states the writer persists a row "in a transaction." If `record` opens its own `BEGIN`/`COMMIT`, a successful audit commit followed by a business transaction rollback leaves an audit row for an event that never occurred from the application's perspective. If `record` joins the caller's transaction, the design must require callers to pass a connection handle — but no task specifies this. Every call site will independently decide, producing inconsistent audit trails.

**Why the design doesn't prevent it:** "Persists a single audit row in a transaction" is ambiguous about transaction ownership. No task defines the relationship between audit writes and the business operation they record.

**Mitigation required:** Specify explicitly: either (a) `AuditWriter::record` takes a `&mut SqliteConnection` (or equivalent transaction handle) from the caller, ensuring atomicity with the business operation, or (b) audit writes are intentionally best-effort and out-of-band. Document which model is chosen. If option (a), add a test asserting audit rows are absent when the surrounding business transaction rolls back.

---

### F010 — MEDIUM — Unbounded `redacted_diff_json` at `max 1000` rows per page; can OOM-kill the daemon

**Category:** abuse case

**Attack:** `redacted_diff_json` is an unbounded `TEXT` column. A diff against a large Caddy config can produce JSON of tens to hundreds of kilobytes. At max 1000 rows per page, a single query response could load 100–500 MB into process memory, OOM-killing the daemon on constrained hardware (Raspberry Pi, embedded server — likely deployment targets for a self-hosted Caddy manager).

**Why the design doesn't prevent it:** The query API acceptance criteria specify only count pagination. No size budget is defined for `redacted_diff_json`.

**Mitigation required:** Add a `max_diff_json_bytes` write-time constraint enforced in `AuditWriter::record` (truncate with a marker if exceeded). For the query API, add a `total_response_bytes` soft cap that terminates a page early, or reduce the effective `max` for rows carrying large diffs.

---

### F011 — MEDIUM — `SecretsRedactor` has no defined secret-field registry; identification mechanism is undefined

**Category:** assumption violation

**Attack:** The acceptance criterion says the redactor "identifies schema-marked secret fields." This requires a registry of which fields are secret. Phase 6 predates Phase 10 (secrets vault), so no such registry exists yet. If the redactor uses a static hardcoded list, it will silently miss any secret field added in a future phase. The design does not specify where this registry lives, who owns it, or how it is passed into `SecretsRedactor` without violating `core`'s no-I/O constraint. The corpus test "covers every schema-marked schema field" cannot be verified without knowing what constitutes "schema-marked."

**Why the design doesn't prevent it:** No task defines the secret-field registry, its location, or how it is passed to `SecretsRedactor`.

**Mitigation required:** Define a `SecretFieldRegistry` type in `core` — a compile-time or const set of field path strings — and make it the required constructor argument: `SecretsRedactor::new(registry: &SecretFieldRegistry)`. Document which config fields are currently secret. Make the registry the source of truth for the corpus test.

---

### F012 — MEDIUM — Correlation ID stored in shared state; concurrent requests can clobber each other's IDs

**Category:** race condition

**Attack:** The design says a tracing layer injects a ULID into "every span at the entry point." If implemented as a thread-local or global (a common Tokio + tracing mistake), concurrent async requests will overwrite each other's correlation ID at every `await` point. Request A sets ID X; Request B sets ID Y; Request A's audit writes record Y. This passes single-request tests and fails silently under load.

**Why the design doesn't prevent it:** The acceptance criterion says "a tracing layer MUST inject a ULID correlation identifier" but does not specify the storage mechanism.

**Mitigation required:** Explicitly require the correlation ID to be stored as a named field on the root span of each request/task (`tracing::info_span!("request", correlation_id = %ulid)`), so Tokio's task-local span context isolates it per async task. Add a concurrency test that fires two simultaneous requests and asserts each audit row carries its own request's correlation ID.

---

### F013 — LOW — `actor` field in `AuditRow` is untyped; system paths can forge human actor identity

**Category:** assumption violation

**Attack:** `AuditRow` carries an `actor` field with no specified type. The existing schema has `actor_kind TEXT NOT NULL` + `actor_id TEXT NOT NULL`. If `actor` is a plain `String`, any internal call site (scheduler, CLI command, bootstrap routine) can set `actor` to `"user:admin"` without authentication, forging a human actor identity in an immutable audit row.

**Why the design doesn't prevent it:** `AuditRow` acceptance criteria name `actor` as a single untyped field.

**Mitigation required:** Define an `Actor` enum in `core` with variants `User { id: String }`, `System { component: &'static str }`, `Bootstrap`, etc. Make `AuditRow.actor` take `Actor`. This makes it impossible to accidentally forge a human actor identity from a system path.
