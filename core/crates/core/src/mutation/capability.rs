//! Capability-gating algorithm for mutations.
//!
//! Each [`Mutation`] variant declares which Caddy modules it needs via
//! [`Mutation::referenced_caddy_modules`]. [`check_capabilities`] compares
//! that set against the [`CapabilitySet`] probed from the running instance and
//! returns [`MutationError::CapabilityMissing`] for the lexicographically first
//! absent module.

use std::collections::BTreeSet;

use crate::caddy::capabilities::CapabilitySet;
use crate::model::primitive::CaddyModule;
use crate::mutation::{error::MutationError, types::Mutation};

impl Mutation {
    /// Returns the set of Caddy module identifiers that this mutation requires
    /// at execution time.
    ///
    /// The result is a [`BTreeSet`] so the ordering is deterministic across
    /// invocations, enabling stable error messages.
    #[must_use]
    pub fn referenced_caddy_modules(&self) -> BTreeSet<CaddyModule> {
        match self {
            // ----------------------------------------------------------------
            // CreateRoute / UpdateRoute — modules depend on the route content.
            // ----------------------------------------------------------------
            Self::CreateRoute { route, .. } => {
                let mut mods = BTreeSet::new();
                if !route.upstreams.is_empty() {
                    mods.insert(CaddyModule::new("http.handlers.reverse_proxy"));
                }
                if !route.headers.request.is_empty() {
                    mods.insert(CaddyModule::new("http.handlers.headers"));
                }
                if !route.headers.response.is_empty() {
                    mods.insert(CaddyModule::new("http.handlers.headers"));
                }
                if route.redirects.is_some() {
                    mods.insert(CaddyModule::new("http.handlers.static_response"));
                }
                mods
            }

            Self::UpdateRoute { patch, .. } => {
                let mut mods = BTreeSet::new();
                if let Some(upstreams) = &patch.upstreams {
                    if !upstreams.is_empty() {
                        mods.insert(CaddyModule::new("http.handlers.reverse_proxy"));
                    }
                }
                if let Some(headers) = &patch.headers {
                    if !headers.request.is_empty() {
                        mods.insert(CaddyModule::new("http.handlers.headers"));
                    }
                    if !headers.response.is_empty() {
                        mods.insert(CaddyModule::new("http.handlers.headers"));
                    }
                }
                if let Some(Some(_)) = &patch.redirects {
                    mods.insert(CaddyModule::new("http.handlers.static_response"));
                }
                mods
            }

            // ----------------------------------------------------------------
            // Destructive / read-only mutations — no modules required.
            // ----------------------------------------------------------------
            Self::DeleteRoute { .. } | Self::DeleteUpstream { .. } | Self::Rollback { .. } => {
                BTreeSet::new()
            }

            // ----------------------------------------------------------------
            // CreateUpstream / UpdateUpstream — reverse_proxy always; active
            // health checks add http.health_checks.active.
            // ----------------------------------------------------------------
            Self::CreateUpstream { upstream, .. } => {
                let mut mods = BTreeSet::new();
                mods.insert(CaddyModule::new("http.handlers.reverse_proxy"));
                if !matches!(
                    upstream.probe,
                    crate::model::upstream::UpstreamProbe::Disabled
                ) {
                    mods.insert(CaddyModule::new("http.health_checks.active"));
                }
                mods
            }

            Self::UpdateUpstream { patch, .. } => {
                let mut mods = BTreeSet::new();
                mods.insert(CaddyModule::new("http.handlers.reverse_proxy"));
                if let Some(probe) = &patch.probe {
                    if !matches!(probe, crate::model::upstream::UpstreamProbe::Disabled) {
                        mods.insert(CaddyModule::new("http.health_checks.active"));
                    }
                }
                mods
            }

            // ----------------------------------------------------------------
            // Policy variants — preset module derivation deferred to phase 18.
            // ----------------------------------------------------------------
            Self::AttachPolicy { .. } | Self::UpgradePolicy { .. } => {
                // zd:CAP-PRESET expires:2026-12-31 reason:phase 18 wires preset module derivation
                BTreeSet::new()
            }

            // ----------------------------------------------------------------
            // Config mutations — TLS module only when ACME email is set.
            // ----------------------------------------------------------------
            Self::DetachPolicy { .. } | Self::SetGlobalConfig { .. } => BTreeSet::new(),

            Self::SetTlsConfig { patch, .. } => {
                let mut mods = BTreeSet::new();
                // Require the tls module whenever any TLS field is being set or cleared,
                // not just when an ACME email is provided.
                let any_set = patch.email.is_some()
                    || patch.on_demand_enabled.is_some()
                    || patch.on_demand_ask_url.is_some()
                    || patch.default_issuer.is_some();
                if any_set {
                    mods.insert(CaddyModule::new("tls"));
                }
                mods
            }

            // ----------------------------------------------------------------
            // ImportFromCaddyfile — union over all synthesised mutations.
            // ----------------------------------------------------------------
            Self::ImportFromCaddyfile { parsed, .. } => {
                let mut mods = BTreeSet::new();

                for route in &parsed.routes {
                    let synthetic = Self::CreateRoute {
                        expected_version: 0,
                        route: route.clone(),
                    };
                    mods.extend(synthetic.referenced_caddy_modules());
                }

                for upstream in &parsed.upstreams {
                    let synthetic = Self::CreateUpstream {
                        expected_version: 0,
                        upstream: upstream.clone(),
                    };
                    mods.extend(synthetic.referenced_caddy_modules());
                }

                mods
            }
        }
    }
}

/// Check that all Caddy modules referenced by `mutation` are present in
/// `capabilities`.
///
/// # Errors
///
/// Returns [`MutationError::CapabilityMissing`] for the lexicographically first
/// missing module when one or more required modules are absent.
pub fn check_capabilities(
    mutation: &Mutation,
    capabilities: &CapabilitySet,
) -> Result<(), MutationError> {
    let referenced_modules = mutation.referenced_caddy_modules();
    let loaded_modules = &capabilities.loaded_modules;

    // BTreeSet iteration order is deterministic (ascending lexicographic).
    let first_missing = referenced_modules
        .into_iter()
        .find(|m| !loaded_modules.contains(m.as_str()));

    // BTreeSet iteration order is deterministic; map_or avoids match_same_arms.
    first_missing.map_or(Ok(()), |module| {
        Err(MutationError::CapabilityMissing {
            module,
            required_by: mutation.kind(),
        })
    })
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
    use crate::caddy::capabilities::CapabilitySet;
    use crate::model::{
        header::HeaderRules,
        identifiers::{RouteId, UpstreamId},
        matcher::MatcherSet,
        redirect::RedirectRule,
        route::Route,
        tls::TlsConfigPatch,
        upstream::{Upstream, UpstreamDestination, UpstreamProbe},
    };
    use crate::mutation::error::MutationError;
    use crate::mutation::types::{Mutation, MutationKind};

    fn empty_caps() -> CapabilitySet {
        CapabilitySet {
            loaded_modules: BTreeSet::new(),
            caddy_version: "v2.8.4".to_owned(),
            probed_at: 0,
        }
    }

    fn caps_with(modules: &[&str]) -> CapabilitySet {
        CapabilitySet {
            loaded_modules: modules.iter().map(|s| (*s).to_owned()).collect(),
            caddy_version: "v2.8.4".to_owned(),
            probed_at: 0,
        }
    }

    fn minimal_route_with_upstream() -> Route {
        Route {
            id: RouteId::new(),
            hostnames: vec![],
            upstreams: vec![UpstreamId("u1".into())],
            matchers: MatcherSet::default(),
            headers: HeaderRules::default(),
            redirects: None,
            policy_attachment: None,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn minimal_route_no_upstream() -> Route {
        Route {
            id: RouteId::new(),
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

    fn redirect_only_route() -> Route {
        Route {
            id: RouteId::new(),
            hostnames: vec![],
            upstreams: vec![],
            matchers: MatcherSet::default(),
            headers: HeaderRules::default(),
            redirects: Some(RedirectRule {
                to: "https://example.com".into(),
                status: 301,
            }),
            policy_attachment: None,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn minimal_upstream() -> Upstream {
        Upstream {
            id: UpstreamId("u1".into()),
            destination: UpstreamDestination::TcpAddr {
                host: "127.0.0.1".into(),
                port: 8080,
            },
            probe: UpstreamProbe::Disabled,
            weight: 100,
            max_request_bytes: None,
        }
    }

    #[test]
    fn create_route_with_upstream_requires_reverse_proxy() {
        let mutation = Mutation::CreateRoute {
            expected_version: 0,
            route: minimal_route_with_upstream(),
        };
        let caps = empty_caps();
        let err = check_capabilities(&mutation, &caps).unwrap_err();
        assert_eq!(
            err,
            MutationError::CapabilityMissing {
                module: CaddyModule::new("http.handlers.reverse_proxy"),
                required_by: MutationKind::CreateRoute,
            }
        );
    }

    #[test]
    fn create_route_without_upstream_succeeds() {
        let mutation = Mutation::CreateRoute {
            expected_version: 0,
            route: minimal_route_no_upstream(),
        };
        let caps = empty_caps();
        assert_eq!(check_capabilities(&mutation, &caps), Ok(()));
    }

    #[test]
    fn redirect_only_route_requires_static_response() {
        let mutation = Mutation::CreateRoute {
            expected_version: 0,
            route: redirect_only_route(),
        };
        let caps = empty_caps();
        let err = check_capabilities(&mutation, &caps).unwrap_err();
        assert_eq!(
            err,
            MutationError::CapabilityMissing {
                module: CaddyModule::new("http.handlers.static_response"),
                required_by: MutationKind::CreateRoute,
            }
        );

        // Providing the module satisfies the check.
        let caps_with_static = caps_with(&["http.handlers.static_response"]);
        assert_eq!(check_capabilities(&mutation, &caps_with_static), Ok(()));
    }

    #[test]
    fn delete_route_requires_no_module() {
        let mutation = Mutation::DeleteRoute {
            expected_version: 0,
            id: RouteId::new(),
        };
        let caps = empty_caps();
        assert_eq!(check_capabilities(&mutation, &caps), Ok(()));
    }

    #[test]
    fn tls_email_requires_tls_module() {
        let mutation = Mutation::SetTlsConfig {
            expected_version: 0,
            patch: TlsConfigPatch {
                email: Some(Some("admin@example.com".to_owned())),
                ..Default::default()
            },
        };
        let caps = empty_caps();
        let err = check_capabilities(&mutation, &caps).unwrap_err();
        assert_eq!(
            err,
            MutationError::CapabilityMissing {
                module: CaddyModule::new("tls"),
                required_by: MutationKind::SetTlsConfig,
            }
        );

        let caps_tls = caps_with(&["tls"]);
        assert_eq!(check_capabilities(&mutation, &caps_tls), Ok(()));
    }

    #[test]
    fn referenced_modules_is_deterministic() {
        let mutation = Mutation::CreateUpstream {
            expected_version: 0,
            upstream: Upstream {
                probe: UpstreamProbe::Http {
                    path: "/health".into(),
                    expected_status: 200,
                },
                ..minimal_upstream()
            },
        };

        let first = mutation.referenced_caddy_modules();
        for _ in 0..99 {
            assert_eq!(
                mutation.referenced_caddy_modules(),
                first,
                "referenced_caddy_modules must be deterministic"
            );
        }
    }
}
