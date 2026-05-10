//! Core mutation variant types.

use serde::{Deserialize, Serialize};

use crate::model::{
    global::GlobalConfigPatch,
    identifiers::{PresetId, RouteId, UpstreamId},
    route::Route,
    tls::TlsConfigPatch,
    upstream::Upstream,
};
use crate::mutation::patches::{ParsedCaddyfile, RoutePatch, UpstreamPatch};
use crate::storage::types::SnapshotId;

/// Every possible desired-state mutation.
///
/// The `expected_version` field on each variant is the optimistic-concurrency
/// guard: the handler rejects the mutation if the current config version does
/// not match.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "PascalCase")]
pub enum Mutation {
    /// Create a new route.
    CreateRoute {
        /// Optimistic-concurrency guard.
        expected_version: i64,
        /// The route to create.
        route: Route,
    },
    /// Apply a partial update to an existing route.
    UpdateRoute {
        /// Optimistic-concurrency guard.
        expected_version: i64,
        /// Target route identifier.
        id: RouteId,
        /// Fields to update.
        patch: RoutePatch,
    },
    /// Remove a route.
    DeleteRoute {
        /// Optimistic-concurrency guard.
        expected_version: i64,
        /// Target route identifier.
        id: RouteId,
    },
    /// Create a new upstream.
    CreateUpstream {
        /// Optimistic-concurrency guard.
        expected_version: i64,
        /// The upstream to create.
        upstream: Upstream,
    },
    /// Apply a partial update to an existing upstream.
    UpdateUpstream {
        /// Optimistic-concurrency guard.
        expected_version: i64,
        /// Target upstream identifier.
        id: UpstreamId,
        /// Fields to update.
        patch: UpstreamPatch,
    },
    /// Remove an upstream.
    DeleteUpstream {
        /// Optimistic-concurrency guard.
        expected_version: i64,
        /// Target upstream identifier.
        id: UpstreamId,
    },
    /// Attach a policy preset to a route.
    AttachPolicy {
        /// Optimistic-concurrency guard.
        expected_version: i64,
        /// Target route identifier.
        route_id: RouteId,
        /// Policy preset to attach.
        preset_id: PresetId,
        /// Version of the preset to attach.
        preset_version: u32,
    },
    /// Remove any policy attachment from a route.
    DetachPolicy {
        /// Optimistic-concurrency guard.
        expected_version: i64,
        /// Target route identifier.
        route_id: RouteId,
    },
    /// Upgrade an attached policy to a newer preset version.
    UpgradePolicy {
        /// Optimistic-concurrency guard.
        expected_version: i64,
        /// Target route identifier.
        route_id: RouteId,
        /// Target preset version.
        to_version: u32,
    },
    /// Replace the global proxy configuration.
    SetGlobalConfig {
        /// Optimistic-concurrency guard.
        expected_version: i64,
        /// Fields to update.
        patch: GlobalConfigPatch,
    },
    /// Replace the global TLS configuration.
    SetTlsConfig {
        /// Optimistic-concurrency guard.
        expected_version: i64,
        /// Fields to update.
        patch: TlsConfigPatch,
    },
    /// Merge routes and upstreams parsed from a Caddyfile.
    ImportFromCaddyfile {
        /// Optimistic-concurrency guard.
        expected_version: i64,
        /// Pre-parsed Caddyfile contents.
        parsed: ParsedCaddyfile,
    },
    /// Roll back desired state to a previous snapshot.
    Rollback {
        /// Optimistic-concurrency guard.
        expected_version: i64,
        /// Target snapshot identifier.
        target: SnapshotId,
    },
    /// Replace desired state with the observed running state (drift adopt).
    #[cfg_attr(feature = "schema", schemars(skip))]
    ReplaceDesiredState {
        /// Optimistic-concurrency guard.
        expected_version: i64,
        /// The new desired state to store.
        new_state: Box<crate::model::desired_state::DesiredState>,
        /// Resolution provenance.
        source: crate::diff::resolve::ResolveSource,
    },
    /// Re-apply an existing snapshot to Caddy (drift reapply).
    #[cfg_attr(feature = "schema", schemars(skip))]
    ReapplySnapshot {
        /// Optimistic-concurrency guard.
        expected_version: i64,
        /// Snapshot to re-push through the applier.
        snapshot_id: SnapshotId,
        /// Resolution provenance.
        source: crate::diff::resolve::ResolveSource,
    },
    /// No-op marker recording that drift was deferred for manual reconciliation.
    #[cfg_attr(feature = "schema", schemars(skip))]
    DriftDeferred {
        /// Optimistic-concurrency guard.
        expected_version: i64,
        /// Correlation id of the originating drift event.
        event_correlation: ulid::Ulid,
    },
}

impl Mutation {
    /// Returns the optimistic-concurrency guard carried by this mutation.
    pub const fn expected_version(&self) -> i64 {
        match self {
            Self::CreateRoute {
                expected_version, ..
            }
            | Self::UpdateRoute {
                expected_version, ..
            }
            | Self::DeleteRoute {
                expected_version, ..
            }
            | Self::CreateUpstream {
                expected_version, ..
            }
            | Self::UpdateUpstream {
                expected_version, ..
            }
            | Self::DeleteUpstream {
                expected_version, ..
            }
            | Self::AttachPolicy {
                expected_version, ..
            }
            | Self::DetachPolicy {
                expected_version, ..
            }
            | Self::UpgradePolicy {
                expected_version, ..
            }
            | Self::SetGlobalConfig {
                expected_version, ..
            }
            | Self::SetTlsConfig {
                expected_version, ..
            }
            | Self::ImportFromCaddyfile {
                expected_version, ..
            }
            | Self::Rollback {
                expected_version, ..
            }
            | Self::ReplaceDesiredState {
                expected_version, ..
            }
            | Self::ReapplySnapshot {
                expected_version, ..
            }
            | Self::DriftDeferred {
                expected_version, ..
            } => *expected_version,
        }
    }

    /// Returns the discriminant kind for this mutation.
    pub const fn kind(&self) -> MutationKind {
        match self {
            Self::CreateRoute { .. } => MutationKind::CreateRoute,
            Self::UpdateRoute { .. } => MutationKind::UpdateRoute,
            Self::DeleteRoute { .. } => MutationKind::DeleteRoute,
            Self::CreateUpstream { .. } => MutationKind::CreateUpstream,
            Self::UpdateUpstream { .. } => MutationKind::UpdateUpstream,
            Self::DeleteUpstream { .. } => MutationKind::DeleteUpstream,
            Self::AttachPolicy { .. } => MutationKind::AttachPolicy,
            Self::DetachPolicy { .. } => MutationKind::DetachPolicy,
            Self::UpgradePolicy { .. } => MutationKind::UpgradePolicy,
            Self::SetGlobalConfig { .. } => MutationKind::SetGlobalConfig,
            Self::SetTlsConfig { .. } => MutationKind::SetTlsConfig,
            Self::ImportFromCaddyfile { .. } => MutationKind::ImportFromCaddyfile,
            Self::Rollback { .. } => MutationKind::Rollback,
            Self::ReplaceDesiredState { .. } => MutationKind::ReplaceDesiredState,
            Self::ReapplySnapshot { .. } => MutationKind::ReapplySnapshot,
            Self::DriftDeferred { .. } => MutationKind::DriftDeferred,
        }
    }
}

/// Discriminant for [`Mutation`] — carries no payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum MutationKind {
    /// See [`Mutation::CreateRoute`].
    CreateRoute,
    /// See [`Mutation::UpdateRoute`].
    UpdateRoute,
    /// See [`Mutation::DeleteRoute`].
    DeleteRoute,
    /// See [`Mutation::CreateUpstream`].
    CreateUpstream,
    /// See [`Mutation::UpdateUpstream`].
    UpdateUpstream,
    /// See [`Mutation::DeleteUpstream`].
    DeleteUpstream,
    /// See [`Mutation::AttachPolicy`].
    AttachPolicy,
    /// See [`Mutation::DetachPolicy`].
    DetachPolicy,
    /// See [`Mutation::UpgradePolicy`].
    UpgradePolicy,
    /// See [`Mutation::SetGlobalConfig`].
    SetGlobalConfig,
    /// See [`Mutation::SetTlsConfig`].
    SetTlsConfig,
    /// See [`Mutation::ImportFromCaddyfile`].
    ImportFromCaddyfile,
    /// See [`Mutation::Rollback`].
    Rollback,
    /// See [`Mutation::ReplaceDesiredState`].
    ReplaceDesiredState,
    /// See [`Mutation::ReapplySnapshot`].
    ReapplySnapshot,
    /// See [`Mutation::DriftDeferred`].
    DriftDeferred,
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
    use super::*;
    use crate::model::{
        header::HeaderRules, identifiers::RouteId, matcher::MatcherSet, route::Route,
    };

    fn minimal_route() -> Route {
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

    #[test]
    fn serde_tag_is_kind() {
        let m = Mutation::CreateRoute {
            expected_version: 7,
            route: minimal_route(),
        };
        let json = serde_json::to_string(&m).expect("serialise");
        assert!(
            json.contains(r#""kind":"CreateRoute""#),
            "expected kind tag in JSON, got: {json}"
        );
    }

    #[test]
    fn expected_version_accessor() {
        let m = Mutation::DeleteRoute {
            expected_version: 42,
            id: RouteId::new(),
        };
        assert_eq!(m.expected_version(), 42);
    }

    #[test]
    fn kind_accessor_matches_variant() {
        let m = Mutation::Rollback {
            expected_version: 1,
            target: SnapshotId("abc".into()),
        };
        assert_eq!(m.kind(), MutationKind::Rollback);
    }
}
