# Phase 20 — Language-model propose mode — Implementation Slices

> Phase reference: [../phases/phase-20-gateway-propose-mode.md](../phases/phase-20-gateway-propose-mode.md)
> Roadmap: [../phases/phased-plan.md](../phases/phased-plan.md)
> Architecture: [architecture.md](../architecture/architecture.md), [trait-signatures.md](../architecture/trait-signatures.md)
> Voice rules: [PROMPT-spec-generation.md §9](../prompts/PROMPT-spec-generation.md)

## Inputs the implementer must have in context

- This file.
- The phase reference: `docs/phases/phase-20-gateway-propose-mode.md`.
- Architecture §4 (component view), §6.6 (audit-kind vocabulary), §6.8 (`proposals` table), §7.3 (proposal lifecycle), §11 (security posture), §12.1 (tracing vocabulary).
- Trait signatures: `core::tool_gateway::ToolGateway` (`invoke_propose`), `ToolGatewayError`.
- ADRs: ADR-0007 (proposal-based Docker discovery — same queue), ADR-0008 (bounded typed tool gateway), ADR-0012 (optimistic concurrency).
- PRD: T2.4 (language-model "propose" mode).
- Hazards: H8 (concurrent modification), H16 (prompt injection).

## Slice plan summary

| # | Slice | Primary files | Effort (h) | Depends on |
|---|-------|---------------|------------|------------|
| 20.1 | Proposals migration and store | `crates/adapters/migrations/0016_proposals.sql`, `crates/adapters/src/proposal_store.rs` | 6 | — |
| 20.2 | Propose scopes and propose-function catalogue | `crates/core/src/tool_gateway/scopes.rs`, `crates/core/src/tool_gateway/propose_functions.rs` | 4 | 20.1 |
| 20.3 | Proposal validation and creation | `crates/adapters/src/tool_gateway.rs`, `crates/core/src/proposals.rs` | 8 | 20.1, 20.2 |
| 20.4 | Proposals HTTP endpoints (list, approve, reject) | `crates/cli/src/http/proposals.rs` | 6 | 20.3 |
| 20.5 | Expiry sweeper and queue cap | `crates/adapters/src/proposal_expiry.rs` | 6 | 20.1 |
| 20.6 | Proposals web UI | `web/src/features/proposals/*` | 6 | 20.4 |
| 20.7 | End-to-end propose-approve-conflict scenarios | `crates/adapters/tests/proposals_*.rs` | 6 | 20.4, 20.5 |

---

## Slice 20.1 [standard] — Proposals migration and store

### Goal

Add migration `0016_proposals.sql` and implement `ProposalStore` covering insert, transition, list-pending-by-source, approve, reject, and expire operations. The schema is authoritative in architecture §6.8 — use that definition exactly (see ⚠️ note below).

### Entry conditions

- Phase 19 complete.
- The `mutations` queue from Phase 4/9 exists.

### Files to create or modify

- `core/crates/adapters/migrations/0016_proposals.sql`.
- `core/crates/adapters/src/proposal_store.rs`.
- `core/crates/adapters/src/lib.rs` — export `proposal_store`.
- `core/crates/core/src/proposals.rs` — `ProposalId`, `ProposalSource`, `ProposalStatus`.

### Signatures and shapes

⚠️ **Schema correction:** The original draft here diverged from architecture §6.8. The authoritative schema is below. Use this — do not re-invent columns.

```sql
-- core/crates/adapters/migrations/0016_proposals.sql
BEGIN;

CREATE TABLE proposals (
    id                  TEXT PRIMARY KEY,
    correlation_id      TEXT NOT NULL,
    source              TEXT NOT NULL CHECK (source IN ('docker', 'llm', 'import')),
    source_ref          TEXT,
    payload_json        TEXT NOT NULL,
    rationale           TEXT,
    submitted_at        INTEGER NOT NULL,
    expires_at          INTEGER NOT NULL,
    state               TEXT NOT NULL CHECK (state IN ('pending', 'approved', 'rejected', 'expired', 'superseded')),
    decided_by_kind     TEXT,
    decided_by_id       TEXT,
    decided_at          INTEGER,
    wildcard_callout    INTEGER NOT NULL DEFAULT 0,
    wildcard_ack_by     TEXT,
    wildcard_ack_at     INTEGER,
    resulting_mutation  TEXT REFERENCES mutations(id)
);

CREATE INDEX proposals_state ON proposals(state);
CREATE INDEX proposals_expires_at ON proposals(expires_at);
CREATE INDEX proposals_source ON proposals(source);

COMMIT;
```

```rust
//! `core/crates/core/src/proposals.rs`

use serde::{Deserialize, Serialize};

pub type ProposalId = ulid::Ulid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProposalSource {
    Docker,  // serialises as "docker"
    Llm,     // serialises as "llm"
    Import,  // serialises as "import"
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProposalState {
    Pending, Approved, Rejected, Expired, Superseded,
}
```

```rust
//! `core/crates/adapters/src/proposal_store.rs`

use rusqlite::Connection;
use trilithon_core::mutation::TypedMutation;
use trilithon_core::proposals::{ProposalId, ProposalSource, ProposalStatus};

#[derive(Debug, thiserror::Error)]
pub enum ProposalStoreError {
    #[error("storage error: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("proposal {0} not found")]
    NotFound(String),
    #[error("proposal {id} is in terminal state {status:?}")]
    Terminal { id: String, status: ProposalStatus },
    #[error("queue at cap (200 pending)")]
    QueueAtCap,
}

#[derive(Clone, Debug)]
pub struct ProposalRecord {
    pub id:                ProposalId,
    pub correlation_id:    String,
    pub source:            ProposalSource,
    pub source_ref:        Option<String>,
    pub payload_json:      String,          // canonical JSON of the TypedMutation
    pub rationale:         Option<String>,
    pub submitted_at:      i64,
    pub expires_at:        i64,
    pub state:             ProposalState,
    pub decided_by_kind:   Option<String>,
    pub decided_by_id:     Option<String>,
    pub decided_at:        Option<i64>,
    pub wildcard_callout:  bool,
    pub wildcard_ack_by:   Option<String>,
    pub wildcard_ack_at:   Option<i64>,
    pub resulting_mutation: Option<String>,
}

pub struct ProposalStore;

impl ProposalStore {
    pub fn insert(
        conn:                 &mut Connection,
        source:               ProposalSource,
        source_identifier:    &str,
        mutation:             &TypedMutation,
        basis_config_version: i64,
        expires_at:           i64,
        clock_now:            i64,
    ) -> Result<ProposalId, ProposalStoreError>;

    pub fn get(conn: &Connection, id: ProposalId) -> Result<ProposalRecord, ProposalStoreError>;

    pub fn list_pending(
        conn:    &Connection,
        source:  Option<ProposalSource>,
    ) -> Result<Vec<ProposalRecord>, ProposalStoreError>;

    pub fn count_pending(conn: &Connection) -> Result<u32, ProposalStoreError>;

    pub fn transition(
        conn:       &mut Connection,
        id:         ProposalId,
        new_status: ProposalStatus,
        decided_by: Option<&str>,
        clock_now:  i64,
    ) -> Result<(), ProposalStoreError>;

    pub fn expire_due(
        conn:      &mut Connection,
        clock_now: i64,
    ) -> Result<Vec<ProposalId>, ProposalStoreError>;
}
```

### Algorithm

`insert`:

1. `count = count_pending(conn)?`.
2. If `count >= 200`:
   - Run `expire_due` first; if still ≥ 200, evict the oldest pending row by transitioning it to `Superseded`.
   - If no rows can be evicted (all are within their expiry window), return `QueueAtCap`.
3. `proposal_id = Ulid::new()`.
4. `INSERT INTO proposals ...`.
5. Return `proposal_id`.

`transition`:

1. `BEGIN IMMEDIATE`.
2. `SELECT status FROM proposals WHERE proposal_id = ?`. On 0 rows: rollback, `NotFound`.
3. If current status is not `Pending`: rollback, `Terminal { id, status: current }`.
4. `UPDATE proposals SET status = ?, decided_at = ?, decided_by = ? WHERE proposal_id = ?`.
5. `COMMIT`.

`expire_due`:

1. `SELECT proposal_id FROM proposals WHERE status = 'pending' AND expires_at_unix_seconds < ?` → `expired_ids`.
2. For each id, `UPDATE ... SET status = 'expired', decided_at = clock_now WHERE proposal_id = ?`.
3. Return `expired_ids`.

### Tests

- `core/crates/adapters/tests/proposal_store.rs`:
  - `insert_persists_typed_record_and_returns_id`.
  - `get_returns_inserted_record`.
  - `list_pending_filters_by_source`.
  - `transition_pending_to_approved_succeeds`.
  - `transition_terminal_state_returns_terminal_error`.
  - `expire_due_transitions_only_past_due_pending`.
  - `count_pending_excludes_terminal`.
  - `insert_at_cap_evicts_oldest_or_returns_queue_at_cap`.

### Acceptance command

```
cargo test -p trilithon-adapters --test proposal_store
```

### Exit conditions

- All eight tests pass.
- Migration runs idempotently.

### Audit kinds emitted

None directly in this slice. Approval/rejection/expiry rows are written by slices 20.4 and 20.5.

### Tracing events emitted

None directly.

### Cross-references

- ADR-0007.
- Architecture §6.8, §7.3.

---

## Slice 20.2 [standard] — Propose scopes and propose-function catalogue

### Goal

Extend `Scope` with `propose.routes`, `propose.policies`, `propose.upstreams`. Define `ProposeFunction` enum with `propose_create_route`, `propose_update_route`, `propose_delete_route`, `propose_attach_policy`. Existing read scopes remain unchanged; the closed scope set grows from eight to eleven.

### Entry conditions

- Slice 20.1 shipped.

### Files to create or modify

- `core/crates/core/src/tool_gateway/scopes.rs` — extend.
- `core/crates/core/src/tool_gateway/propose_functions.rs` — new module.
- `core/crates/core/src/tool_gateway/mod.rs` — re-export.
- `docs/schemas/gateway/propose_create_route.in.json`, `.out.json`, and equivalent for the other three.

### Signatures and shapes

```rust
//! Addition to `core/crates/core/src/tool_gateway/scopes.rs`

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    // ... eight read scopes from Phase 19 ...
    #[serde(rename = "propose.routes")]    ProposeRoutes,
    #[serde(rename = "propose.policies")]  ProposePolicies,
    #[serde(rename = "propose.upstreams")] ProposeUpstreams,
}

impl Scope {
    pub const V1_PROPOSE_MODE_SCOPES: [Scope; 3] = [
        Scope::ProposeRoutes, Scope::ProposePolicies, Scope::ProposeUpstreams,
    ];

    pub const V1_ALL_SCOPES: [Scope; 11] = [
        // eight read + three propose
    ];
}
```

```rust
//! `core/crates/core/src/tool_gateway/propose_functions.rs`

use serde::{Deserialize, Serialize};
use crate::tool_gateway::scopes::Scope;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposeFunction {
    ProposeCreateRoute,
    ProposeUpdateRoute,
    ProposeDeleteRoute,
    ProposeAttachPolicy,
}

impl ProposeFunction {
    pub const ALL: [ProposeFunction; 4] = [
        Self::ProposeCreateRoute, Self::ProposeUpdateRoute,
        Self::ProposeDeleteRoute, Self::ProposeAttachPolicy,
    ];

    pub fn required_scope(self) -> Scope {
        match self {
            Self::ProposeCreateRoute | Self::ProposeUpdateRoute |
            Self::ProposeDeleteRoute  => Scope::ProposeRoutes,
            Self::ProposeAttachPolicy => Scope::ProposePolicies,
        }
    }

    pub fn input_schema(self) -> &'static str;
    pub fn output_schema(self) -> &'static str;
    pub fn wire_name(self) -> &'static str;
}
```

### Algorithm

1. The wire form for `Scope::ProposeUpstreams` is `"propose.upstreams"` even though no V1 propose function requires it. The scope is reserved for Tier 3 work; it is part of the closed set so a token can be issued with it ahead of time.
2. The `V1_ALL_SCOPES` constant is asserted at compile time to have length 11.

### Tests

- `core/crates/core/src/tool_gateway/scopes.rs` `mod tests`:
  - `v1_all_scopes_count_eleven`.
  - `propose_scopes_round_trip_through_wire`.
- `core/crates/core/src/tool_gateway/propose_functions.rs` `mod tests`:
  - `every_propose_function_has_a_required_scope`.
  - `every_input_schema_is_valid_json_schema_2020_12`.
  - `every_output_schema_is_valid_json_schema_2020_12`.
  - `wire_names_are_snake_case_and_unique_within_propose_set`.

### Acceptance command

```
cargo test -p trilithon-core tool_gateway::scopes::tests tool_gateway::propose_functions::tests
```

### Exit conditions

- All six tests pass.
- The closed scope set length equals 11.

### Audit kinds emitted

None.

### Tracing events emitted

None.

### Cross-references

- ADR-0008.
- Phase 20 task: "Add propose scopes to the gateway."

---

## Slice 20.3 [cross-cutting] — Proposal validation and creation

### Goal

Implement `ToolGateway::invoke_propose`. Each call validates the proposed mutation through the standard validation pipeline (capability gating, policy enforcement). On success, insert a `pending` proposal carrying the basis `config_version`. Reject at validation time on any policy violation.

### Entry conditions

- Slices 20.1, 20.2 shipped.
- The standard `validate_mutation(&TypedMutation, &Capabilities, &Policies, &CurrentDesiredState)` pipeline from Phase 4/18 is callable.

### Files to create or modify

- `core/crates/adapters/src/tool_gateway.rs` — implement `invoke_propose`.
- `core/crates/core/src/proposals.rs` — `ProposalCreated { id, validation }`.

### Signatures and shapes

```rust
//! Addition to `core/crates/core/src/proposals.rs`

#[derive(Clone, Debug, serde::Serialize)]
pub struct ProposalCreated {
    pub proposal_id:           crate::proposals::ProposalId,
    pub validation:            crate::validation::ValidationReport,
    pub expires_at_unix_seconds: i64,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ValidationReport {
    pub warnings: Vec<crate::policy::LossyWarning>,
}
```

```rust
//! Addition to `core/crates/adapters/src/tool_gateway.rs`

#[async_trait::async_trait]
impl trilithon_core::tool_gateway::ToolGateway for DefaultToolGateway {
    async fn invoke_propose(
        &self,
        token:    &SessionToken,
        function: trilithon_core::tool_gateway::propose_functions::ProposeFunction,
        args:     serde_json::Value,
    ) -> Result<trilithon_core::proposals::ProposalId, trilithon_core::tool_gateway::ToolGatewayError>;
}
```

### Algorithm

`invoke_propose`:

1. Check `token.scopes.contains(&function.required_scope())`. If absent, return `Unauthorized`.
2. Apply per-token rate limit (slice 19.3 limiter, with the propose-mode capacity from configuration).
3. Validate `args` against the function's input schema.
4. Decode `args` into a `TypedMutation`:
   - `ProposeCreateRoute` → `TypedMutation::CreateRoute(...)`.
   - `ProposeUpdateRoute` → `TypedMutation::UpdateRoute(...)`.
   - `ProposeDeleteRoute` → `TypedMutation::DeleteRoute(...)`.
   - `ProposeAttachPolicy` → `TypedMutation::AttachPolicy(...)`.
5. Read the current `config_version` and `DesiredState`.
6. Run `validate_mutation(&mutation, &capabilities, &policies, &current_state)`. On `ValidationErrorSet` return `ToolGatewayError::PromptInjectionRefused { detail: <serialised errors> }` is wrong — instead return a typed schema-rejection error. This slice introduces a fifth `ToolGatewayError` variant `ValidationFailed { errors: ValidationErrorSet }` if the existing four are insufficient. Update `trait-signatures.md` §7 in the same commit.
7. On `Ok(report)`: compute `expires_at = clock_now + config.tool_gateway.proposal_ttl_seconds.unwrap_or(24 * 3600)`.
8. `ProposalStore::insert(conn, ProposalSource::LanguageModel, &token.token_id, &mutation, current_config_version, expires_at, clock_now)?`.
9. Return `Ok(proposal_id)`.

### Tests

- `core/crates/adapters/tests/tool_gateway_propose.rs`:
  - `propose_create_route_with_correct_scope_creates_pending_proposal`.
  - `propose_without_scope_returns_unauthorized`.
  - `propose_with_invalid_args_returns_validation_failed`.
  - `propose_violating_attached_policy_rejected_at_validation` — attach `public-admin@1` to a route, then propose an update that strips HSTS; assert rejection without queue insert.
  - `propose_inserts_audit_row_mutation_proposed`.
  - `propose_carries_basis_config_version`.

### Acceptance command

```
cargo test -p trilithon-adapters --test tool_gateway_propose
```

### Exit conditions

- All six tests pass.
- A proposal that violates an attached policy is rejected before insertion; no row appears in `proposals`.
- One audit row with `kind = "mutation.proposed"` is written per accepted proposal.

### Audit kinds emitted

Per §6.6: `mutation.proposed`, `tool-gateway.tool-invoked`.

### Tracing events emitted

Per §12.1: `tool-gateway.invocation.started`, `tool-gateway.invocation.completed`, `proposal.received`.

### Cross-references

- ADR-0008, ADR-0007.
- Phase 20 tasks: "Implement proposal validation in the standard pipeline," "Implement gateway propose functions."

---

## Slice 20.4 [cross-cutting] — Proposals HTTP endpoints (list, approve, reject)

### Goal

Implement `GET /api/v1/proposals?source=...`, `POST /api/v1/proposals/{id}/approve`, `POST /api/v1/proposals/{id}/reject`. Approval requires an authenticated human session — a tool-gateway token MUST NOT pass. Approval runs the mutation through the standard apply pipeline; a stale basis flows through the Phase 17 conflict path.

### Entry conditions

- Slices 20.1, 20.3 shipped.
- The Phase 11 web session middleware exists.
- The Phase 17 conflict-handling path is operational.

### Files to create or modify

- `core/crates/cli/src/http/proposals.rs` — three handlers.
- `core/crates/cli/src/http/router.rs` — mount endpoints.
- `core/crates/cli/src/http/auth.rs` — `RequireUserSession` extractor that rejects gateway-token credentials.

### Signatures and shapes

```rust
//! `core/crates/cli/src/http/proposals.rs`

use axum::{Json, extract::{Path, Query}, http::StatusCode};
use serde::{Deserialize, Serialize};
use trilithon_core::proposals::{ProposalId, ProposalSource};

#[derive(Debug, Deserialize)]
pub struct ListQuery { pub source: Option<ProposalSource> }

#[derive(Debug, Serialize)]
pub struct ListResponse { pub proposals: Vec<ProposalSummary> }

#[derive(Debug, Serialize)]
pub struct ProposalSummary {
    pub proposal_id:          String,
    pub source:               ProposalSource,
    pub source_identifier:    String,
    pub mutation:             trilithon_core::mutation::TypedMutation,
    pub basis_config_version: i64,
    pub expires_at_unix_seconds: i64,
    pub status:               trilithon_core::proposals::ProposalStatus,
    pub created_at:           i64,
}

pub async fn list_proposals(
    user_session:       crate::http::auth::RequireUserSession,
    Query(q):           Query<ListQuery>,
    state:              axum::extract::State<crate::AppState>,
) -> (StatusCode, Json<ListResponse>);

#[derive(Debug, Deserialize)]
pub struct ApproveRequest { pub expected_basis_version: i64 }

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ApproveResponse {
    Ok       { mutation_id: String, new_version: i64 },
    Conflict { rebase_token: String, rebase_plan: trilithon_core::concurrency::RebasePlan },
    Gone     { reason: &'static str }, // "already-decided" | "expired"
}

pub async fn approve_proposal(
    user_session: crate::http::auth::RequireUserSession,
    Path(id):     Path<String>,
    Json(req):    Json<ApproveRequest>,
    state:        axum::extract::State<crate::AppState>,
) -> (StatusCode, Json<ApproveResponse>);

pub async fn reject_proposal(
    user_session: crate::http::auth::RequireUserSession,
    Path(id):     Path<String>,
    state:        axum::extract::State<crate::AppState>,
) -> (StatusCode, Json<serde_json::Value>);
```

### Algorithm

`RequireUserSession` extractor:

1. Read the session cookie. If absent or invalid, reject with 401.
2. If the request's authentication source is a gateway token (header `Authorization: Bearer trlt_...`), reject with 401 unconditionally — gateway tokens can read but cannot approve.

`approve_proposal`:

1. Resolve `id` to `ProposalRecord`. On `NotFound`: 404.
2. If `record.status != Pending`: return `(410, ApproveResponse::Gone { reason: "already-decided" })`.
3. If `record.expires_at < clock_now`: transition to `Expired`, write `proposal.expired` audit, return `(410, Gone { reason: "expired" })`.
4. If `req.expected_basis_version != record.basis_config_version`: return 409 (the user UI is out of date).
5. Submit `record.mutation` through the standard apply pipeline with `expected_version = record.basis_config_version`.
6. On `SnapshotWriteError::Conflict`:
   - The Phase 17 conflict path produces a `RebasePlan` and rebase token.
   - Return `(409, ApproveResponse::Conflict { rebase_token, rebase_plan })`.
   - Do NOT mark the proposal approved; it stays `Pending` until the rebase completes.
7. On success:
   - `ProposalStore::transition(id, Approved, decided_by: user.id, clock_now)`.
   - Write `proposal.approved` audit row.
   - Return `(200, ApproveResponse::Ok { mutation_id, new_version })`.

`reject_proposal`:

1. `ProposalStore::transition(id, Rejected, decided_by: user.id, clock_now)`.
2. Write `proposal.rejected` audit row.
3. Return 204.

### Tests

- `core/crates/cli/tests/proposals_endpoints.rs`:
  - `list_returns_pending_with_source_filter`.
  - `approve_with_user_session_succeeds`.
  - `approve_with_gateway_token_returns_401`.
  - `approve_already_decided_returns_410`.
  - `approve_expired_returns_410_and_writes_expired_audit`.
  - `approve_with_stale_basis_returns_409_with_fresh_rebase_plan`.
  - `reject_with_user_session_writes_rejected_audit`.

### Acceptance command

```
cargo test -p trilithon-cli --test proposals_endpoints
```

### Exit conditions

- All seven tests pass.
- Approval requires a user session, never a gateway token.
- A stale basis routes through the Phase 17 conflict path.

### Audit kinds emitted

Per §6.6: `proposal.approved`, `proposal.rejected`, `proposal.expired`, `mutation.applied`, `mutation.conflicted`.

### Tracing events emitted

Per §12.1: `http.request.received`, `http.request.completed`, `proposal.approved`, `proposal.rejected`, `apply.started`, `apply.succeeded`, `apply.failed`.

### Cross-references

- ADR-0008, ADR-0012.
- Phase 20 tasks: "Implement `GET /api/v1/proposals`," "Implement `POST /api/v1/proposals/{id}/approve` and `POST /api/v1/proposals/{id}/reject`."

---

## Slice 20.5 [cross-cutting] — Expiry sweeper and queue cap

### Goal

A background task transitions expired proposals to `expired` and writes `proposal.expired` audit rows. The sweeper runs every 60 seconds. The queue cap of 200 pending entries is enforced at insert time per slice 20.1; oldest-first eviction transitions to `superseded`. Per-token rate limit applies to propose calls.

### Entry conditions

- Slices 20.1, 20.3 shipped.

### Files to create or modify

- `core/crates/adapters/src/proposal_expiry.rs` — sweeper task.
- `core/crates/cli/src/runtime.rs` — spawn the sweeper at daemon start.
- `core/crates/core/src/config.rs` — add `[tool_gateway] proposal_ttl_seconds: u64` (default 86400 = 24 hours, min 60, max 7 * 86400).

### Signatures and shapes

```rust
//! `core/crates/adapters/src/proposal_expiry.rs`

use std::sync::Arc;
use std::time::Duration;
use trilithon_core::time::Clock;

pub struct ProposalExpirySweeper<C: Clock + 'static> {
    pool:     Arc<rusqlite::Connection>,  // serialised via Arc<Mutex<...>> in practice
    audit:    Arc<crate::audit_log_store::AuditLogStore>,
    clock:    C,
    interval: Duration,
}

impl<C: Clock + Send + Sync + 'static> ProposalExpirySweeper<C> {
    pub fn spawn(
        self,
        shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()>;

    async fn tick(&self) -> Result<u32, ProposalSweepError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ProposalSweepError {
    #[error("storage: {0}")]
    Storage(#[from] crate::proposal_store::ProposalStoreError),
}
```

### Algorithm

`tick`:

1. `now = self.clock.now().0 as i64`.
2. `expired = ProposalStore::expire_due(conn, now)?`.
3. For each id in `expired`: write one audit row `kind = "proposal.expired"` with `notes = { proposal_id }`.
4. Return `expired.len() as u32`.

`spawn`:

1. Spawn a Tokio task running an interval loop with `self.interval` (default 60 s).
2. On each tick, call `self.tick().await`. Log a tracing warning on error; do not exit the loop.
3. On shutdown signal, exit cleanly.

### Tests

- `core/crates/adapters/tests/proposal_expiry.rs`:
  - `tick_transitions_only_past_due_pending`.
  - `tick_writes_one_proposal_expired_audit_per_id`.
  - `tick_with_no_due_returns_zero`.
  - `repeated_ticks_do_not_double_expire`.
- `core/crates/adapters/tests/proposal_queue_cap.rs`:
  - `insert_at_cap_evicts_oldest_pending_to_superseded`.
  - `insert_at_cap_with_no_evictable_returns_queue_at_cap`.
- `core/crates/core/src/config.rs` `mod tests`:
  - `proposal_ttl_default_is_24h`.
  - `proposal_ttl_below_min_rejected`.
  - `proposal_ttl_above_max_rejected`.

### Acceptance command

```
cargo test -p trilithon-adapters --test proposal_expiry --test proposal_queue_cap && \
cargo test -p trilithon-core config::tests
```

### Exit conditions

- All nine tests pass.
- The sweeper runs every 60 seconds in the daemon and never blocks the apply path.

### Audit kinds emitted

Per §6.6: `proposal.expired`.

### Tracing events emitted

None new.

### Cross-references

- ADR-0008.
- Phase 20 tasks: "Implement proposal expiry," "Per-token proposal rate limit and queue cap."

---

## Slice 20.6 [standard] — Proposals web UI

### Goal

Land the Proposals page: list pending proposals with source attribution, intent, and a diff preview. Approve and Reject buttons require explicit confirmation.

### Entry conditions

- Slice 20.4 shipped.

### Files to create or modify

- `web/src/features/proposals/types.ts`.
- `web/src/features/proposals/ProposalsPage.tsx` and `.test.tsx`.
- `web/src/features/proposals/ProposalRow.tsx` and `.test.tsx`.
- `web/src/features/proposals/ProposalDiff.tsx`.
- `web/src/features/proposals/useProposals.ts`.

### Signatures and shapes

```typescript
// web/src/features/proposals/types.ts

export type ProposalSource = 'language-model' | 'docker-discovery' | 'caddyfile-import';
export type ProposalStatus = 'pending' | 'approved' | 'rejected' | 'expired' | 'superseded';

export interface ProposalSummary {
  readonly proposal_id: string;
  readonly source: ProposalSource;
  readonly source_identifier: string;
  readonly mutation: unknown;            // typed in `/features/mutation/types.ts`
  readonly basis_config_version: number;
  readonly expires_at_unix_seconds: number;
  readonly status: ProposalStatus;
  readonly created_at: number;
}
```

```typescript
// web/src/features/proposals/ProposalsPage.tsx
export function ProposalsPage(): JSX.Element;

// web/src/features/proposals/ProposalRow.tsx
export function ProposalRow(props: {
  proposal: ProposalSummary;
  onApprove: (id: string, basis: number) => void;
  onReject:  (id: string) => void;
}): JSX.Element;
```

### Algorithm

`ProposalsPage`:

1. `useQuery` `GET /api/v1/proposals?status=pending`.
2. Render rows grouped by `source`, sorted by `created_at` descending.
3. Each row delegates to `ProposalRow` with `onApprove`/`onReject` handlers.

`ProposalRow`:

1. Render the source attribution (`Language model`, `Docker discovery`, `Caddyfile import`).
2. Render a one-line intent summary derived from `mutation.type`.
3. Render `<ProposalDiff>` showing the rendered Caddy JSON delta.
4. The Approve button opens a confirmation dialog with the literal text `Approve this proposal? Trilithon will apply the change immediately.`. Confirm calls `onApprove`.
5. The Reject button opens a confirmation dialog with the literal text `Reject this proposal? It will be removed from the queue.`. Confirm calls `onReject`.

### Tests

- `web/src/features/proposals/ProposalsPage.test.tsx`:
  - `lists_pending_proposals_grouped_by_source`.
  - `axe_finds_zero_violations`.
- `web/src/features/proposals/ProposalRow.test.tsx`:
  - `approve_button_requires_confirmation_before_calling_handler`.
  - `reject_button_requires_confirmation_before_calling_handler`.
  - `approve_handler_receives_proposal_id_and_basis_version`.
  - `wildcard_match_security_warning_renders_when_present` — placeholder for the Phase 21 wildcard banner.

### Acceptance command

```
cd web && pnpm typecheck && pnpm lint && pnpm test --run
```

### Exit conditions

- All six Vitest tests pass.
- `pnpm typecheck` succeeds.
- Approve and reject both require confirmation.

### Audit kinds emitted

None directly from the web tier.

### Tracing events emitted

None directly.

### Cross-references

- Phase 20 task: "Implement the Proposals page."
- ADR-0004.

---

## Slice 20.7 [standard] — End-to-end propose-approve-conflict scenarios

### Goal

Land integration tests for: policy-violation rejection at proposal time; model cannot approve its own proposal; stale-basis approval flows through the conflict path.

### Entry conditions

- Slices 20.4, 20.5, 20.6 shipped.

### Files to create or modify

- `core/crates/adapters/tests/proposals_policy_violation.rs`.
- `core/crates/adapters/tests/proposals_model_cannot_approve.rs`.
- `core/crates/adapters/tests/proposals_stale_basis_conflict.rs`.

### Signatures and shapes

No new public surface. Each test is a `#[tokio::test]` that drives the daemon through the scenario.

### Algorithm

`proposals_policy_violation.rs`:

1. Attach `public-admin@1` to a route.
2. Token A (with `propose.routes`) calls `propose_update_route` to remove HSTS.
3. Assert the call returns `ToolGatewayError::ValidationFailed { ... }` (or the equivalent typed error chosen in slice 20.3).
4. Assert no row in `proposals`.
5. Assert one `mutation.rejected` audit row.

`proposals_model_cannot_approve.rs`:

1. Token A creates a proposal.
2. Token A attempts `POST /api/v1/proposals/<id>/approve` with the bearer header.
3. Assert 401.
4. Authenticated user session calls `approve` and succeeds.

`proposals_stale_basis_conflict.rs`:

1. Token A creates a proposal at `basis_config_version = N`.
2. A separate user mutation lands at version `N+1`.
3. The user approves the proposal with `expected_basis_version = N`.
4. The approval fires through the apply pipeline; the snapshot writer returns `Conflict`.
5. Assert 409 with a fresh `RebasePlan` and `rebase_token`.
6. Assert the proposal status remains `Pending`.
7. Assert one `mutation.conflicted` audit row.

### Tests

The three integration tests above. Each is its own `tests/*.rs` file with one `#[tokio::test]`.

### Acceptance command

```
cargo test -p trilithon-adapters \
  --test proposals_policy_violation \
  --test proposals_model_cannot_approve \
  --test proposals_stale_basis_conflict
```

### Exit conditions

- All three tests pass.
- No flake on ten consecutive runs.

### Audit kinds emitted

Per §6.6: `mutation.proposed`, `mutation.rejected`, `proposal.approved`, `mutation.conflicted`, `mutation.applied`.

### Tracing events emitted

Per §12.1: `tool-gateway.invocation.started`, `tool-gateway.invocation.completed`, `proposal.received`, `proposal.approved`, `apply.started`, `apply.failed`.

### Cross-references

- Phase 20 task block "Tests."
- Hazard H8.

---

## Phase exit checklist

- [ ] `just check` passes locally and in continuous integration.
- [ ] The model cannot apply a proposal directly. Approval requires an authenticated user action.
- [ ] Proposals expire after a configurable window (default 24 hours).
- [ ] The model cannot bypass policy presets: a proposal that would violate an attached policy is rejected at validation.

## Open questions

- Slice 20.3 introduces a fifth `ToolGatewayError` variant `ValidationFailed { errors: ValidationErrorSet }`. The trait-signatures.md §7 closed-set declaration MUST be updated in the same commit. If the project prefers to overload `PromptInjectionRefused` instead, the planner should flag the divergence before slice 20.3 lands.
