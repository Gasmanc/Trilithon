# Phase 03 — Caddy adapter and capability probe — Implementation Slices

> Phase reference: [../phases/phase-03-caddy-adapter.md](../phases/phase-03-caddy-adapter.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md) §phase-3--caddy-adapter-and-capability-probe
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference: [`../phases/phase-03-caddy-adapter.md`](../phases/phase-03-caddy-adapter.md).
- Architecture §6.13 (`capability_probe_results`), §8.1 (Caddy admin contract), §10 (failure model H1, H9, H12), §12.1 (`caddy.connected`, `caddy.disconnected`, `caddy.capability-probe.completed`, `caddy.ownership-sentinel.conflict`).
- Trait signatures: `core::caddy::CaddyClient` (§2), `core::storage::Storage` (§1).
- ADR-0001, ADR-0002, ADR-0010, ADR-0011, ADR-0013, ADR-0015.

## Slice plan summary

| Slice | Title | Primary files | Effort (h) | Depends on |
|-------|-------|---------------|------------|------------|
| 3.1 | `CaddyClient` trait, `CaddyError`, value types | `crates/core/src/caddy/{mod,client,types,error}.rs` | 5 | Phase 2 |
| 3.2 | `CaddyCapabilities` value type and capability migration `0002` | `crates/core/src/caddy/capabilities.rs`, `crates/adapters/migrations/0002_capability_probe.sql` | 3 | 3.1 |
| 3.3 | Configuration validator: loopback-only, `--allow-remote-admin` exits 2 | `crates/adapters/src/caddy/validate_endpoint.rs`, `crates/cli/src/cli.rs` | 4 | 3.1 |
| 3.4 | `HyperCaddyClient` over Unix socket and loopback-mTLS | `crates/adapters/src/caddy/hyper_client.rs` | 8 | 3.1, 3.3 |
| 3.5 | Capability probe at startup with persisted row | `crates/adapters/src/caddy/probe.rs` | 5 | 3.4 |
| 3.6 | Reconnect loop with capped exponential backoff | `crates/adapters/src/caddy/reconnect.rs` | 5 | 3.4, 3.5 |
| 3.7 | Ownership sentinel write and `--takeover` | `crates/adapters/src/caddy/sentinel.rs`, `crates/cli/src/cli.rs` | 6 | 3.4 |
| 3.8 | Wire startup; integration tests against real Caddy 2.8 | `crates/cli/src/main.rs`, `crates/adapters/tests/caddy/*.rs` | 6 | 3.5–3.7 |

Total: 8 slices.

---

## Slice 3.1 [cross-cutting] — `CaddyClient` trait, `CaddyError`, value types

### Goal

Define the `CaddyClient` trait verbatim from trait-signatures.md §2, plus the value types `CaddyConfig`, `CaddyJsonPointer`, `JsonPatch`, `LoadedModules`, `UpstreamHealth`, `TlsCertificate`, `HealthState` in `core`. The trait is async, object-safe, and free of `hyper` and `reqwest` types.

### Entry conditions

- Phase 2 complete.
- `crates/core/Cargo.toml` already declares `serde`, `serde_json`, `async-trait`, `thiserror`.

### Files to create or modify

- `core/crates/core/src/caddy/mod.rs` — re-exports (new).
- `core/crates/core/src/caddy/types.rs` — value types (new).
- `core/crates/core/src/caddy/error.rs` — `CaddyError` (new).
- `core/crates/core/src/caddy/client.rs` — trait (new).
- `core/crates/core/src/lib.rs` — `pub mod caddy;` (modify).

### Signatures and shapes

```rust
// core/crates/core/src/caddy/types.rs
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Opaque Caddy admin JSON document. Internally a `serde_json::Value`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaddyConfig(pub serde_json::Value);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CaddyJsonPointer(pub String);          // RFC 6901, must start with `/apps/`

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonPatch(pub Vec<JsonPatchOp>);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum JsonPatchOp {
    Add     { path: String, value: serde_json::Value },
    Remove  { path: String },
    Replace { path: String, value: serde_json::Value },
    Test    { path: String, value: serde_json::Value },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadedModules {
    pub modules: BTreeSet<String>,                // module identifiers, for example "http.handlers.reverse_proxy"
    pub caddy_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamHealth {
    pub address: String,
    pub healthy: bool,
    pub num_requests: u64,
    pub fails:        u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsCertificate {
    pub names: Vec<String>,
    pub not_before: i64,                          // unix seconds
    pub not_after:  i64,
    pub issuer:     String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState { Reachable, Unreachable }
```

```rust
// core/crates/core/src/caddy/error.rs (matches trait-signatures.md §2 verbatim)
#[derive(Debug, thiserror::Error)]
pub enum CaddyError {
    #[error("caddy admin endpoint unreachable: {detail}")]
    Unreachable { detail: String },
    #[error("caddy responded {status}: {body}")]
    BadStatus { status: u16, body: String },
    #[error("ownership sentinel mismatch (expected {expected}, found {found:?})")]
    OwnershipMismatch { expected: String, found: Option<String> },
    #[error("operation timed out after {seconds}s")]
    Timeout { seconds: u32 },
    #[error("caddy admin protocol violation: {detail}")]
    ProtocolViolation { detail: String },
}
```

```rust
// core/crates/core/src/caddy/client.rs
use async_trait::async_trait;
use crate::caddy::{types::*, error::CaddyError};

#[async_trait]
pub trait CaddyClient: Send + Sync + 'static {
    async fn load_config(&self, body: CaddyConfig) -> Result<(), CaddyError>;
    async fn patch_config(&self, path: CaddyJsonPointer, patch: JsonPatch) -> Result<(), CaddyError>;
    async fn get_running_config(&self) -> Result<CaddyConfig, CaddyError>;
    async fn get_loaded_modules(&self) -> Result<LoadedModules, CaddyError>;
    async fn get_upstream_health(&self) -> Result<Vec<UpstreamHealth>, CaddyError>;
    async fn get_certificates(&self) -> Result<Vec<TlsCertificate>, CaddyError>;
    async fn health_check(&self) -> Result<HealthState, CaddyError>;
}
```

### Tests

- `core/crates/core/src/caddy/client.rs` `mod tests::trait_is_pure` — compile-only, asserts `dyn CaddyClient` is object-safe and the file's `cargo deps` graph contains no `hyper`/`reqwest`.
- `core/crates/core/src/caddy/types.rs` `mod tests::serde_round_trip_loaded_modules` — round-trips a `LoadedModules` value through `serde_json`.

### Acceptance command

```
cargo test -p trilithon-core caddy::
```

### Exit conditions

- The trait, error enum, and value types compile.
- `core` manifest has not gained `hyper` or `reqwest`.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- Trait signatures §2.
- ADR-0001, ADR-0002.
- Architecture §8.1.

---

## Slice 3.2 [cross-cutting] — `CaddyCapabilities` value type and capability migration `0002`

### Goal

Add the `CaddyCapabilities` record (loaded modules + Caddy version + probe timestamp) and the `0002_capability_probe.sql` migration that creates the `capability_probe_results` table from architecture §6.13.

### Entry conditions

- Slice 3.1 complete.
- Phase 2 migration `0001_init.sql` is in place.

### Files to create or modify

- `core/crates/core/src/caddy/capabilities.rs` — `CaddyCapabilities` (new).
- `core/crates/core/src/caddy/mod.rs` — re-export (modify).
- `core/crates/adapters/migrations/0002_capability_probe.sql` — DDL (new).

### Signatures and shapes

```rust
// core/crates/core/src/caddy/capabilities.rs
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use crate::storage::types::UnixSeconds;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CaddyCapabilities {
    pub loaded_modules: BTreeSet<String>,
    pub caddy_version:  String,
    pub probed_at:      UnixSeconds,
}

/// `CapabilitySet` is the mutation-time alias used by Phase 4.
pub type CapabilitySet = CaddyCapabilities;
```

```sql
-- core/crates/adapters/migrations/0002_capability_probe.sql
CREATE TABLE capability_probe_results (
    id                  TEXT PRIMARY KEY,
    caddy_instance_id   TEXT NOT NULL REFERENCES caddy_instances(id),
    probed_at           INTEGER NOT NULL,
    caddy_version       TEXT NOT NULL,
    capabilities_json   TEXT NOT NULL,
    is_current          INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX capability_probe_results_instance ON capability_probe_results(caddy_instance_id, probed_at);
CREATE UNIQUE INDEX capability_probe_results_current
    ON capability_probe_results(caddy_instance_id) WHERE is_current = 1;
```

### Tests

- `core/crates/core/src/caddy/capabilities.rs` `mod tests::serde_round_trip` — round-trips a `CaddyCapabilities` value.
- `core/crates/core/src/caddy/capabilities.rs` `mod tests::eq_and_hash_stable` — asserts two values with identical contents compare equal and hash equal.
- `core/crates/adapters/tests/migrations_capability.rs::migration_0002_creates_table` — applies migrations and asserts `capability_probe_results` exists with the unique partial index.

### Acceptance command

```
cargo test -p trilithon-core caddy::capabilities && \
cargo test -p trilithon-adapters --test migrations_capability
```

### Exit conditions

- `CaddyCapabilities` is a public type in `core::caddy`.
- Migration `0002_capability_probe.sql` applies without error after `0001_init.sql`.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- Architecture §6.13.
- ADR-0013.

---

## Slice 3.3 [cross-cutting] — Configuration validator: loopback-only, `--allow-remote-admin` exits 2

### Goal

Validate `CaddyConfig::admin_endpoint` so that any host outside the loopback set (`127.0.0.1`, `::1`, `localhost`, or a Unix socket path) exits `3`. The CLI flag `--allow-remote-admin` exists for V1 only to print a documented refusal and exit `2`.

### Entry conditions

- Slice 3.1 complete.
- Slice 1.3 (config loader) complete.

### Files to create or modify

- `core/crates/adapters/src/caddy/validate_endpoint.rs` — validator (new).
- `core/crates/adapters/src/config_loader.rs` — invoke validator after deserialise (modify).
- `core/crates/cli/src/cli.rs` — add `--allow-remote-admin` global flag with `Command::Run` handler that exits `2` (modify).

### Signatures and shapes

```rust
// core/crates/adapters/src/caddy/validate_endpoint.rs
use trilithon_core::config::CaddyEndpoint;

pub fn validate_loopback_only(endpoint: &CaddyEndpoint) -> Result<(), EndpointPolicyError>;

#[derive(Debug, thiserror::Error)]
pub enum EndpointPolicyError {
    #[error("non-loopback admin endpoint host {host} is forbidden in V1 (ADR-0011)")]
    NonLoopback { host: String },
}
```

CLI flag (modifies slice 1.1 `Cli`):

```rust
#[arg(long, global = true)]
pub allow_remote_admin: bool,
```

### Algorithm

1. `validate_loopback_only(endpoint)`:
   1. If `Unix { .. }`, return `Ok`.
   2. If `LoopbackTls { url, .. }`, extract `url.host_str()`. Permit `Some("127.0.0.1") | Some("::1") | Some("localhost")`. Reject anything else.
2. CLI: if `allow_remote_admin` is set, write to stderr the literal `--allow-remote-admin is OUT OF SCOPE FOR V1; remove the flag and rerun.` and exit `2`.

### Tests

- `core/crates/adapters/src/caddy/validate_endpoint.rs::tests::unix_ok`.
- `tests::loopback_v4_ok`, `tests::loopback_v6_ok`, `tests::loopback_localhost_ok`.
- `tests::external_host_rejected` — `https://192.168.1.10:2019` → `NonLoopback`.
- `core/crates/cli/tests/allow_remote_admin.rs::flag_exits_2` — runs `trilithon --allow-remote-admin run`, asserts exit `2` and stderr message.

### Acceptance command

```
cargo test -p trilithon-adapters caddy::validate_endpoint && \
cargo test -p trilithon-cli --test allow_remote_admin
```

### Exit conditions

- Loopback-only enforcement is observable on every config load.
- `--allow-remote-admin` exits `2` with the documented message.
- Five named tests pass.

### Audit kinds emitted

None.

### Tracing events emitted

None at this slice; `caddy.disconnected`-class events arrive in 3.6.

### Cross-references

- ADR-0011 (mitigates H1).
- Phase reference: "Reject non-loopback admin endpoints by configuration validation".

---

## Slice 3.4 [cross-cutting] — `HyperCaddyClient` over Unix socket and loopback-mTLS

### Goal

Implement `HyperCaddyClient` over `hyper` 1.x with two transport variants: a Unix-socket connector (`hyperlocal`) and a loopback-mTLS connector (`hyper-rustls`). Every admin call carries the active `traceparent` header derived from the current `tracing::Span` correlation identifier.

### Entry conditions

- Slices 3.1, 3.3 complete.
- `crates/adapters/Cargo.toml` declares `hyper = { version = "1", features = ["client", "http1"] }`, `hyper-util = { version = "0.1", features = ["client", "client-legacy"] }`, `hyperlocal = "0.9"`, `hyper-rustls = "0.27"`, `rustls-pemfile = "2"`, `http-body-util = "0.1"`, `serde_json = "1"`, `bytes = "1"`.

### Files to create or modify

- `core/crates/adapters/src/caddy/hyper_client.rs` — adapter (new).
- `core/crates/adapters/src/caddy/traceparent.rs` — derive `traceparent` from current span (new).
- `core/crates/adapters/src/caddy/mod.rs` — module wiring (new).
- `core/crates/adapters/src/lib.rs` — `pub mod caddy;` (modify).

### Signatures and shapes

```rust
// core/crates/adapters/src/caddy/hyper_client.rs
use trilithon_core::caddy::{client::CaddyClient, error::CaddyError, types::*};
use trilithon_core::config::CaddyEndpoint;
use std::time::Duration;

pub struct HyperCaddyClient {
    inner:           Inner,
    connect_timeout: Duration,
    apply_timeout:   Duration,
}

enum Inner {
    Unix     { socket_path: std::path::PathBuf },
    LoopbackTls {
        client: hyper_util::client::legacy::Client<hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>, http_body_util::Full<bytes::Bytes>>,
        base_url: url::Url,
    },
}

impl HyperCaddyClient {
    pub fn from_config(
        endpoint: &CaddyEndpoint,
        connect_timeout: Duration,
        apply_timeout: Duration,
    ) -> Result<Self, CaddyError>;
}

#[async_trait::async_trait]
impl CaddyClient for HyperCaddyClient { /* every method */ }
```

```rust
// core/crates/adapters/src/caddy/traceparent.rs
/// Render a W3C traceparent header from the active correlation identifier.
/// Format: 00-<32 hex>-<16 hex>-01. Trilithon uses the correlation id
/// (ULID -> 26 chars) padded/truncated to 32 hex chars; span id is a fresh
/// 64-bit random.
pub fn current_traceparent() -> String;
```

### Algorithm

`HyperCaddyClient::from_config`:

1. On `CaddyEndpoint::Unix { path }` build `Inner::Unix { socket_path: path.clone() }`.
2. On `CaddyEndpoint::LoopbackTls { url, mtls_cert_path, mtls_key_path, mtls_ca_path }`:
   1. Load CA pem via `rustls_pemfile::certs`.
   2. Load client cert + key.
   3. Build a `rustls::ClientConfig` with the supplied CA root and a single client identity.
   4. Wrap with `hyper_rustls::HttpsConnectorBuilder`.
   5. Construct `hyper_util::client::legacy::Client::builder(...)` over the connector.

Per-method:

- `load_config(body)` — POST `/load` with `Content-Type: application/json` and the canonical JSON; `Accept: application/json`; `traceparent: <current_traceparent()>`.
- `patch_config(path, patch)` — PATCH against `/config<path>` carrying the JSON-Patch body. Path validation: reject if `!path.0.starts_with("/apps/")` with `CaddyError::ProtocolViolation { detail: "path must start with /apps/" }`.
- `get_running_config()` — GET `/config/`; deserialise into `CaddyConfig(serde_json::Value)`.
- `get_loaded_modules()` — GET `/config/apps`; iterate top-level keys and recursively walk `module` discriminators to populate `LoadedModules.modules`. Read `caddy_version` from `GET /` (server header) or by parsing the response body's `_modules` reflection if available; fallback to a constant `"unknown"`.
- `get_upstream_health()` — GET `/reverse_proxy/upstreams`.
- `get_certificates()` — GET `/pki/ca/local/certificates` (requires PKI app loaded; if 404, return `Ok(vec![])`).
- `health_check()` — GET `/`; any 2xx → `Reachable`; transport error → `Unreachable`.

Every method enforces the per-call timeout from `connect_timeout` and `apply_timeout` via `tokio::time::timeout`. On expiry, return `CaddyError::Timeout { seconds }`.

### Tests

- `core/crates/adapters/src/caddy/hyper_client.rs::tests::traceparent_header_present` — uses an `httptest::Server` pretending to be Caddy; asserts every request carries a `traceparent` header.
- `core/crates/adapters/src/caddy/hyper_client.rs::tests::patch_path_must_start_with_apps`.
- `core/crates/adapters/src/caddy/hyper_client.rs::tests::transport_timeout_maps_to_timeout_variant`.
- `core/crates/adapters/tests/caddy/hyper_real_caddy.rs::round_trip_load_then_get` (gated on env var `TRILITHON_E2E_CADDY=1`) — launches Caddy 2.8 binary on a temp Unix socket, posts a minimal JSON config, GETs it back, asserts equality.

### Acceptance command

```
cargo test -p trilithon-adapters caddy::hyper_client && \
TRILITHON_E2E_CADDY=1 cargo test -p trilithon-adapters --test caddy/hyper_real_caddy
```

### Exit conditions

- Both transports return `HealthState::Reachable` against a real Caddy.
- Every method emits a `traceparent` header.
- The four tests pass.

### Audit kinds emitted

None directly.

### Tracing events emitted

None at this slice; the reconnect loop in 3.6 emits `caddy.connected`/`caddy.disconnected`.

### Cross-references

- Trait signatures §2, "Correlation propagation" cross-trait invariant.
- Architecture §8.1.
- ADR-0002, ADR-0010.

---

## Slice 3.5 [standard] — Capability probe at startup with persisted row

### Goal

Run the capability probe once at startup. Persist a `capability_probe_results` row with `is_current = 1` (after demoting any previously current row). Cache the resulting `CaddyCapabilities` in an `Arc<RwLock<Option<CaddyCapabilities>>>` shared across the daemon. Probe completion MUST happen within one second of Caddy connectivity.

### Entry conditions

- Slices 3.1, 3.2, 3.4 complete.

### Files to create or modify

- `core/crates/adapters/src/caddy/probe.rs` — probe runner (new).
- `core/crates/adapters/src/caddy/capability_store.rs` — DB persistence (new).
- `core/crates/adapters/src/caddy/cache.rs` — in-memory cache (new).

### Signatures and shapes

```rust
// core/crates/adapters/src/caddy/probe.rs
use std::sync::Arc;
use trilithon_core::caddy::{client::CaddyClient, capabilities::CaddyCapabilities};

pub async fn run_initial_probe(
    client:      &dyn CaddyClient,
    cache:       Arc<crate::caddy::cache::CapabilityCache>,
    persistence: &crate::caddy::capability_store::CapabilityStore,
    instance_id: &str,
) -> Result<CaddyCapabilities, ProbeError>;

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("caddy error during probe: {source}")]
    Caddy { #[from] source: trilithon_core::caddy::error::CaddyError },
    #[error("storage error during probe: {source}")]
    Storage { #[from] source: trilithon_core::storage::StorageError },
}
```

```rust
// core/crates/adapters/src/caddy/cache.rs
use parking_lot::RwLock;
use std::sync::Arc;
use trilithon_core::caddy::capabilities::CaddyCapabilities;

#[derive(Default)]
pub struct CapabilityCache {
    inner: RwLock<Option<CaddyCapabilities>>,
}

impl CapabilityCache {
    pub fn snapshot(&self) -> Option<CaddyCapabilities>;
    pub fn replace(&self, value: CaddyCapabilities);
}
```

```rust
// core/crates/adapters/src/caddy/capability_store.rs
use sqlx::SqlitePool;

pub struct CapabilityStore { pool: SqlitePool }

impl CapabilityStore {
    pub fn new(pool: SqlitePool) -> Self { Self { pool } }
    pub async fn record_current(
        &self,
        instance_id: &str,
        caps: &trilithon_core::caddy::capabilities::CaddyCapabilities,
    ) -> Result<(), trilithon_core::storage::StorageError>;
}
```

### Algorithm

`run_initial_probe`:

1. `let modules = client.get_loaded_modules().await?;` — propagates `CaddyError`.
2. `let now = time::OffsetDateTime::now_utc().unix_timestamp();`
3. `let caps = CaddyCapabilities { loaded_modules: modules.modules, caddy_version: modules.caddy_version, probed_at: now };`
4. `cache.replace(caps.clone());`
5. `persistence.record_current(instance_id, &caps).await?;`
6. Emit `tracing::info!("caddy.capability-probe.completed", caddy.endpoint = ..., caddy_version = %caps.caddy_version, module_count = caps.loaded_modules.len());`
7. Return `Ok(caps)`.

`CapabilityStore::record_current` runs inside a transaction:

1. `UPDATE capability_probe_results SET is_current = 0 WHERE caddy_instance_id = ? AND is_current = 1`.
2. `INSERT INTO capability_probe_results (id, caddy_instance_id, probed_at, caddy_version, capabilities_json, is_current) VALUES (?, ?, ?, ?, ?, 1)` with a fresh ULID.

### Tests

- `core/crates/adapters/src/caddy/probe.rs::tests::probe_emits_event_and_caches` — uses a `CaddyClientDouble` returning a fixed `LoadedModules`, asserts the cache snapshot matches and `caddy.capability-probe.completed` is emitted.
- `core/crates/adapters/tests/caddy/probe_persisted.rs::probe_writes_current_row` — opens a real SQLite, runs probe, asserts a single row with `is_current = 1`. Runs probe again, asserts two rows total and exactly one with `is_current = 1`.
- `core/crates/adapters/tests/caddy/probe_under_one_second.rs::probe_within_one_second` (gated `TRILITHON_E2E_CADDY=1`) — launches Caddy 2.8, connects, asserts `run_initial_probe` returns within 1000 ms.

### Acceptance command

```
cargo test -p trilithon-adapters caddy::probe && \
cargo test -p trilithon-adapters --test caddy/probe_persisted
```

### Exit conditions

- Cache snapshot is non-empty after probe.
- Exactly one `is_current = 1` row per `caddy_instance_id` after any number of probes.
- The capability-probe-completed event has the documented field keys.

### Audit kinds emitted

`caddy.capability-probe-completed` (architecture §6.6) — written by Phase 6 audit writer; not in scope here, but the in-memory `AuditEvent::CaddyCapabilityProbeCompleted` enum variant is constructed and held for Phase 6 to flush.

### Tracing events emitted

- `caddy.capability-probe.completed` (architecture §12.1).

### Cross-references

- Trait signatures §2.
- Architecture §6.13, §7.4 (capability probe), §12.1.
- ADR-0013.

---

## Slice 3.6 [cross-cutting] — Reconnect loop with capped exponential backoff

### Goal

Run a background loop that monitors Caddy reachability via `health_check()`. On disconnect, emit `caddy.disconnected` once. Reconnect attempts MUST start at 250 ms and double until 30 seconds, then plateau. On a successful reconnection, emit `caddy.connected` and trigger a fresh capability probe.

### Entry conditions

- Slices 3.4, 3.5 complete.

### Files to create or modify

- `core/crates/adapters/src/caddy/reconnect.rs` — loop (new).

### Signatures and shapes

```rust
// core/crates/adapters/src/caddy/reconnect.rs
use std::sync::Arc;
use std::time::Duration;
use trilithon_core::caddy::{client::CaddyClient, types::HealthState};

pub const INITIAL_BACKOFF: Duration = Duration::from_millis(250);
pub const MAX_BACKOFF:     Duration = Duration::from_secs(30);

pub async fn reconnect_loop(
    client:           Arc<dyn CaddyClient>,
    cache:            Arc<crate::caddy::cache::CapabilityCache>,
    persistence:      crate::caddy::capability_store::CapabilityStore,
    instance_id:      String,
    mut shutdown:     impl ShutdownObserver,
);

pub trait ShutdownObserver: Send + 'static {
    /// Resolve when shutdown is signalled.
    fn changed(&mut self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>;
    fn is_shutting_down(&self) -> bool;
}
```

### Algorithm

1. `let mut state = HealthState::Reachable;` (assumes startup probe succeeded).
2. `let mut backoff = INITIAL_BACKOFF;`
3. Loop forever:
   1. `tokio::select!` on a `tokio::time::sleep(15_seconds_health_interval)` and `shutdown.changed()`.
   2. On shutdown: `break`.
   3. On tick: `match client.health_check().await`:
      - `Ok(Reachable)`:
        - If `state == Unreachable`: emit `caddy.connected`; trigger a fresh probe via `run_initial_probe`; reset `backoff = INITIAL_BACKOFF`.
        - Set `state = Reachable`.
      - `Ok(Unreachable) | Err(_)`:
        - If `state == Reachable`: emit `caddy.disconnected`.
        - Set `state = Unreachable`.
        - `tokio::time::sleep(backoff).await`.
        - `backoff = std::cmp::min(backoff * 2, MAX_BACKOFF)`.

### Tests

- `core/crates/adapters/src/caddy/reconnect.rs::tests::backoff_doubles_then_caps` — uses a stub clock; asserts the sequence 250 ms, 500 ms, 1 s, 2 s, 4 s, 8 s, 16 s, 30 s, 30 s, 30 s.
- `core/crates/adapters/tests/caddy/reconnect_against_killed_caddy.rs::observes_fresh_probe_within_35s` (gated `TRILITHON_E2E_CADDY=1`) — kills Caddy mid-loop, restarts after 5 s, asserts a fresh `caddy.capability-probe.completed` event within 35 s of restart.

### Acceptance command

```
cargo test -p trilithon-adapters caddy::reconnect
```

### Exit conditions

- Backoff schedule matches the documented sequence.
- Fresh probe runs on every reconnect.
- Events `caddy.connected`/`caddy.disconnected` are emitted exactly once per state transition.

### Audit kinds emitted

`caddy.unreachable` and `caddy.reconnected` (architecture §6.6) — held in memory; flushed by Phase 6.

### Tracing events emitted

- `caddy.connected` and `caddy.disconnected` (architecture §12.1).
- `caddy.capability-probe.completed` (re-emitted on each reconnect).

### Cross-references

- Architecture §10, §12.1.
- ADR-0013.
- Phase reference: "Reconnect loop with exponential backoff capped at 30 seconds".

---

## Slice 3.7 [cross-cutting] — Ownership sentinel write and `--takeover`

### Goal

At startup, after the initial capability probe, read Caddy's running config and locate the JSON node at `/apps/http/servers/<key>/@id == "trilithon-owner"` (or by walking for an `@id` of value `"trilithon-owner"` anywhere). If absent, write a sentinel block carrying `{"@id": "trilithon-owner", "installation_id": <our-uuid>}` via `patch_config`. If present and the embedded `installation_id` differs, exit `3` with a human-readable error referencing the conflicting identifier — UNLESS `--takeover` is set, in which case overwrite and stage an `OwnershipSentinelTakeover` audit row for Phase 6.

### Entry conditions

- Slices 3.4, 3.5 complete.
- A daemon installation identifier exists at `<data_dir>/installation_id`. If absent, generate a fresh UUID and persist it.

### Files to create or modify

- `core/crates/adapters/src/caddy/sentinel.rs` — sentinel logic (new).
- `core/crates/adapters/src/caddy/installation_id.rs` — read-or-create installation id (new).
- `core/crates/cli/src/cli.rs` — `--takeover` flag on `Command::Run` (modify).

### Signatures and shapes

```rust
// core/crates/adapters/src/caddy/sentinel.rs
use trilithon_core::caddy::{client::CaddyClient, types::{CaddyJsonPointer, JsonPatch, JsonPatchOp}};

pub const SENTINEL_ID: &str = "trilithon-owner";

pub async fn ensure_sentinel(
    client:          &dyn CaddyClient,
    installation_id: &str,
    takeover:        bool,
) -> Result<SentinelOutcome, SentinelError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SentinelOutcome {
    Created,
    AlreadyOurs,
    TookOver { previous_installation_id: String },
}

#[derive(Debug, thiserror::Error)]
pub enum SentinelError {
    #[error("ownership sentinel conflict: caddy carries installation_id {found}, ours is {ours}")]
    Conflict { found: String, ours: String },
    #[error("caddy error: {source}")]
    Caddy { #[from] source: trilithon_core::caddy::error::CaddyError },
}
```

```rust
// core/crates/adapters/src/caddy/installation_id.rs
pub fn read_or_create(data_dir: &std::path::Path) -> Result<String, std::io::Error>;
```

CLI flag: add `--takeover` (`bool`, default `false`) to `Command::Run`.

### Algorithm

1. `read_or_create(data_dir)` — read `<data_dir>/installation_id`. If missing, generate `uuid::Uuid::new_v4()` (full hyphenated form) and write atomically (create + rename).
2. `ensure_sentinel(client, ours, takeover)`:
   1. `let cfg = client.get_running_config().await?;`
   2. Walk `cfg.0` recursively; collect all objects whose `"@id"` equals `"trilithon-owner"`.
   3. Cases:
      - **None found**: build a sentinel block at `/apps/http/servers/__trilithon_sentinel__` containing `{"@id": "trilithon-owner", "installation_id": ours}` and apply via `patch_config(CaddyJsonPointer("/apps/http/servers/__trilithon_sentinel__"), JsonPatch(vec![JsonPatchOp::Add { path: "/apps/http/servers/__trilithon_sentinel__".into(), value: ... }]))`. Return `Created`.
      - **One found, `installation_id == ours`**: return `AlreadyOurs`.
      - **One found, `installation_id != ours`, `takeover == false`**: emit `tracing::error!("caddy.ownership-sentinel.conflict", expected = %ours, found = %previous);`, return `SentinelError::Conflict { found: previous, ours }`.
      - **One found, `installation_id != ours`, `takeover == true`**: replace via `patch_config(... Replace { path: ".../installation_id", value: ours })`. Return `TookOver { previous_installation_id: previous }`.
3. CLI maps `SentinelError::Conflict` to exit code `3`.

### Tests

- `core/crates/adapters/src/caddy/sentinel.rs::tests::creates_when_absent` — `CaddyClientDouble` returns a config without sentinel; asserts `SentinelOutcome::Created` and the patch payload.
- `tests::already_ours_no_op` — config carries our sentinel; asserts `AlreadyOurs` and zero patch calls.
- `tests::conflict_without_takeover_errors` — config carries a foreign sentinel, `takeover = false`; asserts `Conflict { found: "deadbeef", ours: "ours-id" }` and that `caddy.ownership-sentinel.conflict` event was emitted.
- `tests::takeover_overwrites` — config carries a foreign sentinel, `takeover = true`; asserts `TookOver` and a Replace patch.
- `core/crates/adapters/tests/caddy/sentinel_e2e.rs::foreign_sentinel_exits_3` (gated `TRILITHON_E2E_CADDY=1`) — boots a Caddy with a hand-crafted foreign sentinel, runs the binary without `--takeover`, asserts exit `3`.

### Acceptance command

```
cargo test -p trilithon-adapters caddy::sentinel
```

### Exit conditions

- Foreign sentinel collision exits `3` with a message containing the conflicting identifier.
- `--takeover` overwrites and the audit-row stub is captured for Phase 6 (this slice asserts the in-memory `AuditEvent::OwnershipSentinelTakeover` value is constructed; Phase 6 will append it to the audit log).

### Audit kinds emitted

- `caddy.ownership-sentinel-conflict` (architecture §6.6) — staged for Phase 6 emission. The `--takeover` path stages a corresponding takeover row; the `kind` for the takeover row is `caddy.ownership-sentinel-conflict` with `outcome = "ok"` and `notes` containing both identifiers. **Open question 4**: is `caddy.ownership-sentinel-conflict` the right kind for a successful `--takeover`, or does §6.6 need a new `caddy.ownership-sentinel-takeover` kind? Architecture §6.6 does not currently include that kind; flagging.

### Tracing events emitted

- `caddy.ownership-sentinel.conflict` (architecture §12.1).

### Cross-references

- ADR-0015 (ownership sentinel).
- Architecture §6.6, §12.1.
- Phase reference: "Implement the ownership sentinel write", "Implement the `--takeover` override with audit".

---

## Slice 3.8 [cross-cutting] — Wire startup; integration tests against real Caddy 2.8

### Goal

Compose the components: build `HyperCaddyClient` from config; run initial probe; ensure sentinel; spawn reconnect loop; emit `daemon.started` only after sentinel resolves. Document loopback default and takeover semantics in `core/README.md`.

### Entry conditions

- Slices 3.1 through 3.7 complete.
- Caddy 2.8 binary is reachable in CI runners (cached at `/usr/local/bin/caddy` or `$CADDY` env var).

### Files to create or modify

- `core/crates/cli/src/main.rs` — wire Caddy startup (modify).
- `core/crates/cli/src/exit.rs` — map `SentinelError::Conflict` and `EndpointPolicyError::NonLoopback` to `ExitCode::StartupPreconditionFailure` (modify).
- `core/crates/adapters/tests/caddy/end_to_end.rs` — full path test (new).
- `core/README.md` — "Caddy adapter" section (modify).

### Signatures and shapes

```rust
// core/crates/cli/src/main.rs (excerpt, after storage init)
let caddy_client = std::sync::Arc::new(
    trilithon_adapters::caddy::hyper_client::HyperCaddyClient::from_config(
        &config.caddy.admin_endpoint,
        std::time::Duration::from_secs(config.caddy.connect_timeout_seconds.into()),
        std::time::Duration::from_secs(config.caddy.apply_timeout_seconds.into()),
    )?
);
let cap_cache = std::sync::Arc::new(trilithon_adapters::caddy::cache::CapabilityCache::default());
let cap_store = trilithon_adapters::caddy::capability_store::CapabilityStore::new(storage.pool().clone());
trilithon_adapters::caddy::probe::run_initial_probe(&*caddy_client, cap_cache.clone(), &cap_store, "local").await?;
let installation_id = trilithon_adapters::caddy::installation_id::read_or_create(&config.storage.data_dir)?;
trilithon_adapters::caddy::sentinel::ensure_sentinel(&*caddy_client, &installation_id, cli.takeover).await?;
tokio::spawn(trilithon_adapters::caddy::reconnect::reconnect_loop(
    caddy_client.clone(), cap_cache.clone(), cap_store, "local".into(), shutdown.clone(),
));
tracing::info!("daemon.started");
```

### Tests

- `core/crates/adapters/tests/caddy/end_to_end.rs::happy_path_against_real_caddy` (gated `TRILITHON_E2E_CADDY=1`):
  1. Boots Caddy 2.8 with a minimal config and a Unix admin socket in a temp dir.
  2. Runs the daemon binary with `data_dir` set to a temp directory.
  3. Asserts the binary's stderr contains, in order: `caddy.capability-probe.completed`, the sentinel write (no error), `daemon.started`.
  4. Sends `SIGTERM`, asserts exit `0`.
- `core/crates/cli/tests/storage_startup.rs::caddy_unreachable_exits_3` (extends Phase 2 file) — points the daemon at a non-existent Unix socket, asserts exit `3` and stderr names `caddy.unreachable` or the typed error.

### Acceptance command

```
TRILITHON_E2E_CADDY=1 cargo test -p trilithon-adapters --test caddy/end_to_end && \
cargo test -p trilithon-cli --test storage_startup caddy_unreachable_exits_3
```

### Exit conditions

- The daemon emits `daemon.started` only after every Phase 3 startup gate has passed.
- Caddy unreachable at startup exits `3`.
- Foreign sentinel exits `3`.
- The end-to-end test passes against real Caddy.

### Audit kinds emitted

- `caddy.capability-probe-completed` (staged for Phase 6).
- `caddy.ownership-sentinel-conflict` (on conflict path; staged for Phase 6).

### Tracing events emitted

- `caddy.capability-probe.completed`, `caddy.connected`, `caddy.disconnected`, `caddy.ownership-sentinel.conflict`, `daemon.started`.

### Cross-references

- ADR-0001, ADR-0002, ADR-0011, ADR-0013, ADR-0015.
- Architecture §6.6, §6.13, §8.1, §10, §12.1.
- Phase reference §"Sign-off checklist".

---

## Phase exit checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] Trilithon refuses to start with exit code `3` if the Caddy admin endpoint configuration points to a non-loopback address; `--allow-remote-admin` itself exits `2`.
- [ ] The capability probe result is available to the rest of the daemon within one second of Caddy connectivity.
- [ ] An ownership sentinel collision exits `3` with a human-readable error referencing the conflicting installation identifier.
- [ ] All Caddy admin calls carry the active correlation identifier in a `traceparent` header.
- [ ] Open question 4 (takeover audit `kind`) is resolved before Phase 6.
