//! Audit event vocabulary.
//!
//! `AuditEvent` provides a closed enum of every audit event kind defined in
//! architecture §6.6, with a `Display` implementation that produces the
//! canonical wire string.

use std::fmt;

/// One-to-one Rust ↔ wire mapping per architecture §6.6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEvent {
    /// `mutation.proposed`
    MutationProposed,
    /// `mutation.submitted`
    MutationSubmitted,
    /// `mutation.applied`
    MutationApplied,
    /// `mutation.conflicted`
    MutationConflicted,
    /// `mutation.rejected`
    MutationRejected,
    /// `mutation.rejected.missing-expected-version`
    MutationRejectedMissingExpectedVersion,
    /// `mutation.rebased.auto`
    MutationRebasedAuto,
    /// `mutation.rebased.manual`
    MutationRebasedManual,
    /// `mutation.rebase.expired`
    MutationRebaseExpired,

    /// `config.applied`
    ApplySucceeded,
    /// `config.apply-failed`
    ApplyFailed,
    /// `config.drift-detected`
    DriftDetected,
    /// `config.drift-resolved`
    DriftResolved,
    /// `config.rolled-back`
    ConfigRolledBack,
    /// `config.rebased`
    ConfigRebased,

    /// `caddy.ownership-sentinel-conflict`
    OwnershipSentinelConflict,
    /// `caddy.reconnected`
    CaddyReconnected,
    /// `caddy.unreachable`
    CaddyUnreachable,
    /// `caddy.capability-probe-completed`
    CaddyCapabilityProbeCompleted,

    /// `policy-preset.attached`
    PolicyPresetAttached,
    /// `policy-preset.detached`
    PolicyPresetDetached,
    /// `policy-preset.upgraded`
    PolicyPresetUpgraded,
    /// `policy.registry-mismatch`
    PolicyRegistryMismatch,

    /// `secrets.revealed`
    SecretsRevealed,
    /// `secrets.master-key-rotated`
    SecretsMasterKeyRotated,

    /// `import.caddyfile`
    ImportCaddyfile,
    /// `export.bundle`
    ExportBundle,
    /// `export.caddy-json`
    ExportCaddyJson,
    /// `export.caddyfile`
    ExportCaddyfile,

    /// `tool-gateway.session-opened`
    ToolGatewaySessionOpened,
    /// `tool-gateway.session-closed`
    ToolGatewaySessionClosed,
    /// `tool-gateway.tool-invoked`
    ToolGatewayInvoked,

    /// `auth.login-succeeded`
    AuthLoginSucceeded,
    /// `auth.login-failed`
    AuthLoginFailed,
    /// `auth.logout`
    AuthLogout,
    /// `auth.session-revoked`
    AuthSessionRevoked,
    /// `auth.bootstrap-credentials-rotated`
    AuthBootstrapCredentialsRotated,

    /// `docker.socket-trust-grant`
    DockerSocketTrustGrant,

    /// `proposal.approved`
    ProposalApproved,
    /// `proposal.rejected`
    ProposalRejected,
    /// `proposal.expired`
    ProposalExpired,
}

impl fmt::Display for AuditEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::MutationProposed => "mutation.proposed",
            Self::MutationSubmitted => "mutation.submitted",
            Self::MutationApplied => "mutation.applied",
            Self::MutationConflicted => "mutation.conflicted",
            Self::MutationRejected => "mutation.rejected",
            Self::MutationRejectedMissingExpectedVersion => {
                "mutation.rejected.missing-expected-version"
            }
            Self::MutationRebasedAuto => "mutation.rebased.auto",
            Self::MutationRebasedManual => "mutation.rebased.manual",
            Self::MutationRebaseExpired => "mutation.rebase.expired",
            Self::ApplySucceeded => "config.applied",
            Self::ApplyFailed => "config.apply-failed",
            Self::DriftDetected => "config.drift-detected",
            Self::DriftResolved => "config.drift-resolved",
            Self::ConfigRolledBack => "config.rolled-back",
            Self::ConfigRebased => "config.rebased",
            Self::OwnershipSentinelConflict => "caddy.ownership-sentinel-conflict",
            Self::CaddyReconnected => "caddy.reconnected",
            Self::CaddyUnreachable => "caddy.unreachable",
            Self::CaddyCapabilityProbeCompleted => "caddy.capability-probe-completed",
            Self::PolicyPresetAttached => "policy-preset.attached",
            Self::PolicyPresetDetached => "policy-preset.detached",
            Self::PolicyPresetUpgraded => "policy-preset.upgraded",
            Self::PolicyRegistryMismatch => "policy.registry-mismatch",
            Self::SecretsRevealed => "secrets.revealed",
            Self::SecretsMasterKeyRotated => "secrets.master-key-rotated",
            Self::ImportCaddyfile => "import.caddyfile",
            Self::ExportBundle => "export.bundle",
            Self::ExportCaddyJson => "export.caddy-json",
            Self::ExportCaddyfile => "export.caddyfile",
            Self::ToolGatewaySessionOpened => "tool-gateway.session-opened",
            Self::ToolGatewaySessionClosed => "tool-gateway.session-closed",
            Self::ToolGatewayInvoked => "tool-gateway.tool-invoked",
            Self::AuthLoginSucceeded => "auth.login-succeeded",
            Self::AuthLoginFailed => "auth.login-failed",
            Self::AuthLogout => "auth.logout",
            Self::AuthSessionRevoked => "auth.session-revoked",
            Self::AuthBootstrapCredentialsRotated => "auth.bootstrap-credentials-rotated",
            Self::DockerSocketTrustGrant => "docker.socket-trust-grant",
            Self::ProposalApproved => "proposal.approved",
            Self::ProposalRejected => "proposal.rejected",
            Self::ProposalExpired => "proposal.expired",
        };
        f.write_str(s)
    }
}

/// Canonical audit event kind vocabulary (§6.6).
///
/// Every [`AuditEvent`] variant's `Display` output must appear in this list.
/// Phase 6's audit-row writer imports this constant to populate the `kind`
/// column without re-declaring the vocabulary.
pub const AUDIT_KIND_VOCAB: &[&str] = &[
    "auth.bootstrap-credentials-rotated",
    "auth.login-failed",
    "auth.login-succeeded",
    "auth.logout",
    "auth.session-revoked",
    "caddy.capability-probe-completed",
    "caddy.ownership-sentinel-conflict",
    "caddy.reconnected",
    "caddy.unreachable",
    "config.applied",
    "config.apply-failed",
    "config.drift-detected",
    "config.drift-resolved",
    "config.rebased",
    "config.rolled-back",
    "docker.socket-trust-grant",
    "export.bundle",
    "export.caddy-json",
    "export.caddyfile",
    "import.caddyfile",
    "mutation.applied",
    "mutation.conflicted",
    "mutation.proposed",
    "mutation.rebase.expired",
    "mutation.rebased.auto",
    "mutation.rebased.manual",
    "mutation.rejected",
    "mutation.rejected.missing-expected-version",
    "mutation.submitted",
    "policy-preset.attached",
    "policy-preset.detached",
    "policy-preset.upgraded",
    "policy.registry-mismatch",
    "proposal.approved",
    "proposal.expired",
    "proposal.rejected",
    "secrets.master-key-rotated",
    "secrets.revealed",
    "tool-gateway.session-closed",
    "tool-gateway.session-opened",
    "tool-gateway.tool-invoked",
];

/// Expected count of [`AuditEvent`] variants. Changing this constant requires
/// updating `AUDIT_KIND_VOCAB` and the test helper `all_variants()`.
#[cfg(test)]
const AUDIT_EVENT_VARIANT_COUNT: usize = 41;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn all_variants() -> Vec<AuditEvent> {
        use AuditEvent::*;
        vec![
            MutationProposed,
            MutationSubmitted,
            MutationApplied,
            MutationConflicted,
            MutationRejected,
            MutationRejectedMissingExpectedVersion,
            MutationRebasedAuto,
            MutationRebasedManual,
            MutationRebaseExpired,
            ApplySucceeded,
            ApplyFailed,
            DriftDetected,
            DriftResolved,
            ConfigRolledBack,
            ConfigRebased,
            OwnershipSentinelConflict,
            CaddyReconnected,
            CaddyUnreachable,
            CaddyCapabilityProbeCompleted,
            PolicyPresetAttached,
            PolicyPresetDetached,
            PolicyPresetUpgraded,
            PolicyRegistryMismatch,
            SecretsRevealed,
            SecretsMasterKeyRotated,
            ImportCaddyfile,
            ExportBundle,
            ExportCaddyJson,
            ExportCaddyfile,
            ToolGatewaySessionOpened,
            ToolGatewaySessionClosed,
            ToolGatewayInvoked,
            AuthLoginSucceeded,
            AuthLoginFailed,
            AuthLogout,
            AuthSessionRevoked,
            AuthBootstrapCredentialsRotated,
            DockerSocketTrustGrant,
            ProposalApproved,
            ProposalRejected,
            ProposalExpired,
        ]
    }

    #[test]
    fn variant_count_matches_expected() {
        assert_eq!(
            all_variants().len(),
            AUDIT_EVENT_VARIANT_COUNT,
            "AuditEvent variant count changed — update AUDIT_EVENT_VARIANT_COUNT and all_variants()"
        );
    }

    #[test]
    fn display_strings_match_six_six_vocab() {
        let vocab_set: HashSet<&str> = AUDIT_KIND_VOCAB.iter().copied().collect();
        for event in all_variants() {
            let kind = event.to_string();
            assert!(
                vocab_set.contains(kind.as_str()),
                "AuditEvent::{event:?} produced kind {kind:?} which is not in the §6.6 vocabulary"
            );
        }
    }

    #[test]
    fn no_two_variants_share_a_kind() {
        let variants = all_variants();
        let variant_count = variants.len();
        let unique: HashSet<String> = variants.into_iter().map(|e| e.to_string()).collect();
        assert_eq!(
            unique.len(),
            variant_count,
            "duplicate Display strings detected among AuditEvent variants"
        );
    }
}
