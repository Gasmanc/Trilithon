# Trilithon Workspace Trait Signatures

## Document control

| Field | Value |
| --- | --- |
| Version | 1.0.0 |
| Date | 2026-04-30 |
| Status | Stable |

This document is the single source of truth for every Rust trait surface in the Trilithon workspace. Each trait listed below appears here with its full async signature, including ownership, error types, and lifetimes. Phase TODOs that introduce a trait MUST cross-reference this file rather than restate signatures inline; if the two ever drift, this document is authoritative.

## Glossary

For canonical terms and actor classifications, see `docs/prompts/PROMPT-spec-generation.md` §3 and the architecture §2 glossary in `docs/architecture/architecture.md`. The trait-level error variants below are concrete `thiserror`-derived enums and are not redefined in the glossary.

## Conventions

- All traits below are object-safe unless explicitly noted otherwise. Object-safety is a hard requirement because adapters are stored behind `Arc<dyn Trait>` for the duration of the daemon process.
- All errors are `thiserror`-derived enums named `<Trait>Error`. Each variant carries enough structured context to satisfy the audit log redactor without further enrichment at the call site.
- All async traits use `async-trait` v0.1 with the `#[async_trait]` attribute. The crate is on the workspace dependency list at `core/Cargo.toml`.
- All lifetimes default to `'static` for stored handles. Where an explicit `<'a>` is shown, the lifetime is bound to a single call.
- Method receiver style: `&self` for read-only trait methods, `&mut self` only when the trait body is genuinely mutable, never `self` (consuming) on object-safe trait methods.
- Result types are `Result<T, <Trait>Error>` unless the method is infallible (for example, boolean probes) in which case the return type is documented inline.

## Trait surface

Sections appear in dependency order: storage, then Caddy, then secrets, then policy, then diff, then reconciler, then tool gateway, then probe, then config, then HTTP, then Docker.

### 1. `core::storage::Storage`

Home crate: `core::storage` in `crates/core/src/storage.rs`. Default implementation: `crates/adapters/src/storage_sqlite.rs`.

The persistent store boundary. Wraps SQLite for V1 but is deliberately backend-agnostic: the trait method set is the union of operations Trilithon needs against any persistent store. Every write method records exactly one row at the boundary; transactional grouping happens through dedicated `with_transaction` helpers on the concrete adapter, not through the trait. Cross-references: architecture §6 data model for row shapes; architecture §6.6 for the audit `kind` vocabulary the audit-row writers honour.

```rust
#[async_trait]
pub trait Storage: Send + Sync + 'static {
    /// Insert a new immutable snapshot. Returns the inserted snapshot id.
    /// Acceptance: the row is rejected if `snapshot.id` already exists or the
    /// content hash does not match the canonical-JSON SHA-256.
    async fn insert_snapshot(
        &self,
        snapshot: Snapshot,
    ) -> Result<SnapshotId, StorageError>;

    /// Fetch a snapshot by id. Returns `None` only when the id is unknown.
    /// Acceptance: never returns a partial row; integrity check fails fast.
    async fn get_snapshot(
        &self,
        id: &SnapshotId,
    ) -> Result<Option<Snapshot>, StorageError>;

    /// Walk the parent chain of a snapshot, oldest first.
    /// Acceptance: terminates at the genesis snapshot or at a missing parent
    /// pointer (returns the chain seen so far plus a `Truncated` flag).
    async fn parent_chain(
        &self,
        leaf: &SnapshotId,
        max_depth: usize,
    ) -> Result<ParentChain, StorageError>;

    /// Return the latest desired-state snapshot referenced by `desired_state`.
    /// Acceptance: returns `None` only on first run, before bootstrap.
    async fn latest_desired_state(
        &self,
    ) -> Result<Option<Snapshot>, StorageError>;

    /// Append a single audit event row.
    /// Acceptance: the `kind` string MUST be in the architecture §6.6
    /// vocabulary; rejected with `StorageError::AuditKindUnknown` otherwise.
    async fn record_audit_event(
        &self,
        event: AuditEventRow,
    ) -> Result<AuditRowId, StorageError>;

    /// Stream audit rows in reverse chronological order, filtered by the
    /// provided selector. Used by the audit viewer and by forensic queries.
    async fn tail_audit_log(
        &self,
        selector: AuditSelector,
        limit: u32,
    ) -> Result<Vec<AuditEventRow>, StorageError>;

    /// Append a drift detection row. Pairs with `latest_drift_event` for the
    /// dashboard's "currently drifted" indicator.
    async fn record_drift_event(
        &self,
        event: DriftEventRow,
    ) -> Result<DriftRowId, StorageError>;

    /// Return the latest drift event for the current desired-state snapshot.
    async fn latest_drift_event(
        &self,
    ) -> Result<Option<DriftEventRow>, StorageError>;

    /// Insert a proposal into the queue.
    /// Acceptance: returns `StorageError::ProposalDuplicate` if an open
    /// proposal with the same `(source, source_ref)` already exists.
    async fn enqueue_proposal(
        &self,
        proposal: ProposalRow,
    ) -> Result<ProposalId, StorageError>;

    /// Atomically claim and return the next pending proposal.
    /// Acceptance: returns `None` if no proposal is currently pending.
    async fn dequeue_proposal(
        &self,
    ) -> Result<Option<ProposalRow>, StorageError>;

    /// Sweep proposals whose expiry has passed; transition to `expired`
    /// and return the count.
    async fn expire_proposals(
        &self,
        now: UnixSeconds,
    ) -> Result<u32, StorageError>;

    /// Replace the TLS certificate inventory with the supplied set. Used by
    /// the Phase 14 TLS inventory refresher. The implementation MUST be a
    /// transactional upsert keyed on `(host, issuer)`; rows for hosts not in
    /// the supplied set are deleted in the same transaction.
    /// Acceptance: after success, `list_tls_certificates` returns exactly
    /// the supplied set, sorted by host.
    async fn upsert_tls_certificates(
        &self,
        certs: Vec<TlsCertificate>,
    ) -> Result<(), StorageError>;

    /// Return the current TLS certificate inventory, sorted by host.
    /// Acceptance: returns the empty vector before the first refresh.
    async fn list_tls_certificates(
        &self,
    ) -> Result<Vec<TlsCertificate>, StorageError>;

    /// Append upstream-health observation rows. Append-only; old rows are
    /// pruned by a background job (Phase 14) per the retention policy in
    /// architecture §6 `upstream_health` table.
    /// Acceptance: rows whose `(route_id, upstream_id, observed_at)` triple
    /// already exists are silently deduplicated.
    async fn upsert_upstream_health(
        &self,
        rows: Vec<UpstreamHealthRow>,
    ) -> Result<(), StorageError>;

    /// Return the most recent upstream-health observation per
    /// `(route_id, upstream_id)`.
    /// Acceptance: returns the empty vector before the first probe.
    async fn list_upstream_health(
        &self,
    ) -> Result<Vec<UpstreamHealthRow>, StorageError>;
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("integrity check failed: {detail}")]
    Integrity { detail: String },
    #[error("audit kind {kind} is not in the §6.6 vocabulary")]
    AuditKindUnknown { kind: String },
    #[error("snapshot {id} already exists")]
    SnapshotDuplicate { id: SnapshotId },
    #[error("proposal duplicate for ({source}, {source_ref})")]
    ProposalDuplicate { source: String, source_ref: String },
    #[error("sqlite busy after {retries} retries")]
    SqliteBusy { retries: u32 },
    #[error("sqlite error: {kind:?}")]
    Sqlite { kind: SqliteErrorKind },
    #[error("schema migration {version} failed: {detail}")]
    Migration { version: u32, detail: String },
    #[error("io error: {source}")]
    Io { #[source] source: std::io::Error },
}

/// A TLS certificate as observed via `GET /pki/ca/local/certificates` and
/// `GET /config/apps/tls/certificates` on the Caddy admin API. Persisted in
/// the `tls_certificates` table (architecture §6.13). Hosts use the form
/// stored in Caddy: a wildcard host begins with `*.`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TlsCertificate {
    pub host:           String,
    pub issuer:         String,
    pub not_before:     UnixSeconds,
    pub not_after:      UnixSeconds,
    pub renewal_state:  RenewalState,                // Pending | Renewed | Failed { detail }
    pub source:         CertificateSource,           // Acme | Internal | Imported
    pub fetched_at:     UnixSeconds,
}

/// A single upstream-health observation. Persisted in the `upstream_health`
/// table (architecture §6.14). Each row records the result of one probe
/// (TCP connect, HTTP health, or Caddy `/reverse_proxy/upstreams` flip).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpstreamHealthRow {
    pub route_id:       RouteId,
    pub upstream_id:    UpstreamId,
    pub observed_at:    UnixSeconds,
    pub reachable:      bool,
    pub latency_ms:     Option<u32>,                 // None when unreachable
    pub source:         HealthSource,                // TcpProbe | HttpProbe | CaddyApi
    pub detail:         Option<String>,              // free-text classification
}
```

### 2. `core::caddy::CaddyClient`

Home crate: trait at `core::caddy::client` in `crates/core/src/caddy/client.rs`. Default implementation: `crates/adapters/src/caddy_http.rs`. Cross-references: ADR-0002, ADR-0010, ADR-0011, architecture §8.1.

The Caddy admin API boundary. Every method propagates the correlation identifier through the `traceparent` header. Methods are non-mutating except `load_config` and `patch_config`. The `load_config` method writes a complete Caddy JSON document; `patch_config` issues a JSON-Patch against a specific path.

```rust
#[async_trait]
pub trait CaddyClient: Send + Sync + 'static {
    /// Replace the running configuration with the supplied document.
    /// Acceptance: returns Ok only when Caddy responds 200; any non-2xx
    /// becomes `CaddyError::BadStatus`.
    async fn load_config(
        &self,
        body: CaddyConfig,
    ) -> Result<(), CaddyError>;

    /// Apply a JSON-Patch document against a path inside the running config.
    /// Acceptance: the path MUST start with `/apps/`. The method refuses
    /// patches against paths owned by Caddy (TLS issuance state, upstream
    /// health caches) with `CaddyError::OwnershipMismatch`.
    async fn patch_config(
        &self,
        path: CaddyJsonPointer,
        patch: JsonPatch,
    ) -> Result<(), CaddyError>;

    /// Fetch the running configuration as Caddy understands it.
    async fn get_running_config(
        &self,
    ) -> Result<CaddyConfig, CaddyError>;

    /// Fetch the loaded module set from Caddy's reflective endpoint.
    /// Drives capability gating (Phase 4 + ADR-0013).
    async fn get_loaded_modules(
        &self,
    ) -> Result<LoadedModules, CaddyError>;

    /// Fetch upstream health snapshots for active reverse-proxy routes.
    async fn get_upstream_health(
        &self,
    ) -> Result<Vec<UpstreamHealth>, CaddyError>;

    /// Fetch certificate inventory from Caddy's PKI endpoints.
    async fn get_certificates(
        &self,
    ) -> Result<Vec<TlsCertificate>, CaddyError>;

    /// Lightweight reachability check. Returns Ok when Caddy responds within
    /// the configured connect timeout; never throws on a 4xx body, only on
    /// transport-level failures.
    async fn health_check(
        &self,
    ) -> Result<HealthState, CaddyError>;
}

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

### 3. `core::secrets::SecretsVault`

Home crate: `core::secrets` in `crates/core/src/secrets/mod.rs`. Default implementation: `crates/adapters/src/secrets_local.rs`. Cross-references: ADR-0014, architecture §6.9.

The encryption boundary. The vault holds the decrypted master key in memory; the master key itself is loaded from the OS keychain (Keychain on macOS, `secret-service` on Linux) at daemon startup. All ciphertext is bound to its associated data so that a row leaked into a different field path or owner cannot decrypt.

```rust
pub struct EncryptContext {
    pub owner_kind: OwnerKind,
    pub owner_id:   String,
    pub field_path: JsonPointer,
    pub key_version: u32,
}

pub trait SecretsVault: Send + Sync + 'static {
    /// Encrypt plaintext under the current master key, binding the ciphertext
    /// to the supplied associated data.
    fn encrypt(
        &self,
        plaintext: &[u8],
        context: &EncryptContext,
    ) -> Result<Ciphertext, CryptoError>;

    /// Decrypt ciphertext under the master key whose version matches the
    /// embedded `key_version`. Refuses to decrypt if the supplied context
    /// differs from the one used at encryption.
    fn decrypt(
        &self,
        ciphertext: &Ciphertext,
        context: &EncryptContext,
    ) -> Result<Vec<u8>, CryptoError>;

    /// Rotate the master key. Re-encrypts every stored ciphertext under the
    /// new key version. Emits one `secrets.master-key-rotated` audit row.
    fn rotate_master_key(
        &self,
    ) -> Result<MasterKeyRotation, CryptoError>;

    /// Replace any plaintext secrets in `value` with redaction tokens. Used
    /// by the audit log writer before serialising diff payloads.
    fn redact(
        &self,
        value: &serde_json::Value,
        schema: &SchemaRegistry,
    ) -> RedactedValue;
}

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("master key version {version} not present")]
    KeyMissing { version: u32 },
    #[error("decryption failed: {detail}")]
    Decryption { detail: String },
    #[error("os keychain unavailable: {detail}")]
    KeyringUnavailable { detail: String },
    #[error("argon2 derivation failed: {detail}")]
    Argon2Failure { detail: String },
}
```

### 4. `core::policy::PresetRegistry`

Home crate: `core::policy` in `crates/core/src/policy/registry.rs`. Default implementation: `crates/adapters/src/policy_registry_static.rs`. Cross-reference: ADR-0016, Phase 18, architecture §6.

The policy preset boundary. Presets are versioned, immutable, and identified by `(preset_id, preset_version)`. The registry rejects attachment of a preset version that is not present in its catalogue with `PresetError::Unknown`.

```rust
pub trait PresetRegistry: Send + Sync + 'static {
    /// Resolve a preset version. Returns `Unknown` if the registry no longer
    /// carries the requested version.
    fn get_preset(
        &self,
        id: &PresetId,
        version: u32,
    ) -> Result<PresetDefinition, PresetError>;

    /// List all known preset versions, sorted by id then version ascending.
    fn list_presets(&self) -> Vec<PresetSummary>;

    /// Attach a preset to a route. Returns the resulting attachment record
    /// to be embedded in the next desired-state snapshot.
    fn attach_to_route(
        &self,
        route: &Route,
        preset: &PresetDefinition,
    ) -> Result<RoutePolicyAttachment, PresetError>;

    /// Remove an attachment from a route.
    fn detach_from_route(
        &self,
        route: &Route,
    ) -> Result<RoutePolicyAttachment, PresetError>;

    /// Migrate an attached preset version forward.
    /// Acceptance: returns `VersionMismatch` if `to_version` is older than
    /// the route's current attachment.
    fn migrate_route_attachment(
        &self,
        route: &Route,
        to_version: u32,
    ) -> Result<RoutePolicyAttachment, PresetError>;
}

#[derive(Debug, thiserror::Error)]
pub enum PresetError {
    #[error("unknown preset {id}@{version}")]
    Unknown { id: PresetId, version: u32 },
    #[error("preset version mismatch: route is at {observed}, requested {requested}")]
    VersionMismatch { observed: u32, requested: u32 },
    #[error("route has incompatible field {field} for preset {preset}")]
    RouteHasIncompatibleField { field: JsonPointer, preset: PresetId },
}
```

### 5. `core::diff::DiffEngine`

Home crate: `core::diff` in `crates/core/src/diff.rs`. No async; pure logic only. Cross-reference: Phase 8, architecture §7.2.

The structural diff boundary. Used by the audit log writer, the drift detector, and the rollback preflight engine. The `apply_diff` operation is intentionally lossy: it returns the smallest mutation that, when applied to `before`, produces a state equal to `after`.

```rust
pub trait DiffEngine: Send + Sync + 'static {
    /// Compute the structural diff between two desired-state values.
    /// Acceptance: the result is deterministic given identical inputs;
    /// keys are sorted lexicographically before comparison.
    fn structural_diff(
        &self,
        before: &DesiredState,
        after:  &DesiredState,
    ) -> Result<Diff, DiffError>;

    /// Apply a previously computed diff to a state, returning the result.
    fn apply_diff(
        &self,
        state: &DesiredState,
        diff:  &Diff,
    ) -> Result<DesiredState, DiffError>;

    /// Replace any plaintext secret values inside the diff with redaction
    /// tokens. The result is what the audit log persists.
    fn redact_diff(
        &self,
        diff:   &Diff,
        schema: &SchemaRegistry,
    ) -> Result<RedactedDiff, DiffError>;
}

#[derive(Debug, thiserror::Error)]
pub enum DiffError {
    #[error("incompatible shape at {path}: cannot diff {before_kind} against {after_kind}")]
    IncompatibleShape { path: JsonPointer, before_kind: String, after_kind: String },
    #[error("redaction violated: plaintext secret remains at {path}")]
    RedactionViolated { path: JsonPointer },
}
```

### 6. `core::reconciler::Applier`

Home crate: `core::reconciler` in `crates/core/src/reconciler/applier.rs`. Default implementation: `crates/adapters/src/applier_caddy.rs`. Cross-reference: Phase 7, ADR-0009.

The apply path boundary. The applier is the sole writer of the Caddy admin endpoint outside drift detection's read-only fetches. It performs the eleven-step apply procedure documented in architecture §7.

```rust
#[async_trait]
pub trait Applier: Send + Sync + 'static {
    /// Apply a snapshot to Caddy. Returns the applied outcome on success;
    /// rolls back to the parent snapshot on any failure.
    /// Acceptance: returns `OptimisticConflict` if the desired-state pointer
    /// moved between snapshot validation and the load call.
    async fn apply(
        &self,
        snapshot: &Snapshot,
        expected_version: i64,
    ) -> Result<ApplyOutcome, ApplyError>;

    /// Validate a snapshot without applying it. Used by Phase 12 preflight.
    async fn validate(
        &self,
        snapshot: &Snapshot,
    ) -> Result<ValidationReport, ApplyError>;

    /// Force a rollback to a previous snapshot. Caller MUST have already
    /// run preflight; this method does not re-check.
    async fn rollback(
        &self,
        target: &SnapshotId,
    ) -> Result<ApplyOutcome, ApplyError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("caddy rejected the load: {detail}")]
    CaddyRejected { detail: String },
    #[error("optimistic conflict: observed {observed_version}, expected {expected_version}")]
    OptimisticConflict { observed_version: i64, expected_version: i64 },
    #[error("preflight failed: {failures:?}")]
    PreflightFailed { failures: Vec<PreflightFailure> },
    #[error("caddy unreachable: {detail}")]
    Unreachable { detail: String },
}
```

### 7. `core::tool_gateway::ToolGateway`

Home crate: `core::tool_gateway` in `crates/core/src/tool_gateway/mod.rs`. Default implementation: `crates/adapters/src/tool_gateway.rs`. Cross-reference: ADR-0008, Phase 19, Phase 20.

The bounded language-model boundary. Every method is gated by per-token scope and per-token rate limit. Read-mode and propose-mode are split deliberately; a token granted only the read scope MUST receive `Unauthorized` from `invoke_propose`.

```rust
#[async_trait]
pub trait ToolGateway: Send + Sync + 'static {
    /// Invoke a read-only function. Always observable; never mutates.
    /// Acceptance: returns `Unauthorized` if the token lacks the function's
    /// scope; returns `OutOfScope` if the function name is not in the
    /// closed read-mode catalogue.
    async fn invoke_read(
        &self,
        token:    &SessionToken,
        function: ReadFunction,
        args:     serde_json::Value,
    ) -> Result<serde_json::Value, ToolGatewayError>;

    /// Invoke a propose-mode function. Always produces a queued proposal,
    /// never an apply.
    async fn invoke_propose(
        &self,
        token:    &SessionToken,
        function: ProposeFunction,
        args:     serde_json::Value,
    ) -> Result<ProposalId, ToolGatewayError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ToolGatewayError {
    #[error("unauthorized: token lacks scope {scope}")]
    Unauthorized { scope: String },
    #[error("function {function} is not in the {mode} catalogue")]
    OutOfScope { function: String, mode: String },
    #[error("rate limited; retry after {retry_after_seconds}s")]
    RateLimited { retry_after_seconds: u32 },
    #[error("prompt-injection refused: {detail}")]
    PromptInjectionRefused { detail: String },
    #[error("validation failed: {} error(s)", errors.len())]
    ValidationFailed { errors: ValidationErrorSet },
}

/// A non-empty list of typed validation errors produced when a propose-mode
/// tool invocation is rejected at validation. Each entry carries a JSON
/// pointer to the offending field, the validation rule that fired, and a
/// human-readable hint. The set is constructed by `core::mutation::validate`
/// and propagated through the gateway's response envelope.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidationErrorSet {
    pub errors: Vec<ValidationError>,   // non-empty by construction
}
```

### 8. `core::probe::ProbeAdapter`

Home crate: `core::probe` in `crates/core/src/probe.rs`. Default implementation: `crates/adapters/src/probe_tokio.rs`. Cross-reference: ADR-0013, Phase 12, Phase 14.

The reachability probe boundary. Used by rollback preflight (Phase 12), upstream health (Phase 14), and capability gating (Phase 4). Probe results are boolean for the simple checks; classified failures are returned through `ProbeFailure`.

```rust
#[async_trait]
pub trait ProbeAdapter: Send + Sync + 'static {
    /// TCP-level reachability of an upstream destination.
    async fn tcp_reachable(
        &self,
        target: &UpstreamDestination,
    ) -> bool;

    /// TLS handshake completion against a hostname (for ACME preflight).
    /// Returns Ok(()) on a successful handshake; classified failures live
    /// in `ProbeFailure`.
    async fn tls_handshake(
        &self,
        host: &Hostname,
    ) -> Result<(), ProbeFailure>;

    /// Capability check: whether the named Caddy module is loaded.
    async fn caddy_module_loaded(
        &self,
        module: &CaddyModule,
    ) -> bool;
}

#[derive(Debug, thiserror::Error)]
pub enum ProbeFailure {
    #[error("tcp unreachable: {detail}")]
    TcpUnreachable { detail: String },
    #[error("tls handshake failed: {detail}")]
    TlsHandshake { detail: String },
    #[error("dns resolution failed for {host}: {detail}")]
    DnsResolution { host: String, detail: String },
    #[error("probe timed out after {seconds}s")]
    Timeout { seconds: u32 },
}
```

### 9. `core::config::EnvProvider`

Home crate: `core::config` in `crates/core/src/config/env.rs`. Default implementation: `crates/adapters/src/env_provider.rs`. Cross-reference: Phase 1.

The environment-variable boundary. Pulled into a trait so the configuration loader is testable without `std::env`.

```rust
pub trait EnvProvider: Send + Sync + 'static {
    /// Fetch a single variable.
    fn var(&self, key: &str) -> Result<String, EnvError>;

    /// Fetch every variable whose name starts with the supplied prefix.
    /// The prefix is stripped from the returned keys.
    fn vars_with_prefix(&self, prefix: &str) -> Vec<(String, String)>;
}

#[derive(Debug, thiserror::Error)]
pub enum EnvError {
    #[error("environment variable {key} is not present")]
    NotPresent { key: String },
    #[error("environment variable {key} is not valid Unicode")]
    NotUnicode { key: String },
}
```

### 10. `core::http::HttpServer`

Home crate: `core::http` in `crates/core/src/http/mod.rs`. Default implementation: `crates/adapters/src/http_axum.rs`. Cross-reference: Phase 9, ADR-0011.

The daemon's inbound HTTP face. Bound to loopback by default; remote binding requires the `[server] allow_remote = true` flag (architecture §8.1).

```rust
#[async_trait]
pub trait HttpServer: Send + 'static {
    /// Bind the configured listener. Returns the bound socket address so the
    /// caller can log it once and inject it into the audit log.
    async fn bind(
        &mut self,
        config: &ServerConfig,
    ) -> Result<SocketAddr, HttpServerError>;

    /// Run the server until graceful shutdown is requested.
    async fn run(
        self,
        shutdown: ShutdownSignal,
    ) -> Result<(), HttpServerError>;

    /// Trigger graceful shutdown from another task.
    async fn shutdown(
        &self,
    ) -> Result<(), HttpServerError>;
}

#[derive(Debug, thiserror::Error)]
pub enum HttpServerError {
    #[error("bind failed for {addr}: {detail}")]
    BindFailed { addr: SocketAddr, detail: String },
    #[error("server crashed: {detail}")]
    Crashed { detail: String },
}
```

### 11. `core::docker::DockerWatcher`

Home crate: `core::docker` in `crates/core/src/docker/mod.rs`. Default implementation: `crates/adapters/src/docker_bollard.rs`. Cross-reference: ADR-0007, Phase 21.

The Docker engine boundary. Only present when the daemon is configured with Docker socket access. The watcher reconnects with exponential backoff on socket failures.

```rust
#[async_trait]
pub trait DockerWatcher: Send + Sync + 'static {
    /// Stream Docker events. The stream terminates on socket loss; the
    /// caller is responsible for reconnect backoff.
    async fn events(
        &self,
    ) -> Result<DockerEventStream, DockerError>;

    /// Inspect a container by id. Used to resolve label sets after an event.
    async fn inspect_container(
        &self,
        id: &ContainerId,
    ) -> Result<ContainerInspect, DockerError>;

    /// Probe whether a container is reachable from the Caddy network.
    /// Used to surface "Docker container claims hostname X but is not on
    /// the trilithon network" warnings (Phase 21).
    async fn reachability_check(
        &self,
        id: &ContainerId,
    ) -> Result<ContainerReachability, DockerError>;
}

#[derive(Debug, thiserror::Error)]
pub enum DockerError {
    #[error("docker socket unavailable: {detail}")]
    SocketUnavailable { detail: String },
    #[error("permission denied accessing docker socket")]
    Permission,
    #[error("docker engine error: {detail}")]
    EngineError { detail: String },
}
```

### 12. `core::shutdown::ShutdownObserver`

Home crate: `core::shutdown` in `crates/core/src/shutdown.rs`. Default implementation: `crates/adapters/src/shutdown_tokio.rs` (built on `tokio::sync::watch`). Cross-reference: Phase 1, Phase 2, architecture §5 (layer rules).

The graceful-shutdown signal boundary. The `cli` binary owns the OS-signal handler (SIGTERM, SIGINT). When a signal arrives, the binary publishes "shutdown requested" through the observer; long-running tasks (HTTP server, drift loop, Docker watcher) consume the observer and unwind. The trait pattern preserves the §5 layer rule: `core` does not depend on `tokio`, but every layer can depend on the abstract `ShutdownObserver`.

```rust
pub trait ShutdownObserver: Send + Sync + 'static {
    /// Returns `true` once a shutdown has been requested. Cheap, non-blocking.
    /// Implementations MUST use atomic/fenced read so the value is visible
    /// across threads.
    fn is_requested(&self) -> bool;

    /// Awaits shutdown notification. Resolves once the producer has signalled
    /// shutdown. After resolution, all subsequent `await` calls resolve
    /// immediately.
    /// Acceptance: this method is the only place a `core` module observes
    /// the runtime's idleness; no `tokio` types appear in the trait surface.
    fn wait(&self) -> ShutdownFuture<'_>;
}

/// An opaque future wrapper. Adapter implementations may back it with
/// `tokio::sync::watch::Receiver::changed`; alternative runtimes may use
/// equivalent primitives. The wrapper exists to keep the trait
/// runtime-agnostic without requiring `core` to depend on a specific
/// async-runtime crate.
pub struct ShutdownFuture<'a> {
    inner: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>>,
}
```

There is no `ShutdownObserverError` variant; the abstraction is infallible from the consumer's perspective. The producer side (the `cli` signal handler) lives in `crates/cli/src/shutdown.rs` and is not part of this trait surface.

## Cross-trait invariants

The traits above are designed to compose cleanly. Several invariants hold across the surface and MUST be preserved by any implementor.

**Correlation propagation.** Every public method on every async trait above is invoked from a tracing span carrying `correlation_id`. Implementors MUST NOT generate a fresh correlation identifier on entry; they MUST inherit the caller's span. Background loops (drift detector, capability probe scheduler, upstream-health refresher) MAY generate a new identifier per iteration but MUST log the iteration's identifier on the first event of each iteration. The architecture §12.1 span field key is `correlation_id`; this is non-negotiable.

**Audit row provenance.** Any trait method that, directly or indirectly, causes an audit row to be written MUST do so through `Storage::record_audit_event`. No other path writes the audit log. The `kind` string passed in the `AuditEventRow` MUST be one of the strings in architecture §6.6, returned by the `Display` impl on `core::audit::AuditEvent`.

**Error-to-tracing mapping.** Every trait error variant maps to a `tracing::Span` field via the `error.kind` and `error.detail` keys (architecture §12.1). Implementors emit `error.kind` as the `Debug` form of the variant constructor (`"OptimisticConflict"`, `"CaddyRejected"`, and so on) and `error.detail` as the `Display` form of the full error.

**Object-safety preservation.** Every trait above is currently object-safe. Adding a generic method, a method consuming `self`, or an associated type without `Self: Sized` constraints would break object-safety and is forbidden. New methods MUST follow the conventions section above.

**Lifetime conventions.** Stored handles (`Arc<dyn Storage>`, `Arc<dyn CaddyClient>`, and other trait objects held for the daemon's lifetime) are `'static`. Method-local borrows MAY use elided lifetimes; if an explicit lifetime is required for a borrowed argument or return, use a single named lifetime (`<'a>`) and document it in the doc comment.

**Test doubles.** Every trait above has a default test double in `crates/adapters/tests/doubles/` named `<Trait>Double`. The double records every method call into a shared `Vec<DoubleCall>` and returns a programmable `Result`. This convention is reused across phases; new traits introduced in later phases MUST ship with a matching double in the same commit.

## Stability and authority

The signatures above are STABLE for V1.0. Adding a new trait or a new method to an existing trait requires:

1. Adding the signature to this document in the same commit that introduces it.
2. Updating any phase TODO that depends on the new method to cross-reference this section.
3. Updating the architecture §6 data model if the trait introduces a new persistent row shape.

If a phase TODO contradicts a signature here, this document wins; open a fix-up commit against the TODO.
