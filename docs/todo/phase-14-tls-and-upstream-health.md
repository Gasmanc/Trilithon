# Phase 14 — TLS visibility and upstream health — Implementation Slices

> Phase reference: [../phases/phase-14-tls-and-upstream-health.md](../phases/phase-14-tls-and-upstream-health.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference [phase-14-tls-and-upstream-health.md](../phases/phase-14-tls-and-upstream-health.md).
- Architecture sections: §4.2 (`adapters` boundary, no I/O outside this layer), §6.6 (`audit_log`), §6.13 (`capability_probe_results.tls_inventory` JSON column reuse), §10 (failure model rows for ACME failure and Caddy unreachable), §12.1 (tracing vocabulary, especially `upstream.probe.completed` and `caddy.capability-probe.completed`), §13 (performance budget).
- Trait signatures: §2 `core::caddy::CaddyClient` (`get_certificates`, `get_upstream_health`), §1 `core::storage::Storage`, §8 `core::probe::ProbeAdapter`.
- ADRs: ADR-0002 (Caddy JSON Admin API as source of truth), ADR-0013 (capability probe gates optional features).
- PRD T1.9 (TLS certificate visibility), T1.10 (basic upstream health visibility).
- Hazards: H17 (apply-time TLS provisioning latency).

## Glossary specific to this phase

| Term | Definition |
|------|------------|
| TLS inventory | The complete set of certificates Trilithon currently knows about for its managed hosts. Held in `tls_certificates` and refreshed every 5 minutes. |
| Renewal state | One of `healthy`, `pending`, `failed`, `unknown`. Computed from `not_after - now` and the underlying ACME status reported by Caddy. |
| Upstream-health state | One of `reachable`, `unreachable`, `probe-disabled`, `unknown`. The first three are observable; `unknown` is the initial state before any probe completes. |
| Long-poll subscription | A single long-lived HTTP request to Caddy's `/reverse_proxy/upstreams` that returns whenever an upstream's status flips. Trilithon reconnects on socket loss with exponential back-off. |
| Probe debounce | A 1-second timer started on a long-poll flip. A second flip during the window resets the timer. The refresh fires once per debounce window. |
| Issuing state | A user-visible state distinct from "applied". When Caddy is in the middle of ACME issuance for a freshly added managed host, the route surfaces "Issuing" until issuance succeeds or fails. |

## Trait surfaces consumed by this phase

This phase touches three traits from `trait-signatures.md`. The implementer MUST consult that document before writing call sites.

- §1 `core::storage::Storage` — extended with `upsert_tls_certificates`, `list_tls_certificates`, `upsert_upstream_health`, `list_upstream_health` (slice 14.1). The trait extension MUST land in `trait-signatures.md` in the same commit per its stability rule.
- §2 `core::caddy::CaddyClient` — calls `get_certificates` and `get_upstream_health`. No new methods.
- §8 `core::probe::ProbeAdapter` — calls `tcp_reachable`. No new methods.

## Slice plan summary

| # | Slice title | Primary files | Effort (h) | Depends on |
|---|---|---|---|---|
| 14.1 | Migration `0005_tls_and_health.sql` and storage extension | `core/crates/adapters/migrations/0005_tls_and_health.sql`, `core/crates/adapters/src/storage_sqlite.rs` | 4 | Phase 12 |
| 14.2 | `TlsInventory` adapter with 5-minute Tokio interval | `core/crates/adapters/src/tls_inventory.rs` | 6 | 14.1 |
| 14.3 | `UpstreamHealth` adapter with 30-second Tokio interval and Caddy long-poll subscription | `core/crates/adapters/src/upstream_health.rs` | 8 | 14.1 |
| 14.4 | Route-level probe opt-out (`disable_trilithon_probes`) wiring | `core/crates/core/src/route.rs`, `core/crates/adapters/src/upstream_health.rs` | 3 | 14.3 |
| 14.5 | HTTP endpoints `GET /api/v1/tls/certificates` and `/upstreams/health` | `core/crates/cli/src/http/tls.rs`, `core/crates/cli/src/http/upstreams.rs` | 4 | 14.2, 14.3 |
| 14.6 | Web UI per-route TLS and upstream health badges | `web/src/features/routes/RouteCard.tsx`, `web/src/features/routes/RouteCard.test.tsx` | 6 | 14.5 |
| 14.7 | Dashboard "TLS expiring soon" widget | `web/src/features/dashboard/TlsExpiringWidget.tsx` | 4 | 14.5 |
| 14.8 | "Issuing" vs "applied" state with ACME error surfacing (H17) | `core/crates/adapters/src/tls_inventory.rs`, `core/crates/cli/src/http/tls.rs`, `web/src/features/routes/RouteCard.tsx` | 6 | 14.5, 14.6 |

After every slice: `cargo build --workspace` succeeds; `pnpm typecheck` succeeds where the slice touches the web; the slice's named tests pass.

---

## Slice 14.1 — Migration `0005_tls_and_health.sql` and storage extension

### Goal

Add the persistent storage for the TLS inventory and the upstream-health state. Two new tables: `tls_certificates` and `upstream_health`. Every time column is UTC Unix seconds (architecture H6).

### Entry conditions

- Phase 12 complete.
- The Phase 2 migration runner picks up new files in `core/crates/adapters/migrations/`.

### Files to create or modify

- `core/crates/adapters/migrations/0005_tls_and_health.sql` — DDL.
- `core/crates/adapters/src/storage_sqlite.rs` — extend with `upsert_tls_certificates`, `list_tls_certificates`, `upsert_upstream_health`, `list_upstream_health`.
- `core/crates/core/src/storage.rs` — extend the `Storage` trait if the methods are not already on the trait surface (trait-signatures.md §1 lists the canonical surface; if extending, update that file in the same commit).

### Signatures and shapes

```sql
-- core/crates/adapters/migrations/0005_tls_and_health.sql
CREATE TABLE tls_certificates (
    host                TEXT NOT NULL PRIMARY KEY,
    issuer              TEXT NOT NULL,
    not_before          INTEGER NOT NULL,
    not_after           INTEGER NOT NULL,
    renewal_state       TEXT NOT NULL CHECK (renewal_state IN ('healthy', 'pending', 'failed', 'unknown')),
    source              TEXT NOT NULL CHECK (source IN ('acme', 'internal', 'imported')),
    fetched_at          INTEGER NOT NULL,
    last_error          TEXT
);
CREATE INDEX tls_certificates_not_after ON tls_certificates(not_after);

CREATE TABLE upstream_health (
    route_id            TEXT NOT NULL,
    upstream            TEXT NOT NULL,
    state               TEXT NOT NULL CHECK (state IN ('reachable', 'unreachable', 'probe-disabled', 'unknown')),
    source              TEXT NOT NULL CHECK (source IN ('caddy', 'trilithon-tcp', 'merged')),
    last_transition_at  INTEGER NOT NULL,
    last_checked_at     INTEGER NOT NULL,
    detail              TEXT,
    PRIMARY KEY (route_id, upstream)
);
CREATE INDEX upstream_health_route_id ON upstream_health(route_id);
```

```rust
// trait-signatures.md §1 extensions
#[async_trait]
pub trait Storage: Send + Sync + 'static {
    // ... existing methods ...

    async fn upsert_tls_certificates(
        &self,
        certs: &[TlsCertificateRow],
        fetched_at: UnixSeconds,
    ) -> Result<u32, StorageError>;

    async fn list_tls_certificates(
        &self,
    ) -> Result<Vec<TlsCertificateRow>, StorageError>;

    async fn upsert_upstream_health(
        &self,
        rows: &[UpstreamHealthRow],
    ) -> Result<u32, StorageError>;

    async fn list_upstream_health(
        &self,
        route_id: Option<&RouteId>,
    ) -> Result<Vec<UpstreamHealthRow>, StorageError>;
}

pub struct TlsCertificateRow {
    pub host: String,
    pub issuer: String,
    pub not_before: i64,
    pub not_after: i64,
    pub renewal_state: RenewalState,
    pub source: CertificateSource,
    pub fetched_at: i64,
    pub last_error: Option<String>,
}

pub struct UpstreamHealthRow {
    pub route_id: RouteId,
    pub upstream: String,
    pub state: UpstreamState,
    pub source: HealthSource,
    pub last_transition_at: i64,
    pub last_checked_at: i64,
    pub detail: Option<String>,
}
```

### Algorithm

`upsert_tls_certificates` is a transactional `INSERT ... ON CONFLICT(host) DO UPDATE`. `upsert_upstream_health` is a transactional `INSERT ... ON CONFLICT(route_id, upstream) DO UPDATE` that updates `last_transition_at` only when `state` changes.

Numbered procedure for `upsert_upstream_health`:

1. `BEGIN IMMEDIATE` (the SQLite busy-timeout discipline from Phase 2 applies).
2. For each row in the input vector:
   1. `SELECT state, last_transition_at FROM upstream_health WHERE route_id = ? AND upstream = ?`.
   2. If no prior row: `INSERT` with `last_transition_at = now`.
   3. If `prior.state == row.state`: `UPDATE` setting `last_checked_at = now` only.
   4. If `prior.state != row.state`: `UPDATE` setting both `state = row.state, last_transition_at = now, last_checked_at = now, source = row.source, detail = row.detail`.
3. `COMMIT`.

This procedure is exercised by the slice's named tests; it preserves the `last_transition_at` semantics that the dashboard relies on for the "flipped to red 12 seconds ago" surface.

### Tests

Integration tests at `core/crates/adapters/tests/storage_tls_health.rs`:

- `migration_creates_tls_certificates_and_upstream_health_tables`.
- `upsert_tls_certificate_inserts_then_updates_on_conflict`.
- `upsert_upstream_health_advances_last_transition_only_on_state_flip`.
- `list_upstream_health_filters_by_route_id`.

### Acceptance command

`cargo test -p trilithon-adapters --test storage_tls_health`

### Exit conditions

- All four tests pass.
- The migration is forward-only and recorded in `schema_migrations` per architecture §14.

### Audit kinds emitted

None.

### Tracing events emitted

`storage.migrations.applied` (existing; emitted by the Phase 2 migration runner when migration 0005 is applied).

### Cross-references

- Architecture §6 (data model conventions), §14 (upgrade and migration).
- PRD T1.9, T1.10.

---

## Slice 14.2 — `TlsInventory` adapter with 5-minute Tokio interval

### Goal

Implement `refresh_tls_inventory` and the periodic refresh task. The adapter calls `GET /pki/ca/local/certificates` AND `GET /config/apps/tls/certificates` against Caddy, merges by host, computes `renewal_state` from `(not_after - now)`, and persists via `Storage`.

### Entry conditions

- Slice 14.1 complete.
- `CaddyClient::get_certificates` is implemented per trait-signatures.md §2.

### Files to create or modify

- `core/crates/adapters/src/tls_inventory.rs`.
- `core/crates/cli/src/services.rs` — register the periodic task.

### Signatures and shapes

```rust
// core/crates/adapters/src/tls_inventory.rs
use std::sync::Arc;
use std::time::Duration;
use crate::Storage;
use trilithon_core::caddy::client::CaddyClient;

pub async fn refresh_tls_inventory(
    client: &dyn CaddyClient,
    store: &dyn Storage,
) -> Result<TlsInventoryReport, TlsInventoryError>;

#[derive(Debug, Clone, serde::Serialize)]
pub struct TlsInventoryReport {
    pub certificates: Vec<TlsCertificate>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TlsCertificate {
    pub host: String,
    pub issuer: String,
    pub not_before: i64,
    pub not_after: i64,
    pub renewal_state: RenewalState,
    pub source: CertificateSource,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenewalState { Healthy, Pending, Failed, Unknown }

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CertificateSource { Acme, Internal, Imported }

#[derive(Debug, thiserror::Error)]
pub enum TlsInventoryError {
    #[error("caddy client error: {source}")]
    Caddy { #[source] source: trilithon_core::caddy::client::CaddyError },
    #[error("storage error: {source}")]
    Storage { #[source] source: trilithon_adapters::StorageError },
}

pub async fn run_inventory_loop(
    client: Arc<dyn CaddyClient>,
    store: Arc<dyn Storage>,
    period: Duration,        // default Duration::from_secs(300)
    shutdown: tokio::sync::watch::Receiver<bool>,
);
```

### Algorithm

```
1. Tokio interval with period = 5 minutes.
2. On tick:
   a. Fetch GET /pki/ca/local/certificates AND /config/apps/tls/certificates via CaddyClient::get_certificates (which already merges these endpoints per trait-signatures.md §2).
   b. Merge by host. Compute renewal_state:
      - now - not_after >= 0           => Failed
      - not_after - now <= 3 days      => Pending (renewal imminent)
      - not_after - now <= 14 days     => Pending
      - otherwise                       => Healthy
   c. Persist via Storage::upsert_tls_certificates with fetched_at = now.
   d. Emit tracing event `caddy.capability-probe.completed` with span field `caddy.module = "tls"`.
3. On shutdown signal, exit cleanly.
```

The `caddy.capability-probe.completed` event is reused by design (architecture §12.1); its span field `caddy.module = "tls"` distinguishes the TLS-inventory tick from the general capability probe.

### Tests

Integration tests at `core/crates/adapters/tests/tls_inventory.rs`:

- `refresh_persists_one_row_per_host`.
- `refresh_marks_certificates_within_3_days_as_pending`.
- `refresh_marks_expired_as_failed`.
- `inventory_loop_ticks_every_300_seconds_within_tolerance` — use `tokio::time::pause` and `advance` to verify cadence without wall-clock waiting.

### Acceptance command

`cargo test -p trilithon-adapters --test tls_inventory`

### Exit conditions

- All four tests pass.
- The periodic task respects the `shutdown` watch channel.

### Audit kinds emitted

None directly. The capability probe writes `caddy.capability-probe-completed` per Phase 3; this loop reuses the same audit kind on its first tick of each daemon lifetime to record TLS visibility availability.

### Tracing events emitted

- `caddy.capability-probe.completed` (architecture §12.1) with `caddy.module = "tls"`.

### Cross-references

- PRD T1.9.
- ADR-0013.
- Trait signatures §2.

---

## Slice 14.3 — `UpstreamHealth` adapter with 30-second interval and Caddy long-poll

### Goal

Implement the upstream-health refresher. Two data sources are merged: Caddy's `/reverse_proxy/upstreams` long-poll endpoint and a Trilithon-side TCP probe (skipped when `disable_trilithon_probes` is set, see slice 14.4). The refresher runs on a 30-second interval and additionally reacts to long-poll-driven state flips with a 1-second debounce.

### Entry conditions

- Slice 14.1 complete.
- `CaddyClient::get_upstream_health` is implemented.
- `ProbeAdapter::tcp_reachable` is implemented.

### Files to create or modify

- `core/crates/adapters/src/upstream_health.rs`.
- `core/crates/cli/src/services.rs` — register the task and the long-poll subscriber.

### Signatures and shapes

```rust
// core/crates/adapters/src/upstream_health.rs
use std::sync::Arc;
use std::time::Duration;
use trilithon_core::caddy::client::CaddyClient;
use trilithon_core::probe::ProbeAdapter;
use trilithon_core::route::{Route, Upstream};

pub async fn probe_upstream(
    probe: &dyn ProbeAdapter,
    upstream: &Upstream,
) -> UpstreamProbeResult;

#[derive(Debug, Clone)]
pub struct UpstreamProbeResult {
    pub state: UpstreamState,
    pub source: HealthSource,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpstreamState { Reachable, Unreachable, ProbeDisabled, Unknown }

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HealthSource { Caddy, TrilithonTcp, Merged }

pub async fn run_health_loop(
    caddy: Arc<dyn CaddyClient>,
    probe: Arc<dyn ProbeAdapter>,
    store: Arc<dyn Storage>,
    period: Duration,           // default Duration::from_secs(30)
    debounce: Duration,         // default Duration::from_secs(1)
    shutdown: tokio::sync::watch::Receiver<bool>,
);
```

### Algorithm

```
1. Tokio interval with period = 30 seconds.
2. ALSO subscribe to the Caddy `/reverse_proxy/upstreams` long-poll endpoint.
   On any flip event, schedule a debounced refresh with delay = 1 second; if a
   second flip arrives within the window, reset the timer.
3. Refresh procedure:
   a. Read every route's upstream list from the current desired-state snapshot.
   b. For each (route, upstream):
      i.   Fetch Caddy's reported state via CaddyClient::get_upstream_health.
      ii.  If route.disable_trilithon_probes is true, skip the TCP probe;
           the resulting source is `caddy`.
      iii. Otherwise, await ProbeAdapter::tcp_reachable. The result merges
           with Caddy's state per the merge rule:
             - Both reachable        => Reachable, source = Merged.
             - Trilithon reachable, Caddy unknown => Reachable, source = TrilithonTcp.
             - Either unreachable    => Unreachable, source = Merged.
             - Neither has data      => Unknown.
      iv.  Persist via Storage::upsert_upstream_health.
4. Emit tracing event `upstream.probe.completed` with span fields
   `route.id`, `correlation_id`, and `latency_ms`.
5. On shutdown signal, exit cleanly.
```

The `upstream.probe.completed` event was added to architecture §12.1 in the same commit that introduces this slice (per the §12.1 vocabulary-authority rule).

### Tests

Integration tests at `core/crates/adapters/tests/upstream_health.rs`:

- `probe_upstream_returns_reachable_when_tcp_listener_present`.
- `probe_upstream_returns_unreachable_on_connection_refused`.
- `health_loop_ticks_every_30_seconds_within_tolerance` (using `tokio::time::pause`).
- `long_poll_flip_triggers_debounced_refresh_within_1500_ms`.
- `merge_rule_caddy_unknown_trilithon_reachable_yields_reachable_source_trilithon_tcp`.
- `flip_to_unreachable_propagates_within_30_seconds` — start with a listener, drop it, advance time; assert state flips.

### Acceptance command

`cargo test -p trilithon-adapters --test upstream_health`

### Exit conditions

- All six tests pass.
- The 30-second cadence and 1-second debounce are observable.

### Audit kinds emitted

None.

### Tracing events emitted

- `upstream.probe.completed` (architecture §12.1).

### Cross-references

- PRD T1.10.
- Trait signatures §2, §8.

---

## Slice 14.4 — Route-level probe opt-out

### Goal

Honour the `disable_trilithon_probes` route field. When set, only Caddy-reported reachability surfaces; no Trilithon TCP probe is performed against any upstream of that route.

### Entry conditions

- Slice 14.3 complete.
- The Phase 4 / Phase 11 mutation algebra exposes `Route { ..., disable_trilithon_probes: bool, ... }`.

### Files to create or modify

- `core/crates/core/src/route.rs` — confirm the field exists; add migration if not.
- `core/crates/adapters/src/upstream_health.rs` — branch on the flag.
- `core/crates/cli/src/http/routes.rs` — surface the flag in the route mutation API.

### Signatures and shapes

```rust
// core/crates/core/src/route.rs (relevant subset)
pub struct Route {
    pub id: RouteId,
    pub host: String,
    pub port: u16,
    pub upstreams: Vec<Upstream>,
    pub disable_trilithon_probes: bool,
    // ... other fields ...
}
```

### Algorithm

In `run_health_loop` step 3.b.ii, if `route.disable_trilithon_probes`, set:

- `state = Caddy::reachable_from_caddy_or_unknown`.
- `source = HealthSource::Caddy`.
- `detail = Some("trilithon probe disabled by route configuration")`.

When the route emits no Caddy-derived reachability either (Caddy returned `unknown`), persist `state = ProbeDisabled` to make the surface state distinguishable from `Unreachable`.

### Tests

Integration tests at `core/crates/adapters/tests/upstream_health.rs` (extend the slice 14.3 file):

- `route_with_disable_trilithon_probes_skips_tcp_probe` — assert the probe adapter records zero `tcp_reachable` calls for that route's upstreams.
- `route_with_disable_trilithon_probes_persists_probe_disabled_when_caddy_unknown`.

### Acceptance command

`cargo test -p trilithon-adapters --test upstream_health route_with_disable_trilithon_probes`

### Exit conditions

- Both tests pass.
- The flag is settable via the route mutation API.

### Audit kinds emitted

None.

### Tracing events emitted

`upstream.probe.completed` (with `latency_ms = 0` when the probe is skipped).

### Cross-references

- PRD T1.10.

---

## Slice 14.5 — HTTP endpoints `GET /api/v1/tls/certificates` and `/upstreams/health`

### Goal

Expose the persisted TLS inventory and the upstream-health state through the authenticated HTTP API. Both endpoints are reads with simple JSON responses.

### Entry conditions

- Slices 14.2 and 14.3 complete.

### Files to create or modify

- `core/crates/cli/src/http/tls.rs`.
- `core/crates/cli/src/http/upstreams.rs`.
- `core/crates/cli/src/http/mod.rs` — register the routes.

### Signatures and shapes

```rust
// core/crates/cli/src/http/tls.rs
#[derive(serde::Serialize)]
pub struct TlsCertificatesResponse {
    pub certificates: Vec<TlsCertificate>,
    pub fetched_at: i64,
}

pub async fn get_certificates(
    State(ctx): State<HttpContext>,
    auth: AuthenticatedActor,
) -> Result<Json<TlsCertificatesResponse>, ApiError>;
```

```rust
// core/crates/cli/src/http/upstreams.rs
#[derive(serde::Serialize)]
pub struct UpstreamHealthResponse {
    pub items: Vec<UpstreamHealthRow>,
}

pub async fn get_upstream_health(
    State(ctx): State<HttpContext>,
    auth: AuthenticatedActor,
    Query(query): Query<UpstreamHealthQuery>,
) -> Result<Json<UpstreamHealthResponse>, ApiError>;

#[derive(serde::Deserialize)]
pub struct UpstreamHealthQuery {
    pub route_id: Option<RouteId>,
}
```

### Algorithm

Each handler:

1. Authenticate session.
2. Open span `http.request.received`.
3. Call `Storage::list_tls_certificates` or `Storage::list_upstream_health`.
4. Return `200` with the response.

### Tests

Integration tests at `core/crates/cli/tests/tls_health_http.rs`:

- `get_certificates_returns_persisted_inventory`.
- `get_upstream_health_filters_by_route_id`.
- `get_certificates_requires_authentication`.
- `get_upstream_health_requires_authentication`.

### Acceptance command

`cargo test -p trilithon-cli --test tls_health_http`

### Exit conditions

- All four tests pass.

### Audit kinds emitted

None (read endpoints do not write the audit log).

### Tracing events emitted

`http.request.received`, `http.request.completed`.

### Cross-references

- PRD T1.9, T1.10.
- Architecture §12.1.

---

## Slice 14.6 — Web UI per-route TLS and upstream health badges

### Goal

Render TLS and upstream-health badges on each `RouteCard`. The TLS badge MUST be green if `expiry > 14 days`, amber if `14 days >= expiry > 3 days`, red if `expiry <= 3 days OR renewal_state === 'failed'`. The upstream-health badge MUST show reachable, unreachable, or probe-disabled.

### Entry conditions

- Slice 14.5 complete.

### Files to create or modify

- `web/src/features/routes/RouteCard.tsx` — extend with both badges.
- `web/src/features/routes/RouteCard.test.tsx` — tests with the exact names from the phase reference.
- `web/src/features/routes/api.ts` — extend with `fetchCertificates`, `fetchUpstreamHealth`.

### Signatures and shapes

```tsx
// web/src/features/routes/RouteCard.tsx
export interface RouteCardProps {
  readonly route: Route;
  readonly certificate?: TlsCertificate;
  readonly health?: readonly UpstreamHealthRow[];
}
export function RouteCard(props: RouteCardProps): JSX.Element;
```

```ts
// web/src/features/routes/types.ts (additions)
export type RenewalState = 'healthy' | 'pending' | 'failed' | 'unknown';
export type UpstreamState = 'reachable' | 'unreachable' | 'probe-disabled' | 'unknown';

export interface TlsCertificate {
  readonly host: string;
  readonly issuer: string;
  readonly not_before: number;
  readonly not_after: number;
  readonly renewal_state: RenewalState;
  readonly source: 'acme' | 'internal' | 'imported';
}
```

### Algorithm — TLS badge colour

1. Compute `expiry_seconds = certificate.not_after - now_unix_seconds`.
2. If `certificate.renewal_state === 'failed'`, return `'red'`.
3. If `expiry_seconds <= 3 * 86400`, return `'red'`.
4. If `expiry_seconds <= 14 * 86400`, return `'amber'`.
5. Otherwise return `'green'`.

### Algorithm — upstream health badge

1. If every upstream's `state === 'reachable'`, render green "Reachable".
2. If any upstream's `state === 'unreachable'`, render red with a tooltip listing unreachable upstreams.
3. If every upstream's `state === 'probe-disabled'`, render grey "Probe disabled".
4. Otherwise render grey "Unknown".

### Tests

Vitest tests at `web/src/features/routes/RouteCard.test.tsx` (names match the phase reference verbatim):

- `renders_green_when_expiry_gt_14_days`.
- `renders_amber_when_expiry_within_14_days`.
- `renders_red_when_expiry_within_3_days`.
- `renders_red_when_renewal_failed`.
- `renders_reachable_badge_when_all_upstreams_reachable`.
- `renders_unreachable_badge_when_any_upstream_unreachable`.
- `renders_probe_disabled_badge_when_all_upstreams_probe_disabled`.

### Acceptance command

`pnpm vitest run web/src/features/routes/RouteCard.test.tsx`

### Exit conditions

- All seven tests pass with the exact names listed.
- `pnpm typecheck` passes.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.9, T1.10.

---

## Slice 14.7 — Dashboard "TLS expiring soon" widget

### Goal

A dashboard widget listing every certificate within 14 days of expiry. The widget consumes `GET /api/v1/tls/certificates` and renders host, issuer, days-until-expiry, and a link to the relevant route detail.

### Entry conditions

- Slice 14.5 complete.

### Files to create or modify

- `web/src/features/dashboard/TlsExpiringWidget.tsx`.
- `web/src/features/dashboard/TlsExpiringWidget.test.tsx`.

### Signatures and shapes

```tsx
// web/src/features/dashboard/TlsExpiringWidget.tsx
export interface TlsExpiringWidgetProps {
  readonly certificates: readonly TlsCertificate[];
}
export function TlsExpiringWidget(props: TlsExpiringWidgetProps): JSX.Element;
```

### Algorithm

1. Filter `certificates` to those with `not_after - now <= 14 * 86400` OR `renewal_state === 'failed'`.
2. Sort ascending by `not_after`.
3. Render each row with host, issuer, days remaining (rounded down), and a link to `/routes/<id>` derived from `host`.
4. If the filtered list is empty, render "No certificates expiring soon".

### Tests

Vitest tests at `web/src/features/dashboard/TlsExpiringWidget.test.tsx`:

- `widget_renders_only_certificates_within_14_days`.
- `widget_sorts_ascending_by_not_after`.
- `widget_renders_empty_state_when_no_expiring_certificates`.
- `widget_includes_failed_renewal_regardless_of_expiry`.

### Acceptance command

`pnpm vitest run web/src/features/dashboard/TlsExpiringWidget.test.tsx`

### Exit conditions

- All four tests pass.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- PRD T1.9.

---

## Slice 14.8 — "Issuing" vs "applied" state with ACME error surfacing

### Goal

Mitigate hazard H17. When an apply introduces a new managed host, the route MUST show an "issuing" indicator until Caddy reports the certificate ready. ACME failures MUST surface with actionable messages from Caddy's status endpoint. Backend changes: extend `TlsInventory` to record `pending` / `failed` states with the underlying ACME error detail in `tls_certificates.last_error`. Frontend: a third badge state ("issuing") and an error banner.

### Entry conditions

- Slices 14.5 and 14.6 complete.

### Files to create or modify

- `core/crates/adapters/src/tls_inventory.rs` — extend the merge step to capture ACME failure detail when Caddy's `tls_issuance` reports an error.
- `core/crates/cli/src/http/tls.rs` — surface `last_error` in the JSON response.
- `web/src/features/routes/RouteCard.tsx` — add the "issuing" state and the error banner.
- `web/src/features/routes/RouteCard.test.tsx` — add transition tests.

### Signatures and shapes

```rust
// extension to TlsCertificate
pub struct TlsCertificate {
    // ... existing fields ...
    pub last_error: Option<String>,
}
```

```ts
// web/src/features/routes/types.ts (extension)
export interface TlsCertificate {
  // ... existing fields ...
  readonly last_error: string | null;
}
```

### Algorithm — adapter

1. After merging `/pki/ca/local/certificates` and `/config/apps/tls/certificates`, query Caddy's `tls_issuance` status (exposed under `/config/apps/tls`).
2. For any host with `state === 'pending'` or `state === 'failed'`, capture the underlying ACME error detail and persist it in `tls_certificates.last_error`.
3. The renewal-state mapping extends:
   - Caddy reports `issuing` and no certificate present yet => `Pending`, `last_error = None`.
   - Caddy reports `issuing` after a prior failure => `Pending`, `last_error = Some(<detail>)`.
   - Caddy reports `error` => `Failed`, `last_error = Some(<detail>)`.

### Algorithm — frontend

1. The TLS badge gains a fourth colour: blue "Issuing" when `renewal_state === 'pending' && certificate.not_after === 0` (no certificate yet). The transition from blue to green or red happens when the next inventory tick updates the row.
2. When `last_error` is non-null and `renewal_state === 'failed'`, render an inline banner with the error detail and an "Retry" button that triggers a desired-state re-apply for the affected route.

### Tests

Integration tests at `core/crates/adapters/tests/tls_inventory.rs` (extend slice 14.2):

- `inventory_records_pending_state_for_freshly_added_host`.
- `inventory_records_failed_state_with_acme_error_detail`.
- `inventory_transitions_pending_to_healthy_on_certificate_arrival`.

Vitest tests at `web/src/features/routes/RouteCard.test.tsx` (extend slice 14.6):

- `renders_issuing_badge_for_freshly_added_host`.
- `renders_acme_error_banner_with_detail_when_renewal_failed`.
- `transition_pending_to_healthy_renders_green_badge`.

### Acceptance command

`cargo test -p trilithon-adapters --test tls_inventory inventory_records_pending inventory_records_failed inventory_transitions && pnpm vitest run web/src/features/routes/RouteCard.test.tsx`

### Exit conditions

- All six new tests pass.
- The H17 hazard is mitigated by a distinct "issuing" surface.

### Audit kinds emitted

None directly. The Phase 7 applier emits `config.applied`; the inventory tick reuses `caddy.capability-probe-completed` per slice 14.2.

### Tracing events emitted

`caddy.capability-probe.completed`.

### Cross-references

- Hazards H17.
- PRD T1.9.

---

## Verification matrix

Every Phase 14 acceptance bar maps to a specific test. The table below lets the implementer cross-check completeness before declaring the phase shipped.

| Acceptance bar | Slice | Test name | Status |
|---|---|---|---|
| Certificate expiry green > 14 days | 14.6 | `renders_green_when_expiry_gt_14_days` | required |
| Amber within 14 days | 14.6 | `renders_amber_when_expiry_within_14_days` | required |
| Red within 3 days | 14.6 | `renders_red_when_expiry_within_3_days` | required |
| Red on failed renewal regardless of expiry | 14.6 | `renders_red_when_renewal_failed` | required |
| Health update within 30 seconds of transition | 14.3 | `flip_to_unreachable_propagates_within_30_seconds` | required |
| 5-minute TLS inventory cadence | 14.2 | `inventory_loop_ticks_every_300_seconds_within_tolerance` | required |
| Probe opt-out honoured | 14.4 | `route_with_disable_trilithon_probes_skips_tcp_probe` | required |
| Issuing distinct from applied | 14.8 | `renders_issuing_badge_for_freshly_added_host` | required |
| ACME error surfaces with detail | 14.8 | `renders_acme_error_banner_with_detail_when_renewal_failed` | required |
| Long-poll debounce within 1500 ms | 14.3 | `long_poll_flip_triggers_debounced_refresh_within_1500_ms` | required |

## Phase 14 exit checklist

- [ ] Every slice from 14.1 through 14.8 has shipped and its acceptance command passes.
- [ ] `just check` passes locally and in continuous integration.
- [ ] Certificates expiring within 14 days are flagged amber; within 3 days flagged red; failed renewals flagged red regardless of expiry.
- [ ] Health state updates within 30 seconds of an underlying transition.
- [ ] The user MAY disable Trilithon-side probes per route.
- [ ] "Issuing certificate" is a distinct visible state from "applied," and ACME errors surface with actionable messages (hazard H17).
- [ ] The `upstream.probe.completed` tracing event is documented in architecture §12.1 (added in the same commit that ships slice 14.3).

## Cross-cutting invariants

The following invariants hold across every slice of this phase. Implementers MUST preserve them.

- **UTC storage, local-time display.** Every column carrying a wall-clock time stores UTC Unix seconds. The web UI converts to the viewer's local time zone via `Intl.DateTimeFormat` (hazard H6). Specifications and tests MUST distinguish "stored time" from "displayed time."
- **Read-only HTTP surfaces.** The endpoints introduced in slice 14.5 are reads. They never mutate state, never write the audit log, and never enqueue mutations.
- **Capability-gated fields.** The TLS DNS-provider variant emits `LossyWarning::TlsDnsProviderUnavailable` at import time (Phase 13) AND surfaces the same condition at the inventory layer when the capability cache reports the provider missing. The two paths share the same capability-cache lookup.
- **No silent overwrite of Caddy-owned fields.** The TLS issuance state and the upstream-health state in Caddy's running config are owned by Caddy. Trilithon's reconciler MUST NOT include these fields in any apply payload. Architecture §7.2 and trait-signatures.md §2 (`CaddyClient::patch_config` `OwnershipMismatch`) enforce this.
- **Probe opt-out propagates uniformly.** The `disable_trilithon_probes` flag suppresses Trilithon's TCP probe but never suppresses Caddy-reported reachability. The merged surface MUST reflect Caddy's view in this case.

## Open questions

1. The 5-minute TLS inventory cadence is taken from the phase reference. Whether a shorter cadence is warranted for sites with very short certificate lifetimes (under 7 days) is unresolved.
2. The merge rule between Caddy-reported reachability and Trilithon TCP probes (slice 14.3 step 3.b.iii) treats "either unreachable" as authoritative. A future ADR may relax this to "Caddy reachable wins" when Trilithon's network path differs from Caddy's; this is not planned for V1.
3. The exact long-poll endpoint shape (`/reverse_proxy/upstreams` with what query parameters) is not fully specified by Caddy 2.11.2 documentation. The implementer MAY need to fall back to repeated `GET` polls if long-poll support proves unreliable; the slice 14.3 algorithm allows for this through the unconditional 30-second tick.
4. Whether to expose the full `last_error` string from ACME failures verbatim in the web UI raises a minor information-disclosure question for hosted deployments. For loopback-only V1 the risk is negligible; for V2 multi-instance fleet management this MUST be reviewed.
