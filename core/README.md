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
