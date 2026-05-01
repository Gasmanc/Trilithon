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
