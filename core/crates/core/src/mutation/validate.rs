//! Schema and pre-condition validators for mutations.
//!
//! [`pre_conditions`] checks all variant-specific invariants against the current
//! [`DesiredState`] before any state mutation is applied.

use crate::model::desired_state::DesiredState;
use crate::model::identifiers::{PresetId, RouteId, UpstreamId};
use crate::model::primitive::JsonPointer;
use crate::model::route::{HostPattern, validate_hostname};
use crate::mutation::error::{ForbiddenReason, MutationError, ValidationRule};
use crate::mutation::patches::RoutePatch;
use crate::mutation::types::Mutation;

/// Check all pre-conditions for a mutation against the current desired state.
///
/// # Errors
///
/// Returns a [`MutationError::Validation`] or [`MutationError::Forbidden`] if
/// any pre-condition is violated.
pub fn pre_conditions(state: &DesiredState, mutation: &Mutation) -> Result<(), MutationError> {
    match mutation {
        Mutation::CreateRoute { route, .. } => {
            check_route_id_unused(state, &route.id)?;
            check_upstreams_exist(state, &route.upstreams)?;
            check_hostnames_valid(&route.hostnames)?;
            Ok(())
        }

        Mutation::UpdateRoute { id, patch, .. } => check_update_route(state, id, patch),

        Mutation::DeleteRoute { id, .. } => check_route_exists(state, id),

        Mutation::CreateUpstream { upstream, .. } => {
            if state.upstreams.contains_key(&upstream.id) {
                return Err(MutationError::Validation {
                    rule: ValidationRule::DuplicateUpstreamId,
                    path: JsonPointer::root().push("upstream").push("id"),
                    hint: "upstream id already exists".to_owned(),
                });
            }
            Ok(())
        }

        Mutation::UpdateUpstream { id, .. } => {
            if !state.upstreams.contains_key(id) {
                return Err(MutationError::Validation {
                    rule: ValidationRule::UpstreamReferenceMissing,
                    path: JsonPointer::root().push("id"),
                    hint: "upstream does not exist".to_owned(),
                });
            }
            Ok(())
        }

        Mutation::DeleteUpstream { id, .. } => check_delete_upstream(state, id),

        Mutation::AttachPolicy {
            route_id,
            preset_id,
            preset_version,
            ..
        } => check_attach_policy(state, route_id, preset_id, *preset_version),

        Mutation::DetachPolicy { route_id, .. } => check_detach_policy(state, route_id),

        Mutation::UpgradePolicy {
            route_id,
            to_version,
            ..
        } => check_upgrade_policy(state, route_id, *to_version),

        // Phase 7 wires the snapshot resolver; for Phase 4, rollback always returns RollbackTargetUnknown.
        Mutation::Rollback { .. } => Err(MutationError::Forbidden {
            reason: ForbiddenReason::RollbackTargetUnknown,
        }),

        // No pre-condition failures for these variants.
        Mutation::SetGlobalConfig { .. }
        | Mutation::SetTlsConfig { .. }
        | Mutation::ImportFromCaddyfile { .. } => Ok(()),
    }
}

/// Verify that the given route id is not already present in `state`.
fn check_route_id_unused(state: &DesiredState, id: &RouteId) -> Result<(), MutationError> {
    if state.routes.contains_key(id) {
        return Err(MutationError::Validation {
            rule: ValidationRule::DuplicateRouteId,
            path: JsonPointer::root().push("route").push("id"),
            hint: "route id already exists".to_owned(),
        });
    }
    Ok(())
}

/// Verify that the given route id exists in `state`.
fn check_route_exists(state: &DesiredState, id: &RouteId) -> Result<(), MutationError> {
    if !state.routes.contains_key(id) {
        return Err(MutationError::Validation {
            rule: ValidationRule::RouteMissing,
            path: JsonPointer::root().push("id"),
            hint: "route does not exist".to_owned(),
        });
    }
    Ok(())
}

/// Verify that every upstream id in `ids` exists in `state`.
fn check_upstreams_exist(state: &DesiredState, ids: &[UpstreamId]) -> Result<(), MutationError> {
    for uid in ids {
        if !state.upstreams.contains_key(uid) {
            return Err(MutationError::Validation {
                rule: ValidationRule::UpstreamReferenceMissing,
                path: JsonPointer::root().push("route").push("upstreams"),
                hint: format!("upstream '{}' does not exist", uid.as_str()),
            });
        }
    }
    Ok(())
}

/// Verify that every hostname pattern is RFC 1123-compliant.
fn check_hostnames_valid(hostnames: &[HostPattern]) -> Result<(), MutationError> {
    for (i, hp) in hostnames.iter().enumerate() {
        let raw = match hp {
            HostPattern::Exact(s) | HostPattern::Wildcard(s) => s.as_str(),
        };
        if validate_hostname(raw).is_err() {
            return Err(MutationError::Validation {
                rule: ValidationRule::HostnameInvalid,
                path: JsonPointer::root()
                    .push("route")
                    .push("hostnames")
                    .push(&i.to_string()),
                hint: format!("hostname '{raw}' is not a valid RFC 1123 hostname"),
            });
        }
    }
    Ok(())
}

/// Pre-conditions for [`Mutation::UpdateRoute`].
fn check_update_route(
    state: &DesiredState,
    id: &RouteId,
    patch: &RoutePatch,
) -> Result<(), MutationError> {
    check_route_exists(state, id)?;
    if let Some(upstreams) = &patch.upstreams {
        check_upstreams_exist(state, upstreams)?;
    }
    if let Some(hostnames) = &patch.hostnames {
        check_hostnames_valid(hostnames)?;
    }
    Ok(())
}

/// Pre-conditions for [`Mutation::AttachPolicy`].
fn check_attach_policy(
    state: &DesiredState,
    route_id: &RouteId,
    preset_id: &PresetId,
    preset_version: u32,
) -> Result<(), MutationError> {
    if !state.routes.contains_key(route_id) {
        return Err(MutationError::Validation {
            rule: ValidationRule::RouteMissing,
            path: JsonPointer::root().push("route_id"),
            hint: "route does not exist".to_owned(),
        });
    }
    match state.presets.get(preset_id) {
        None => Err(MutationError::Validation {
            rule: ValidationRule::PolicyPresetMissing,
            path: JsonPointer::root().push("preset_id"),
            hint: "policy preset does not exist".to_owned(),
        }),
        Some(pv) if pv.version != preset_version => Err(MutationError::Validation {
            rule: ValidationRule::PolicyPresetMissing,
            path: JsonPointer::root().push("preset_version"),
            hint: format!(
                "preset version {preset_version} not available; current version is {}",
                pv.version
            ),
        }),
        Some(_) => Ok(()),
    }
}

/// Pre-conditions for [`Mutation::DetachPolicy`].
fn check_detach_policy(state: &DesiredState, route_id: &RouteId) -> Result<(), MutationError> {
    let route = state
        .routes
        .get(route_id)
        .ok_or_else(|| MutationError::Validation {
            rule: ValidationRule::RouteMissing,
            path: JsonPointer::root().push("route_id"),
            hint: "route does not exist".to_owned(),
        })?;
    if route.policy_attachment.is_none() {
        return Err(MutationError::Validation {
            rule: ValidationRule::PolicyAttachmentMissing,
            path: JsonPointer::root().push("route_id"),
            hint: "route does not carry a policy attachment".to_owned(),
        });
    }
    Ok(())
}

/// Pre-conditions for [`Mutation::UpgradePolicy`].
fn check_upgrade_policy(
    state: &DesiredState,
    route_id: &RouteId,
    to_version: u32,
) -> Result<(), MutationError> {
    let route = state
        .routes
        .get(route_id)
        .ok_or_else(|| MutationError::Validation {
            rule: ValidationRule::RouteMissing,
            path: JsonPointer::root().push("route_id"),
            hint: "route does not exist".to_owned(),
        })?;
    let attachment = route
        .policy_attachment
        .as_ref()
        .ok_or_else(|| MutationError::Validation {
            rule: ValidationRule::PolicyAttachmentMissing,
            path: JsonPointer::root().push("route_id"),
            hint: "route does not carry a policy attachment".to_owned(),
        })?;
    // Verify the preset exists in state and the requested version is available.
    match state.presets.get(&attachment.preset_id) {
        None => {
            return Err(MutationError::Validation {
                rule: ValidationRule::PolicyPresetMissing,
                path: JsonPointer::root().push("preset_id"),
                hint: "policy preset does not exist".to_owned(),
            });
        }
        Some(pv) if pv.version != to_version => {
            return Err(MutationError::Validation {
                rule: ValidationRule::PolicyPresetMissing,
                path: JsonPointer::root().push("to_version"),
                hint: format!(
                    "preset version {to_version} not available; current version is {}",
                    pv.version
                ),
            });
        }
        Some(_) => {}
    }
    if to_version <= attachment.preset_version {
        return Err(MutationError::Forbidden {
            reason: ForbiddenReason::PolicyDowngrade,
        });
    }
    Ok(())
}

/// Pre-conditions for [`Mutation::DeleteUpstream`].
///
/// Rejects the deletion when any route still references the target upstream,
/// which would leave dangling upstream IDs in those routes.
fn check_delete_upstream(state: &DesiredState, id: &UpstreamId) -> Result<(), MutationError> {
    if !state.upstreams.contains_key(id) {
        return Err(MutationError::Validation {
            rule: ValidationRule::UpstreamReferenceMissing,
            path: JsonPointer::root().push("id"),
            hint: "upstream does not exist".to_owned(),
        });
    }
    // Reject if any route still references this upstream.
    for (route_id, route) in &state.routes {
        if route.upstreams.contains(id) {
            return Err(MutationError::Validation {
                rule: ValidationRule::UpstreamReferenceMissing,
                path: JsonPointer::root().push("id"),
                hint: format!(
                    "upstream '{}' is still referenced by route '{}'",
                    id.as_str(),
                    route_id.as_str()
                ),
            });
        }
    }
    Ok(())
}
