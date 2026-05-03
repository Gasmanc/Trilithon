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
