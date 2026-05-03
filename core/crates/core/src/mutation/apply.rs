//! Entry point for pure desired-state mutation application.
//!
//! [`apply_mutation`] is the single public surface exposed by this module.
//! It is deliberately free of I/O and clock reads — all time values that need
//! to flow in must be carried by the mutation payload itself.

use crate::audit::AuditEvent;
use crate::caddy::capabilities::CapabilitySet;
use crate::model::desired_state::DesiredState;
use crate::model::primitive::JsonPointer;
use crate::model::route::RoutePolicyAttachment;
use crate::mutation::{
    capability::check_capabilities,
    error::MutationError,
    outcome::{Diff, DiffChange, MutationOutcome},
    patches::{RoutePatch, UpstreamPatch},
    types::{Mutation, MutationKind},
    validate,
};

/// Apply a mutation against an immutable desired state. Pure: no I/O,
/// no clock reads except via caller-supplied `UnixSeconds` already on the
/// mutation payload.
///
/// # Errors
///
/// - [`MutationError::Conflict`] if `mutation.expected_version()` differs from
///   `state.version`.
/// - [`MutationError::CapabilityMissing`] when the mutation references a Caddy
///   module absent from `capabilities`.
/// - [`MutationError::Validation`] for schema or pre-condition failures.
/// - [`MutationError::Forbidden`] for operations blocked by policy or phase constraints.
pub fn apply_mutation(
    state: &DesiredState,
    mutation: &Mutation,
    capabilities: &CapabilitySet,
) -> Result<MutationOutcome, MutationError> {
    // 1. Concurrency check.
    let expected = mutation.expected_version();
    if expected != state.version {
        return Err(MutationError::Conflict {
            observed_version: state.version,
            expected_version: expected,
        });
    }

    // 2. Capability check.
    check_capabilities(mutation, capabilities)?;

    // 3. Schema and pre-conditions.
    validate::pre_conditions(state, mutation)?;

    // 4. Build new state (clone + increment version).
    let mut new_state = state.clone();
    new_state.version = state.version + 1;

    // 5. State application + diff building.
    let changes = apply_variant(state, &mut new_state, mutation)?;

    // 6. Audit kind selection.
    let kind = audit_event_for(mutation.kind());

    Ok(MutationOutcome {
        new_state,
        diff: Diff { changes },
        kind,
    })
}

/// Apply the mutation payload to `new_state` and return the diff changes.
///
/// # Errors
///
/// Propagates errors from internal operations.
fn apply_variant(
    state: &DesiredState,
    new_state: &mut DesiredState,
    mutation: &Mutation,
) -> Result<Vec<DiffChange>, MutationError> {
    match mutation {
        Mutation::CreateRoute { route, .. } => Ok(apply_create_route(new_state, route)),
        Mutation::UpdateRoute { id, patch, .. } => apply_route_patch(state, new_state, id, patch),
        Mutation::DeleteRoute { id, .. } => Ok(apply_delete_route(state, new_state, id)),
        Mutation::CreateUpstream { upstream, .. } => Ok(apply_create_upstream(new_state, upstream)),
        Mutation::UpdateUpstream { id, patch, .. } => {
            apply_upstream_patch(state, new_state, id, patch)
        }
        Mutation::DeleteUpstream { id, .. } => Ok(apply_delete_upstream(state, new_state, id)),
        Mutation::AttachPolicy {
            route_id,
            preset_id,
            preset_version,
            ..
        } => apply_attach_policy(new_state, route_id, preset_id, *preset_version),
        Mutation::DetachPolicy { route_id, .. } => apply_detach_policy(new_state, route_id),
        Mutation::UpgradePolicy {
            route_id,
            to_version,
            ..
        } => apply_upgrade_policy(new_state, route_id, *to_version),
        Mutation::SetGlobalConfig { patch, .. } => Ok(apply_set_global_config(new_state, patch)),
        Mutation::SetTlsConfig { patch, .. } => Ok(apply_set_tls_config(new_state, patch)),
        Mutation::ImportFromCaddyfile { parsed, .. } => {
            Ok(apply_import_caddyfile(new_state, parsed))
        }
        // Rollback always returns Forbidden in Phase 4 — handled by pre_conditions.
        // This arm is unreachable in practice, but must be exhaustive.
        Mutation::Rollback { .. } => Err(MutationError::Forbidden {
            reason: crate::mutation::error::ForbiddenReason::RollbackTargetUnknown,
        }),
    }
}

fn apply_create_route(
    new_state: &mut DesiredState,
    route: &crate::model::route::Route,
) -> Vec<DiffChange> {
    let pointer = JsonPointer::root().push("routes").push(route.id.as_str());
    let after = to_json(route);
    new_state.routes.insert(route.id.clone(), route.clone());
    vec![DiffChange {
        path: pointer,
        before: None,
        after,
    }]
}

fn apply_delete_route(
    state: &DesiredState,
    new_state: &mut DesiredState,
    id: &crate::model::identifiers::RouteId,
) -> Vec<DiffChange> {
    let pointer = JsonPointer::root().push("routes").push(id.as_str());
    let before = state.routes.get(id).and_then(to_json);
    new_state.routes.remove(id);
    vec![DiffChange {
        path: pointer,
        before,
        after: None,
    }]
}

fn apply_create_upstream(
    new_state: &mut DesiredState,
    upstream: &crate::model::upstream::Upstream,
) -> Vec<DiffChange> {
    let pointer = JsonPointer::root()
        .push("upstreams")
        .push(upstream.id.as_str());
    let after = to_json(upstream);
    new_state
        .upstreams
        .insert(upstream.id.clone(), upstream.clone());
    vec![DiffChange {
        path: pointer,
        before: None,
        after,
    }]
}

fn apply_delete_upstream(
    state: &DesiredState,
    new_state: &mut DesiredState,
    id: &crate::model::identifiers::UpstreamId,
) -> Vec<DiffChange> {
    let pointer = JsonPointer::root().push("upstreams").push(id.as_str());
    let before = state.upstreams.get(id).and_then(to_json);
    new_state.upstreams.remove(id);
    vec![DiffChange {
        path: pointer,
        before,
        after: None,
    }]
}

/// Build the `policy_attachment` JSON pointer and snapshot the before-value.
///
/// Shared preamble for the three policy mutation helpers (`attach`, `detach`,
/// `upgrade`).  The caller is responsible for the `get_mut` call so that
/// Rust's borrow checker can see the immutable read (before) ends before the
/// mutable borrow begins.
fn policy_attachment_preamble(
    new_state: &DesiredState,
    route_id: &crate::model::identifiers::RouteId,
) -> (JsonPointer, Option<serde_json::Value>) {
    let pointer = JsonPointer::root()
        .push("routes")
        .push(route_id.as_str())
        .push("policy_attachment");
    let before = new_state
        .routes
        .get(route_id)
        .and_then(|r| r.policy_attachment.as_ref())
        .and_then(to_json);
    (pointer, before)
}

fn apply_attach_policy(
    new_state: &mut DesiredState,
    route_id: &crate::model::identifiers::RouteId,
    preset_id: &crate::model::identifiers::PresetId,
    preset_version: u32,
) -> Result<Vec<DiffChange>, MutationError> {
    let (pointer, before) = policy_attachment_preamble(new_state, route_id);
    let route = new_state
        .routes
        .get_mut(route_id)
        .ok_or_else(|| missing_route_error(route_id.as_str()))?;
    route.policy_attachment = Some(RoutePolicyAttachment {
        preset_id: preset_id.clone(),
        preset_version,
    });
    let after = route.policy_attachment.as_ref().and_then(to_json);
    Ok(vec![DiffChange {
        path: pointer,
        before,
        after,
    }])
}

fn apply_detach_policy(
    new_state: &mut DesiredState,
    route_id: &crate::model::identifiers::RouteId,
) -> Result<Vec<DiffChange>, MutationError> {
    let (pointer, before) = policy_attachment_preamble(new_state, route_id);
    let route = new_state
        .routes
        .get_mut(route_id)
        .ok_or_else(|| missing_route_error(route_id.as_str()))?;
    route.policy_attachment = None;
    Ok(vec![DiffChange {
        path: pointer,
        before,
        after: None,
    }])
}

fn apply_upgrade_policy(
    new_state: &mut DesiredState,
    route_id: &crate::model::identifiers::RouteId,
    to_version: u32,
) -> Result<Vec<DiffChange>, MutationError> {
    let (pointer, before) = policy_attachment_preamble(new_state, route_id);
    let route = new_state
        .routes
        .get_mut(route_id)
        .ok_or_else(|| missing_route_error(route_id.as_str()))?;
    if let Some(attachment) = route.policy_attachment.as_mut() {
        attachment.preset_version = to_version;
    }
    let after = route.policy_attachment.as_ref().and_then(to_json);
    Ok(vec![DiffChange {
        path: pointer,
        before,
        after,
    }])
}

fn apply_set_global_config(
    new_state: &mut DesiredState,
    global_patch: &crate::model::global::GlobalConfigPatch,
) -> Vec<DiffChange> {
    let pointer = JsonPointer::root().push("global");
    let before = to_json(&new_state.global);
    if let Some(admin_listen) = &global_patch.admin_listen {
        new_state.global.admin_listen.clone_from(admin_listen);
    }
    if let Some(default_sni) = &global_patch.default_sni {
        new_state.global.default_sni.clone_from(default_sni);
    }
    if let Some(log_level) = &global_patch.log_level {
        new_state.global.log_level.clone_from(log_level);
    }
    let after = to_json(&new_state.global);
    vec![DiffChange {
        path: pointer,
        before,
        after,
    }]
}

fn apply_set_tls_config(
    new_state: &mut DesiredState,
    tls_patch: &crate::model::tls::TlsConfigPatch,
) -> Vec<DiffChange> {
    let pointer = JsonPointer::root().push("tls");
    let before = to_json(&new_state.tls);
    if let Some(email) = &tls_patch.email {
        new_state.tls.email.clone_from(email);
    }
    if let Some(on_demand_enabled) = tls_patch.on_demand_enabled {
        new_state.tls.on_demand_enabled = on_demand_enabled;
    }
    if let Some(on_demand_ask_url) = &tls_patch.on_demand_ask_url {
        new_state
            .tls
            .on_demand_ask_url
            .clone_from(on_demand_ask_url);
    }
    if let Some(default_issuer) = &tls_patch.default_issuer {
        new_state.tls.default_issuer.clone_from(default_issuer);
    }
    let after = to_json(&new_state.tls);
    vec![DiffChange {
        path: pointer,
        before,
        after,
    }]
}

fn apply_import_caddyfile(
    new_state: &mut DesiredState,
    parsed: &crate::mutation::patches::ParsedCaddyfile,
) -> Vec<DiffChange> {
    let root = JsonPointer::root();
    let before_routes = to_json(&new_state.routes);
    let before_upstreams = to_json(&new_state.upstreams);
    for route in &parsed.routes {
        new_state.routes.insert(route.id.clone(), route.clone());
    }
    for upstream in &parsed.upstreams {
        new_state
            .upstreams
            .insert(upstream.id.clone(), upstream.clone());
    }
    let after_routes = to_json(&new_state.routes);
    let after_upstreams = to_json(&new_state.upstreams);
    vec![
        DiffChange {
            path: root.push("routes"),
            before: before_routes,
            after: after_routes,
        },
        DiffChange {
            path: root.push("upstreams"),
            before: before_upstreams,
            after: after_upstreams,
        },
    ]
}

/// Apply a [`RoutePatch`] to the route identified by `id` in `new_state`.
fn apply_route_patch(
    state: &DesiredState,
    new_state: &mut DesiredState,
    id: &crate::model::identifiers::RouteId,
    route_patch: &RoutePatch,
) -> Result<Vec<DiffChange>, MutationError> {
    let pointer = JsonPointer::root().push("routes").push(id.as_str());
    let before = state.routes.get(id).and_then(to_json);

    let route = new_state
        .routes
        .get_mut(id)
        .ok_or_else(|| missing_route_error(id.as_str()))?;

    if let Some(hostnames) = route_patch.hostnames.clone() {
        route.hostnames = hostnames;
    }
    if let Some(upstreams) = route_patch.upstreams.clone() {
        route.upstreams = upstreams;
    }
    if let Some(matchers) = route_patch.matchers.clone() {
        route.matchers = matchers;
    }
    if let Some(headers) = route_patch.headers.clone() {
        route.headers = headers;
    }
    if let Some(redirects) = route_patch.redirects.clone() {
        route.redirects = redirects;
    }
    if let Some(policy_attachment) = route_patch.policy_attachment.clone() {
        route.policy_attachment = policy_attachment;
    }
    if let Some(enabled) = route_patch.enabled {
        route.enabled = enabled;
    }

    let after = to_json(route);
    Ok(vec![DiffChange {
        path: pointer,
        before,
        after,
    }])
}

/// Apply an [`UpstreamPatch`] to the upstream identified by `id` in `new_state`.
fn apply_upstream_patch(
    state: &DesiredState,
    new_state: &mut DesiredState,
    id: &crate::model::identifiers::UpstreamId,
    upstream_patch: &UpstreamPatch,
) -> Result<Vec<DiffChange>, MutationError> {
    let pointer = JsonPointer::root().push("upstreams").push(id.as_str());
    let before = state.upstreams.get(id).and_then(to_json);

    let upstream = new_state
        .upstreams
        .get_mut(id)
        .ok_or_else(|| missing_upstream_error(id.as_str()))?;

    if let Some(destination) = upstream_patch.destination.clone() {
        upstream.destination = destination;
    }
    if let Some(probe) = upstream_patch.probe.clone() {
        upstream.probe = probe;
    }
    if let Some(weight) = upstream_patch.weight {
        upstream.weight = weight;
    }
    if let Some(max_request_bytes) = upstream_patch.max_request_bytes {
        upstream.max_request_bytes = max_request_bytes;
    }

    let after = to_json(upstream);
    Ok(vec![DiffChange {
        path: pointer,
        before,
        after,
    }])
}

/// Serialize `v` to a [`serde_json::Value`], discarding serialization errors.
///
/// Used throughout the diff-building helpers to snapshot state before/after.
/// Serialization of well-typed domain models should never fail in practice;
/// swallowing the error here matches the `Option<Value>` contract on `DiffChange`.
fn to_json<T: serde::Serialize>(v: &T) -> Option<serde_json::Value> {
    serde_json::to_value(v).ok()
}

/// Build a `MutationError::Validation` for a missing route.
fn missing_route_error(id: &str) -> MutationError {
    use crate::mutation::error::ValidationRule;
    MutationError::Validation {
        rule: ValidationRule::RouteMissing,
        path: JsonPointer::root().push("route_id"),
        hint: format!("route '{id}' does not exist"),
    }
}

/// Build a `MutationError::Validation` for a missing upstream.
fn missing_upstream_error(id: &str) -> MutationError {
    use crate::mutation::error::ValidationRule;
    MutationError::Validation {
        rule: ValidationRule::UpstreamReferenceMissing,
        path: JsonPointer::root().push("id"),
        hint: format!("upstream '{id}' does not exist"),
    }
}

/// Map a [`MutationKind`] to the corresponding [`AuditEvent`].
const fn audit_event_for(kind: MutationKind) -> AuditEvent {
    match kind {
        MutationKind::CreateRoute
        | MutationKind::UpdateRoute
        | MutationKind::DeleteRoute
        | MutationKind::CreateUpstream
        | MutationKind::UpdateUpstream
        | MutationKind::DeleteUpstream
        | MutationKind::SetGlobalConfig
        | MutationKind::SetTlsConfig => AuditEvent::MutationApplied,
        MutationKind::AttachPolicy => AuditEvent::PolicyPresetAttached,
        MutationKind::DetachPolicy => AuditEvent::PolicyPresetDetached,
        MutationKind::UpgradePolicy => AuditEvent::PolicyPresetUpgraded,
        MutationKind::ImportFromCaddyfile => AuditEvent::ImportCaddyfile,
        MutationKind::Rollback => AuditEvent::ConfigRolledBack,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::audit::AuditEvent;
    use crate::caddy::capabilities::CapabilitySet;
    use crate::model::{
        desired_state::DesiredState,
        header::HeaderRules,
        identifiers::{PresetId, RouteId, UpstreamId},
        matcher::MatcherSet,
        policy::PresetVersion,
        route::Route,
        upstream::{Upstream, UpstreamDestination, UpstreamProbe},
    };
    use crate::mutation::{
        error::{ForbiddenReason, MutationError},
        patches::RoutePatch,
        types::Mutation,
    };

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn full_caps() -> CapabilitySet {
        CapabilitySet {
            loaded_modules: BTreeSet::from([
                "http.handlers.reverse_proxy".to_owned(),
                "http.handlers.rewrite".to_owned(),
                "http.handlers.headers".to_owned(),
                "http.handlers.static_response".to_owned(),
                "http.health_checks.active".to_owned(),
                "tls".to_owned(),
            ]),
            caddy_version: "v2.8.4".to_owned(),
            probed_at: 0,
        }
    }

    fn empty_caps() -> CapabilitySet {
        CapabilitySet {
            loaded_modules: BTreeSet::new(),
            caddy_version: "v2.8.4".to_owned(),
            probed_at: 0,
        }
    }

    fn minimal_upstream(id: &str) -> Upstream {
        Upstream {
            id: UpstreamId(id.to_owned()),
            destination: UpstreamDestination::TcpAddr {
                host: "127.0.0.1".to_owned(),
                port: 8080,
            },
            probe: UpstreamProbe::Disabled,
            weight: 100,
            max_request_bytes: None,
        }
    }

    fn minimal_route(id: &str) -> Route {
        Route {
            id: RouteId(id.to_owned()),
            hostnames: vec![],
            upstreams: vec![],
            matchers: MatcherSet::default(),
            headers: HeaderRules::default(),
            redirects: None,
            policy_attachment: None,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn state_with_route(route: Route) -> DesiredState {
        let mut state = DesiredState::empty();
        state.routes.insert(route.id.clone(), route);
        state
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn create_route_succeeds() {
        let state = DesiredState::empty();
        let route = minimal_route("route-1");
        let mutation = Mutation::CreateRoute {
            expected_version: 0,
            route: route.clone(),
        };
        let outcome = apply_mutation(&state, &mutation, &empty_caps()).unwrap();
        assert!(outcome.new_state.routes.contains_key(&route.id));
        assert_eq!(outcome.kind, AuditEvent::MutationApplied);
    }

    #[test]
    fn create_route_with_unknown_upstream_rejected() {
        let state = DesiredState::empty();
        let mut route = minimal_route("route-2");
        route.upstreams = vec![UpstreamId("no-such-upstream".to_owned())];
        let mutation = Mutation::CreateRoute {
            expected_version: 0,
            route,
        };
        let err = apply_mutation(&state, &mutation, &full_caps()).unwrap_err();
        assert!(matches!(
            err,
            MutationError::Validation {
                rule: crate::mutation::error::ValidationRule::UpstreamReferenceMissing,
                ..
            }
        ));
    }

    #[test]
    fn update_route_partial_patch_applies() {
        let route = minimal_route("route-3");
        let mut state = state_with_route(route.clone());
        // Also insert an upstream that the patch will reference.
        let upstream = minimal_upstream("u1");
        state.upstreams.insert(upstream.id.clone(), upstream);

        let patch = RoutePatch {
            upstreams: Some(vec![UpstreamId("u1".to_owned())]),
            enabled: Some(false),
            ..RoutePatch::default()
        };
        let mutation = Mutation::UpdateRoute {
            expected_version: 0,
            id: route.id.clone(),
            patch,
        };
        let outcome = apply_mutation(&state, &mutation, &full_caps()).unwrap();
        let updated = outcome.new_state.routes.get(&route.id).unwrap();
        assert_eq!(updated.upstreams, vec![UpstreamId("u1".to_owned())]);
        assert!(!updated.enabled);
        // Unchanged field must be preserved.
        assert!(updated.hostnames.is_empty());
    }

    #[test]
    fn delete_route_idempotent_when_present_and_rejected_when_absent() {
        let route = minimal_route("route-4");
        let state = state_with_route(route.clone());

        // Delete when present — must succeed.
        let mutation = Mutation::DeleteRoute {
            expected_version: 0,
            id: route.id.clone(),
        };
        let outcome = apply_mutation(&state, &mutation, &empty_caps()).unwrap();
        assert!(!outcome.new_state.routes.contains_key(&route.id));

        // Delete from state where route is absent — must return Validation error.
        let empty = DesiredState::empty();
        let mutation2 = Mutation::DeleteRoute {
            expected_version: 0,
            id: route.id,
        };
        let err = apply_mutation(&empty, &mutation2, &empty_caps()).unwrap_err();
        assert!(matches!(err, MutationError::Validation { .. }));
    }

    #[test]
    fn version_mismatch_returns_conflict() {
        let state = DesiredState::empty(); // version = 0
        let mutation = Mutation::CreateRoute {
            expected_version: 99, // wrong
            route: minimal_route("route-5"),
        };
        let err = apply_mutation(&state, &mutation, &empty_caps()).unwrap_err();
        assert_eq!(
            err,
            MutationError::Conflict {
                observed_version: 0,
                expected_version: 99,
            }
        );
    }

    #[test]
    fn capability_missing_returns_capability_missing() {
        let state = DesiredState::empty();
        // CreateUpstream with Disabled probe requires http.handlers.reverse_proxy.
        let mutation = Mutation::CreateUpstream {
            expected_version: 0,
            upstream: minimal_upstream("u2"),
        };
        let err = apply_mutation(&state, &mutation, &empty_caps()).unwrap_err();
        assert!(matches!(err, MutationError::CapabilityMissing { .. }));
    }

    #[test]
    fn policy_downgrade_forbidden() {
        let route_id = RouteId("route-6".to_owned());
        let preset_id = PresetId("preset-1".to_owned());

        let mut route = minimal_route("route-6");
        route.policy_attachment = Some(RoutePolicyAttachment {
            preset_id: preset_id.clone(),
            preset_version: 5,
        });

        let mut state = DesiredState::empty();
        state.routes.insert(route_id.clone(), route);
        // Preset must exist at the requested target version for the downgrade
        // check to be reached (version 3 and 5 both register as available).
        state.presets.insert(
            preset_id.clone(),
            PresetVersion {
                preset_id: preset_id.clone(),
                version: 3,
                body_json: "{}".to_owned(),
            },
        );

        // Try downgrading from 5 to 3.
        let mutation = Mutation::UpgradePolicy {
            expected_version: 0,
            route_id: route_id.clone(),
            to_version: 3,
        };
        let err = apply_mutation(&state, &mutation, &empty_caps()).unwrap_err();
        assert_eq!(
            err,
            MutationError::Forbidden {
                reason: ForbiddenReason::PolicyDowngrade,
            }
        );

        // Same version (no increase) must also be rejected.
        // Update preset to version 5 to match the "same version" case.
        state.presets.insert(
            preset_id.clone(),
            PresetVersion {
                preset_id,
                version: 5,
                body_json: "{}".to_owned(),
            },
        );
        let mutation_same = Mutation::UpgradePolicy {
            expected_version: 0,
            route_id,
            to_version: 5,
        };
        let err2 = apply_mutation(&state, &mutation_same, &empty_caps()).unwrap_err();
        assert_eq!(
            err2,
            MutationError::Forbidden {
                reason: ForbiddenReason::PolicyDowngrade,
            }
        );
    }

    #[test]
    fn policy_upgrade_strictly_increases_version() {
        let route_id = RouteId("route-7".to_owned());
        let preset_id = PresetId("preset-1".to_owned());

        let mut route = minimal_route("route-7");
        route.policy_attachment = Some(RoutePolicyAttachment {
            preset_id: preset_id.clone(),
            preset_version: 2,
        });

        let mut state = DesiredState::empty();
        state.routes.insert(route_id.clone(), route);
        // Preset must exist at version 3 for the upgrade to succeed.
        state.presets.insert(
            preset_id.clone(),
            PresetVersion {
                preset_id,
                version: 3,
                body_json: "{}".to_owned(),
            },
        );

        let mutation = Mutation::UpgradePolicy {
            expected_version: 0,
            route_id: route_id.clone(),
            to_version: 3,
        };
        let outcome = apply_mutation(&state, &mutation, &empty_caps()).unwrap();
        let updated = outcome.new_state.routes.get(&route_id).unwrap();
        assert_eq!(
            updated.policy_attachment.as_ref().unwrap().preset_version,
            3
        );
        assert_eq!(outcome.kind, AuditEvent::PolicyPresetUpgraded);
    }

    #[test]
    fn rollback_resolves_via_supplied_resolver() {
        // Phase 7 wires the snapshot resolver; for Phase 4, assert Forbidden { reason: RollbackTargetUnknown }.
        use crate::storage::types::SnapshotId;
        let state = DesiredState::empty();
        let mutation = Mutation::Rollback {
            expected_version: 0,
            target: SnapshotId("snap-abc".to_owned()),
        };
        let err = apply_mutation(&state, &mutation, &empty_caps()).unwrap_err();
        assert_eq!(
            err,
            MutationError::Forbidden {
                reason: ForbiddenReason::RollbackTargetUnknown,
            }
        );
    }

    #[test]
    fn version_increments_by_one_on_success() {
        let mut state = DesiredState::empty();
        state.version = 7;
        let mutation = Mutation::CreateRoute {
            expected_version: 7,
            route: minimal_route("route-8"),
        };
        let outcome = apply_mutation(&state, &mutation, &empty_caps()).unwrap();
        assert_eq!(outcome.new_state.version, 8);
    }

    #[test]
    fn create_upstream_succeeds() {
        let state = DesiredState::empty();
        let upstream = minimal_upstream("u3");
        let mutation = Mutation::CreateUpstream {
            expected_version: 0,
            upstream: upstream.clone(),
        };
        let outcome = apply_mutation(&state, &mutation, &full_caps()).unwrap();
        assert!(outcome.new_state.upstreams.contains_key(&upstream.id));
        assert_eq!(outcome.kind, AuditEvent::MutationApplied);
    }

    #[test]
    fn preset_exists_for_attach_policy() {
        let route_id = RouteId("route-9".to_owned());
        let preset_id = PresetId("preset-2".to_owned());

        let mut state = DesiredState::empty();
        state
            .routes
            .insert(route_id.clone(), minimal_route("route-9"));
        state.presets.insert(
            preset_id.clone(),
            PresetVersion {
                preset_id: preset_id.clone(),
                version: 1,
                body_json: "{}".to_owned(),
            },
        );

        let mutation = Mutation::AttachPolicy {
            expected_version: 0,
            route_id: route_id.clone(),
            preset_id: preset_id.clone(),
            preset_version: 1,
        };
        let outcome = apply_mutation(&state, &mutation, &empty_caps()).unwrap();
        let route = outcome.new_state.routes.get(&route_id).unwrap();
        assert_eq!(
            route.policy_attachment.as_ref().unwrap().preset_id,
            preset_id
        );
        assert_eq!(outcome.kind, AuditEvent::PolicyPresetAttached);
    }
}
