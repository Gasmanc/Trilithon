# Trilithon Core

Rust workspace for the Trilithon daemon.

## Running the daemon

```
trilithon --config /path/to/config.toml run
```

### Exit-code table

| Code | Variant | Meaning |
|------|---------|---------|
| 0 | `CleanShutdown` | Normal exit. |
| 2 | `ConfigError` | Configuration missing, malformed, or invalid. |
| 3 | `StartupPreconditionFailure` | A startup precondition (storage, Caddy reachability) failed. |
| 64 | `InvalidInvocation` | Command-line invocation was malformed. |

## Caddy adapter

Trilithon manages a running Caddy 2.8 instance via its admin API.

### Admin endpoint

The Caddy admin endpoint is configured in `[caddy.admin_endpoint]` and supports
two transports:

| Transport | Config | Notes |
|-----------|--------|-------|
| `unix` | `path = "/run/caddy/admin.sock"` | **Default and recommended.** The daemon communicates over a Unix-domain socket. Loopback by definition. |
| `loopback_tls` | `url`, `mtls_cert_path`, `mtls_key_path`, `mtls_ca_path` | Mutual-TLS over loopback TCP. |

> **Loopback-only policy (ADR-0011):** In V1, only loopback addresses (`127.0.0.1`,
> `::1`, `localhost`) and Unix sockets are accepted. Attempting to configure a
> non-loopback endpoint causes the daemon to exit with code `2`.

### Startup sequence

On every start, before emitting `daemon.started`, the daemon:

1. **Validates the endpoint policy** — rejects non-loopback hosts (exit 2).
2. **Runs an initial capability probe** — calls `GET /config/apps`, caches the
   result, and persists it.  Caddy unreachable → exit 3.
3. **Reads or creates the installation id** — a UUID v4 stored in
   `<data_dir>/installation_id`.
4. **Ensures the ownership sentinel** — writes or verifies a
   `"trilithon-owner"` marker in the running Caddy config to prevent two
   Trilithon instances from managing the same Caddy simultaneously.
   A foreign sentinel without `--takeover` → exit 3.
5. **Spawns the reconnect loop** — monitors Caddy health every 15 s; on
   disconnect emits `caddy.disconnected` and re-probes on reconnect.

### Takeover semantics

When a second Trilithon installation is configured against a Caddy that already
carries a sentinel from a different `installation_id`, the default behavior is
to exit with code 3 (`StartupPreconditionFailure`) and log
`caddy.ownership-sentinel.conflict`.

Pass `--takeover` to overwrite the sentinel and assume ownership. An audit
event (`AuditEvent::OwnershipSentinelTakeover`) is recorded for Phase 6
processing.

## Persistence

Trilithon uses SQLite with WAL mode for persistence. The database file lives in `<data_dir>/trilithon.db`.

### Pragmas

On every pool connection: `PRAGMA journal_mode = WAL`, `PRAGMA synchronous = NORMAL`, `PRAGMA foreign_keys = ON`, `PRAGMA busy_timeout = 5000`.

### Migrations

Migration files live in `crates/adapters/migrations/`. They are embedded at compile time via `sqlx::migrate!` and run automatically on daemon startup. Migrations are **up-only** — see `crates/adapters/migrations/README.md`.

### Integrity checks

A background task runs `PRAGMA integrity_check` every 6 hours. Any non-`ok` result is logged as `storage.integrity-check.failed`.

### Advisory lock

An exclusive file lock at `<data_dir>/trilithon.lock` prevents two daemon instances from opening the same database simultaneously. The second instance exits with code `3`.

See also: [ADR-0006](docs/adr/0006-sqlite-as-v1-persistence-layer.md)

## Snapshots

Every time Trilithon reads the live Caddy configuration it produces a **snapshot** — an immutable, content-addressed record stored in the `snapshots` table.

### Canonical JSON

Before hashing, the JSON object is serialised to a canonical form so that two logically identical configurations always produce the same bytes:

- Object keys are sorted in lexicographic order at every nesting level.
- Numbers are normalised (no trailing zeros, no redundant sign).
- The serialiser version is recorded in the `CANONICAL_JSON_VERSION` constant so future format changes can be detected and migrated.

### Content addressing

The `snapshot_id` is the lowercase hex-encoded SHA-256 digest of the canonical JSON bytes — always exactly 64 characters. Storing the digest as the primary key makes deduplication trivial: two snapshots with identical content share the same id and the second write is a no-op.

### Parent linkage

Each snapshot row carries a `parent_id` foreign key that references the preceding snapshot for the same installation. The very first snapshot in a lineage stores `NULL` for `parent_id`, forming the root of the chain. This singly-linked list lets the daemon reconstruct the full configuration history and detect gaps or forks.

### Immutability guarantee

The `snapshots` table is made append-only at the database layer by `BEFORE UPDATE` and `BEFORE DELETE` triggers introduced in migration `0004`. Any attempt to mutate or remove an existing row — whether from application code or a direct SQL client — is rejected immediately with an error. This guarantee holds regardless of the calling process, so audit trails built on the snapshot chain cannot be silently tampered with.

See also: [ADR-0009](../docs/adr/0009-immutable-content-addressed-snapshots-and-audit-log.md)

## Audit log

Every significant operation — auth, config apply, drift detection, secret
reveal — produces an immutable row in the `audit_log` table.

### Single write path: `AuditWriter`

The crate-level invariant is that **only `AuditWriter::record` writes to
`audit_log`**.  No adapter, CLI handler, or background task may call
`Storage::record_audit_event` directly.  A bypass-guard integration test scans
the source tree to enforce this; production code that needs to record an event
constructs an `AuditAppend` and calls `AuditWriter::record`.

### Redactor invariant

Every `AuditAppend.diff` value is run through `SecretsRedactor` before it
reaches storage.  The redactor walks the JSON tree, replaces secret-marked
leaves with `***<hash-prefix>`, and performs a self-check that errors out if
any plaintext survived.  The fields in scope are enumerated in
`core::schema::secret_fields::TIER_1_SECRET_FIELDS`; any new schema element
that can carry secret material MUST be registered there in the same PR that
introduces it.  `notes` and `target_id` are **not** redacted and MUST NOT
contain secret material (length-bounded by `NOTES_MAX_LEN` /
`TARGET_ID_MAX_LEN`).

### Hash chain

Each audit row carries `prev_hash` — the SHA-256 of the prior row's canonical
JSON, anchored at the seed defined by `audit_prev_hash_seed`.  Insert ordering
is serialised by `BEGIN IMMEDIATE` so concurrent writers cannot fork the
chain.  Operators can verify the chain end-to-end with
`storage::helpers::verify_audit_chain`; a `Broken` verdict signals tampering
or corruption (the SQLite immutability triggers below are application-layer
guards, not a security boundary against filesystem-level edits).

### Immutability triggers

Migration `0006_audit_immutable.sql` installs `BEFORE UPDATE` and
`BEFORE DELETE` triggers on `audit_log` that abort any mutation attempt.
These triggers fire for normal SQL but can be bypassed by writers that use
`PRAGMA writable_schema = ON` or manipulate the WAL directly — that is why
`verify_audit_chain` exists.

See also: [ADR-0009](../docs/adr/0009-immutable-content-addressed-snapshots-and-audit-log.md)
