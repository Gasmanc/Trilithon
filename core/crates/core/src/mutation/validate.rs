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
            check_matchers_valid(&route.matchers)?;
            check_route_has_destination(&route.upstreams, route.redirects.as_ref())?;
            if let Some(redirect) = &route.redirects {
                check_redirect_url(&redirect.to)?;
                check_redirect_status(redirect.status)?;
            }
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

        Mutation::ImportFromCaddyfile { parsed, .. } => check_import_caddyfile(state, parsed),

        Mutation::SetGlobalConfig { patch, .. } => {
            if patch.is_noop() {
                return Err(MutationError::Validation {
                    rule: ValidationRule::NoOpMutation,
                    path: JsonPointer::root().push("patch"),
                    hint: "all patch fields are None — mutation would make no change".to_owned(),
                });
            }
            Ok(())
        }

        Mutation::SetTlsConfig { patch, .. } => {
            if let Some(Some(ask_url)) = &patch.on_demand_ask_url {
                check_on_demand_ask_url(ask_url)?;
            }
            Ok(())
        }
    }
}

/// Pre-conditions for [`Mutation::ImportFromCaddyfile`].
///
/// Validates every route and upstream in the parsed payload with the same
/// checks applied by `CreateRoute`/`CreateUpstream`, preventing crafted payloads
/// from smuggling invalid hostnames, duplicate IDs, or dangling upstream
/// references into `DesiredState`.
fn check_import_caddyfile(
    state: &DesiredState,
    parsed: &crate::mutation::patches::ParsedCaddyfile,
) -> Result<(), MutationError> {
    // Track IDs introduced within this import so intra-import duplicates are caught.
    let mut seen_route_ids = std::collections::BTreeSet::new();
    let mut seen_upstream_ids = std::collections::BTreeSet::new();

    for upstream in &parsed.upstreams {
        if state.upstreams.contains_key(&upstream.id) || !seen_upstream_ids.insert(&upstream.id) {
            return Err(MutationError::Validation {
                rule: ValidationRule::DuplicateUpstreamId,
                path: JsonPointer::root().push("parsed").push("upstreams"),
                hint: format!("upstream '{}' already exists", upstream.id.as_str()),
            });
        }
    }

    for route in &parsed.routes {
        if state.routes.contains_key(&route.id) || !seen_route_ids.insert(&route.id) {
            return Err(MutationError::Validation {
                rule: ValidationRule::DuplicateRouteId,
                path: JsonPointer::root().push("parsed").push("routes"),
                hint: format!("route '{}' already exists", route.id.as_str()),
            });
        }
        check_hostnames_valid(&route.hostnames)?;
        check_matchers_valid(&route.matchers)?;
        check_route_has_destination(&route.upstreams, route.redirects.as_ref())?;
        if let Some(redirect) = &route.redirects {
            check_redirect_url(&redirect.to)?;
            check_redirect_status(redirect.status)?;
        }
        // Check upstreams against both state and those introduced in this import.
        for uid in &route.upstreams {
            if !state.upstreams.contains_key(uid) && !seen_upstream_ids.contains(uid) {
                return Err(MutationError::Validation {
                    rule: ValidationRule::UpstreamReferenceMissing,
                    path: JsonPointer::root()
                        .push("parsed")
                        .push("routes")
                        .push("upstreams"),
                    hint: format!("upstream '{}' does not exist", uid.as_str()),
                });
            }
        }
    }
    Ok(())
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
        if let Err(e) = validate_hostname(raw) {
            return Err(MutationError::Validation {
                rule: ValidationRule::HostnameInvalid,
                path: JsonPointer::root()
                    .push("route")
                    .push("hostnames")
                    .push(&i.to_string()),
                hint: format!("hostname '{raw}': {e}"),
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
    if let Some(matchers) = &patch.matchers {
        check_matchers_valid(matchers)?;
    }
    if let Some(Some(redirect)) = &patch.redirects {
        check_redirect_url(&redirect.to)?;
        check_redirect_status(redirect.status)?;
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
    if preset_version == 0 {
        return Err(MutationError::Validation {
            rule: ValidationRule::PolicyPresetVersionZero,
            path: JsonPointer::root().push("preset_version"),
            hint: "preset versions start at 1; version 0 is reserved".to_owned(),
        });
    }
    check_route_exists(state, route_id)?;
    match state.presets.get(preset_id) {
        None => Err(MutationError::Validation {
            rule: ValidationRule::PolicyPresetMissing,
            path: JsonPointer::root().push("preset_id"),
            hint: "policy preset does not exist".to_owned(),
        }),
        Some(pv) if pv.version != preset_version => Err(MutationError::Validation {
            rule: ValidationRule::PolicyPresetVersionMismatch,
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
    // Single lookup: the missing-route guard and the attachment check share one get.
    let Some(route) = state.routes.get(route_id) else {
        return Err(MutationError::Validation {
            rule: ValidationRule::RouteMissing,
            path: JsonPointer::root().push("id"),
            hint: "route does not exist".to_owned(),
        });
    };
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
    // Single lookup: eliminates the redundant get after check_route_exists.
    let Some(route) = state.routes.get(route_id) else {
        return Err(MutationError::Validation {
            rule: ValidationRule::RouteMissing,
            path: JsonPointer::root().push("id"),
            hint: "route does not exist".to_owned(),
        });
    };
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
                rule: ValidationRule::PolicyPresetVersionMismatch,
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
                rule: ValidationRule::UpstreamStillReferenced,
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

/// Validate a redirect target URL — only http/https schemes are accepted.
fn check_redirect_url(url: &str) -> Result<(), MutationError> {
    match url::Url::parse(url) {
        Ok(parsed) if parsed.scheme() == "http" || parsed.scheme() == "https" => Ok(()),
        Ok(parsed) => Err(MutationError::Validation {
            rule: ValidationRule::RedirectUrlInvalid,
            path: JsonPointer::root().push("redirects").push("to"),
            hint: format!(
                "redirect URL scheme '{}' is not allowed; only http and https are accepted",
                parsed.scheme()
            ),
        }),
        Err(_) => Err(MutationError::Validation {
            rule: ValidationRule::RedirectUrlInvalid,
            path: JsonPointer::root().push("redirects").push("to"),
            hint: format!("redirect URL '{url}' is not a valid URL"),
        }),
    }
}

/// Validate the on-demand TLS ask URL — must be https and must not target
/// loopback or RFC 1918 addresses (SSRF guard).
fn check_on_demand_ask_url(url: &str) -> Result<(), MutationError> {
    let parsed = url::Url::parse(url).map_err(|_| MutationError::Validation {
        rule: ValidationRule::OnDemandAskUrlInvalid,
        path: JsonPointer::root().push("on_demand_ask_url"),
        hint: format!("on-demand ask URL '{url}' is not a valid URL"),
    })?;

    if parsed.scheme() != "https" {
        return Err(MutationError::Validation {
            rule: ValidationRule::OnDemandAskUrlInvalid,
            path: JsonPointer::root().push("on_demand_ask_url"),
            hint: format!(
                "on-demand ask URL must use https; got '{}'",
                parsed.scheme()
            ),
        });
    }

    if let Some(host) = parsed.host_str() {
        if is_loopback_or_private(host) {
            return Err(MutationError::Validation {
                rule: ValidationRule::OnDemandAskUrlInvalid,
                path: JsonPointer::root().push("on_demand_ask_url"),
                hint: format!(
                    "on-demand ask URL must not target a loopback or private address; got '{host}'"
                ),
            });
        }
    }

    Ok(())
}

/// Returns `true` if `host` resolves to a loopback or RFC 1918 / RFC 4193
/// address — used to block SSRF in on-demand TLS ask URLs.
fn is_loopback_or_private(host: &str) -> bool {
    use std::net::IpAddr;

    // Try to parse directly as an IP first.
    if let Ok(ip) = host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => v4.is_loopback() || is_private_v4(v4),
            IpAddr::V6(v6) => v6.is_loopback() || is_private_v6(v6),
        };
    }

    // Known loopback hostnames.
    matches!(host, "localhost" | "ip6-localhost" | "ip6-loopback")
}

fn is_private_v4(ip: std::net::Ipv4Addr) -> bool {
    let octets = ip.octets();
    // 10.0.0.0/8
    octets[0] == 10
        // 172.16.0.0/12
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        // 192.168.0.0/16
        || (octets[0] == 192 && octets[1] == 168)
}

const fn is_private_v6(ip: std::net::Ipv6Addr) -> bool {
    // fc00::/7 — Unique Local Addresses
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

/// Validate that all CIDR matchers in a [`MatcherSet`] contain valid CIDR notation.
fn check_matchers_valid(matchers: &crate::model::matcher::MatcherSet) -> Result<(), MutationError> {
    for (i, cidr) in matchers.remote.iter().enumerate() {
        parse_cidr(&cidr.0).map_err(|e| MutationError::Validation {
            rule: ValidationRule::CidrInvalid,
            path: JsonPointer::root()
                .push("matchers")
                .push("remote")
                .push(&i.to_string()),
            hint: e,
        })?;
    }
    Ok(())
}

/// Parse and validate a CIDR string.  Returns `Err` with a human-readable
/// message when the string is not valid IPv4 or IPv6 CIDR notation.
fn parse_cidr(cidr: &str) -> Result<(), String> {
    let Some(slash) = cidr.rfind('/') else {
        return Err(format!(
            "'{cidr}' is missing a prefix length (expected 'address/prefix')"
        ));
    };
    let (addr_part, prefix_part) = cidr.split_at(slash);
    let prefix_str = &prefix_part[1..]; // strip the '/'

    let prefix: u8 = prefix_str
        .parse()
        .map_err(|_| format!("prefix length '{prefix_str}' is not a valid integer in '{cidr}'"))?;

    if addr_part.parse::<std::net::Ipv4Addr>().is_ok() {
        if prefix > 32 {
            return Err(format!(
                "IPv4 prefix length {prefix} exceeds 32 in '{cidr}'"
            ));
        }
        return Ok(());
    }

    if addr_part.parse::<std::net::Ipv6Addr>().is_ok() {
        if prefix > 128 {
            return Err(format!(
                "IPv6 prefix length {prefix} exceeds 128 in '{cidr}'"
            ));
        }
        return Ok(());
    }

    Err(format!(
        "'{addr_part}' is not a valid IPv4 or IPv6 address in '{cidr}'"
    ))
}

/// Reject routes that have no upstream and no redirect — a black-hole route
/// matches traffic but has no configured handler, producing an opaque Caddy error.
fn check_route_has_destination(
    upstreams: &[crate::model::identifiers::UpstreamId],
    redirects: Option<&crate::model::redirect::RedirectRule>,
) -> Result<(), MutationError> {
    if upstreams.is_empty() && redirects.is_none() {
        return Err(MutationError::Validation {
            rule: ValidationRule::NoOpMutation,
            path: JsonPointer::root(),
            hint: "route must have at least one upstream or a redirect rule".to_owned(),
        });
    }
    Ok(())
}

/// Valid HTTP redirect status codes.
const VALID_REDIRECT_STATUSES: &[u16] = &[300, 301, 302, 303, 307, 308];

/// Validate that a redirect status code is a recognised HTTP redirect code.
fn check_redirect_status(status: u16) -> Result<(), MutationError> {
    if !VALID_REDIRECT_STATUSES.contains(&status) {
        return Err(MutationError::Validation {
            rule: ValidationRule::RedirectUrlInvalid,
            path: JsonPointer::root().push("redirects").push("status"),
            hint: format!(
                "redirect status {status} is not a valid HTTP redirect code; \
                 accepted values are 300, 301, 302, 303, 307, 308"
            ),
        });
    }
    Ok(())
}
