//! Capability re-check at apply time (Slice 7.3).
//!
//! [`check_against_capability_set`] validates that every Caddy module required
//! by the [`DesiredState`] is present in the live [`CapabilitySet`] before the
//! applier issues `POST /load`.  The function is pure: no I/O, no async.
//!
//! # Cross-references
//!
//! - ADR-0013 (capability probe gates optional Caddy features)
//! - PRD T1.1, T1.11
//! - Hazard H5

use crate::caddy::CapabilitySet;
use crate::model::desired_state::DesiredState;
use crate::model::route::Route;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Error returned when a required Caddy module is absent from the live
/// capability set at apply time.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CapabilityCheckError {
    /// A module required by a route is not loaded by the running Caddy.
    #[error("module {module} required by {site} is not loaded by the running Caddy")]
    Missing {
        /// The missing Caddy module identifier.
        module: String,
        /// JSON pointer of the route segment that requires the module.
        site: String,
    },
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Re-check that every Caddy module referenced by `state` is present in
/// `capabilities`.
///
/// This is the "at apply time" capability guard described in Hazard H5.  It
/// mirrors the mutation-time check from Phase 4 but operates over the full
/// [`DesiredState`] rather than a single mutation.
///
/// # Algorithm
///
/// For every enabled route in `state.routes`:
/// 1. Derive basic module requirements from the route's structure
///    (upstreams → `http.handlers.reverse_proxy`, redirects →
///    `http.handlers.static_response`, headers → `http.handlers.headers`).
/// 2. If the route carries a policy attachment, look up the preset body and
///    derive optional module requirements from top-level keys in the body JSON
///    (`rate_limit` → `http.handlers.rate_limit`, `forward_auth` →
///    `http.handlers.forward_auth`, `coraza` → `http.handlers.waf`).
/// 3. For every required module, verify `capabilities.has_module(&module)`.
///    The first missing module yields
///    [`CapabilityCheckError::Missing`] with `site` set to the JSON pointer
///    of the offending route.
///
/// # Errors
///
/// Returns [`CapabilityCheckError::Missing`] on the first missing module.
/// Iteration order is deterministic because `state.routes` is a [`BTreeMap`].
pub fn check_against_capability_set(
    state: &DesiredState,
    capabilities: &CapabilitySet,
) -> Result<(), CapabilityCheckError> {
    for (route_id, route) in &state.routes {
        if !route.enabled {
            continue;
        }
        let site = format!("/routes/{}", route_id.as_str());
        check_route(route, state, capabilities, &site)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Check all modules required by a single route.
fn check_route(
    route: &Route,
    state: &DesiredState,
    capabilities: &CapabilitySet,
    site: &str,
) -> Result<(), CapabilityCheckError> {
    // Structural module requirements.
    if !route.upstreams.is_empty() {
        require(capabilities, "http.handlers.reverse_proxy", site)?;
    }
    if !route.headers.request.is_empty() || !route.headers.response.is_empty() {
        require(capabilities, "http.handlers.headers", site)?;
    }
    if route.redirects.is_some() {
        require(capabilities, "http.handlers.static_response", site)?;
    }

    // Policy attachment — derive modules from preset body keys.
    if let Some(ref attachment) = route.policy_attachment {
        if let Some(preset) = state
            .presets
            .get(&attachment.preset_id)
            .filter(|p| p.version == attachment.preset_version)
        {
            for module in preset_body_modules(&preset.body_json) {
                require(capabilities, &module, site)?;
            }
        }
    }

    Ok(())
}

/// Return the Caddy module identifiers implied by top-level keys in `body_json`.
///
/// This is deliberately conservative: only keys that map to well-known optional
/// Caddy modules are translated.  Unknown keys are silently ignored so that
/// future preset fields do not break existing checks.
fn preset_body_modules(body_json: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body_json) else {
        return Vec::new();
    };
    let Some(obj) = value.as_object() else {
        return Vec::new();
    };

    let mut modules = Vec::new();
    for key in obj.keys() {
        match key.as_str() {
            "rate_limit" => modules.push("http.handlers.rate_limit".to_owned()),
            "forward_auth" => modules.push("http.handlers.forward_auth".to_owned()),
            "coraza" => modules.push("http.handlers.waf".to_owned()),
            _ => {}
        }
    }
    // Sort for deterministic error messages.
    modules.sort();
    modules
}

/// Assert that `module` is present in `capabilities`, returning
/// [`CapabilityCheckError::Missing`] if it is not.
fn require(
    capabilities: &CapabilitySet,
    module: &str,
    site: &str,
) -> Result<(), CapabilityCheckError> {
    if capabilities.loaded_modules.contains(module) {
        Ok(())
    } else {
        Err(CapabilityCheckError::Missing {
            module: module.to_owned(),
            site: site.to_owned(),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use super::*;
    use crate::caddy::capabilities::CapabilitySet;
    use crate::model::{
        desired_state::DesiredState,
        header::HeaderRules,
        identifiers::{PresetId, RouteId, UpstreamId},
        matcher::MatcherSet,
        policy::PresetVersion,
        route::{HostPattern, Route, RoutePolicyAttachment},
    };

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn caps_with(modules: &[&str]) -> CapabilitySet {
        CapabilitySet {
            loaded_modules: modules.iter().map(|s| (*s).to_owned()).collect(),
            caddy_version: "v2.8.4".to_owned(),
            probed_at: 0,
        }
    }

    /// The set of modules that ship with a stock (standard) Caddy build.
    fn stock_caps() -> CapabilitySet {
        caps_with(&[
            "http.handlers.reverse_proxy",
            "http.handlers.static_response",
            "http.handlers.headers",
            "http.handlers.file_server",
            "tls",
        ])
    }

    fn route_with_upstream(id: &str, upstream_id: &str) -> Route {
        Route {
            id: RouteId(id.to_owned()),
            hostnames: vec![HostPattern::Exact("example.com".to_owned())],
            upstreams: vec![UpstreamId(upstream_id.to_owned())],
            matchers: MatcherSet::default(),
            headers: HeaderRules::default(),
            redirects: None,
            policy_attachment: None,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn route_with_policy(id: &str, preset_id: &str, preset_version: u32) -> Route {
        Route {
            id: RouteId(id.to_owned()),
            hostnames: vec![HostPattern::Exact("example.com".to_owned())],
            upstreams: vec![],
            matchers: MatcherSet::default(),
            headers: HeaderRules::default(),
            redirects: None,
            policy_attachment: Some(RoutePolicyAttachment {
                preset_id: PresetId(preset_id.to_owned()),
                preset_version,
            }),
            enabled: true,
            created_at: 0,
            updated_at: 0,
        }
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    /// A state that attaches a `rate_limit` preset to a route passes when the
    /// capability set includes `http.handlers.rate_limit`.
    #[test]
    fn passes_with_full_capabilities() {
        let preset_id = PresetId("01PRESET0000000000000000A1".to_owned());

        let mut state = DesiredState::empty();
        state.routes.insert(
            RouteId("01ROUTE0000000000000000001".to_owned()),
            route_with_policy(
                "01ROUTE0000000000000000001",
                "01PRESET0000000000000000A1",
                1,
            ),
        );
        state.presets.insert(
            preset_id,
            PresetVersion {
                preset_id: PresetId("01PRESET0000000000000000A1".to_owned()),
                version: 1,
                body_json: r#"{"rate_limit":50}"#.to_owned(),
            },
        );

        let caps = caps_with(&["http.handlers.rate_limit"]);
        assert_eq!(check_against_capability_set(&state, &caps), Ok(()));
    }

    /// The same state fails when `http.handlers.rate_limit` is absent.
    #[test]
    fn fails_when_module_missing() {
        let preset_id = PresetId("01PRESET0000000000000000A1".to_owned());

        let mut state = DesiredState::empty();
        state.routes.insert(
            RouteId("01ROUTE0000000000000000001".to_owned()),
            route_with_policy(
                "01ROUTE0000000000000000001",
                "01PRESET0000000000000000A1",
                1,
            ),
        );
        state.presets.insert(
            preset_id,
            PresetVersion {
                preset_id: PresetId("01PRESET0000000000000000A1".to_owned()),
                version: 1,
                body_json: r#"{"rate_limit":50}"#.to_owned(),
            },
        );

        let caps = caps_with(&[]); // empty — rate_limit is absent
        let err = check_against_capability_set(&state, &caps).unwrap_err();
        assert_eq!(
            err,
            CapabilityCheckError::Missing {
                module: "http.handlers.rate_limit".to_owned(),
                site: "/routes/01ROUTE0000000000000000001".to_owned(),
            }
        );
    }

    /// A desired state with only reverse-proxy routes passes against the stock
    /// Caddy capability set.
    #[test]
    fn stock_caddy_admits_basic_route() {
        let mut state = DesiredState::empty();
        state.routes.insert(
            RouteId("01ROUTE0000000000000000001".to_owned()),
            route_with_upstream("01ROUTE0000000000000000001", "01UPSTREAM000000000000001A"),
        );

        assert_eq!(check_against_capability_set(&state, &stock_caps()), Ok(()));
    }

    /// Disabled routes are not checked.
    #[test]
    fn disabled_routes_are_skipped() {
        let mut state = DesiredState::empty();
        let mut route =
            route_with_upstream("01ROUTE0000000000000000001", "01UPSTREAM000000000000001A");
        route.enabled = false;

        state
            .routes
            .insert(RouteId("01ROUTE0000000000000000001".to_owned()), route);

        // Empty capability set — would fail if the route were checked.
        let caps = caps_with(&[]);
        assert_eq!(check_against_capability_set(&state, &caps), Ok(()));
    }

    /// A policy attachment whose preset is not found (version mismatch) does
    /// not contribute any module requirements — we cannot derive what modules
    /// are needed without the preset body.
    #[test]
    fn missing_preset_version_does_not_fail() {
        let mut state = DesiredState::empty();
        state.routes.insert(
            RouteId("01ROUTE0000000000000000001".to_owned()),
            route_with_policy(
                "01ROUTE0000000000000000001",
                "01PRESET0000000000000000A1",
                99,
            ),
        );
        // No preset entry in state.presets.

        let caps = caps_with(&[]);
        assert_eq!(check_against_capability_set(&state, &caps), Ok(()));
    }
}
