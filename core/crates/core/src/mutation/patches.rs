//! Patch types for mutations.
//!
//! All patch types follow the `Option<Option<T>>` convention:
//! - `None` means "do not modify this field"
//! - `Some(None)` means "clear/delete this field"
//! - `Some(Some(value))` means "set to this value"

use serde::{Deserialize, Serialize};

use crate::model::{
    header::HeaderRules,
    identifiers::UpstreamId,
    matcher::MatcherSet,
    redirect::RedirectRule,
    route::{HostPattern, Route, RoutePolicyAttachment},
    upstream::{Upstream, UpstreamDestination, UpstreamProbe},
};

/// Patch to apply to a route.
///
/// All fields follow the `Option<Option<T>>` convention where:
/// - `None` = do not modify
/// - `Some(None)` = clear the field
/// - `Some(Some(value))` = set to value
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[allow(clippy::option_option)]
pub struct RoutePatch {
    /// New hostnames for the route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostnames: Option<Vec<HostPattern>>,

    /// New upstream IDs for the route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstreams: Option<Vec<UpstreamId>>,

    /// New matcher set for the route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matchers: Option<MatcherSet>,

    /// New header rules for the route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<HeaderRules>,

    /// New or cleared redirect rule. `Some(None)` clears any redirect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirects: Option<Option<RedirectRule>>,

    /// New or cleared policy attachment. `Some(None)` clears any attachment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_attachment: Option<Option<RoutePolicyAttachment>>,

    /// New enabled state for the route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Patch to apply to an upstream.
///
/// All fields follow the `Option<Option<T>>` convention where:
/// - `None` = do not modify
/// - `Some(None)` = clear the field
/// - `Some(Some(value))` = set to value
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[allow(clippy::option_option)]
pub struct UpstreamPatch {
    /// New destination for the upstream.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<UpstreamDestination>,

    /// New probe configuration for the upstream.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe: Option<UpstreamProbe>,

    /// New weight for the upstream.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<u16>,

    /// New or cleared max request bytes. `Some(None)` clears the limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_request_bytes: Option<Option<u64>>,
}

/// A parsed Caddyfile ready to be merged into desired state.
///
/// Phase 13 supplies the parsed-Caddyfile shape. For Phase 4 this is an
/// opaque carrier; the `apply_mutation` handler for `ImportFromCaddyfile`
/// reads `routes` and `upstreams` and merges them into `DesiredState`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct ParsedCaddyfile {
    /// Routes parsed from the Caddyfile.
    pub routes: Vec<Route>,

    /// Upstreams parsed from the Caddyfile.
    pub upstreams: Vec<Upstream>,

    /// Warnings generated during parsing.
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_patch_serde_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let patch = RoutePatch {
            hostnames: Some(vec![
                HostPattern::Exact("example.com".into()),
                HostPattern::Wildcard("*.example.com".into()),
            ]),
            upstreams: Some(vec![UpstreamId("upstream-1".into())]),
            matchers: Some(MatcherSet::default()),
            headers: Some(HeaderRules::default()),
            redirects: Some(Some(RedirectRule {
                to: "https://example.com".into(),
                status: 301,
            })),
            policy_attachment: None,
            enabled: Some(true),
        };

        let json = serde_json::to_string(&patch)?;
        let deserialized: RoutePatch = serde_json::from_str(&json)?;

        assert_eq!(patch, deserialized);
        Ok(())
    }

    #[test]
    fn route_patch_default_is_all_none() {
        let patch = RoutePatch::default();

        assert!(patch.hostnames.is_none());
        assert!(patch.upstreams.is_none());
        assert!(patch.matchers.is_none());
        assert!(patch.headers.is_none());
        assert!(patch.redirects.is_none());
        assert!(patch.policy_attachment.is_none());
        assert!(patch.enabled.is_none());
    }

    #[test]
    fn upstream_patch_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let patch = UpstreamPatch {
            destination: Some(UpstreamDestination::TcpAddr {
                host: "127.0.0.1".into(),
                port: 8080,
            }),
            probe: Some(UpstreamProbe::Http {
                path: "/healthz".into(),
                expected_status: 200,
            }),
            weight: Some(100),
            max_request_bytes: Some(Some(1024 * 1024)),
        };

        let json = serde_json::to_string(&patch)?;
        let deserialized: UpstreamPatch = serde_json::from_str(&json)?;

        assert_eq!(patch, deserialized);
        Ok(())
    }
}
