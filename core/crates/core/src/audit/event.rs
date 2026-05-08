//! Closed Tier 1 audit-event vocabulary (architecture §6.6).
//!
//! [`AuditEvent`] provides a closed enum of every audit event kind defined in
//! architecture §6.6, with a [`Display`](std::fmt::Display) implementation
//! that produces the canonical wire string and a [`FromStr`] implementation
//! that round-trips it.

use std::fmt;
use std::str::FromStr;

/// Closed Tier 1 audit-event vocabulary. Variants map one-to-one to wire
/// `kind` strings recorded in `audit_log.kind` (architecture §6.6).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum AuditEvent {
    // Authentication (T1.14)
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
    /// `auth.bootstrap-credentials-created`
    AuthBootstrapCredentialsCreated,

    // Caddy lifecycle (T1.11)
    /// `caddy.capability-probe-completed`
    CaddyCapabilityProbeCompleted,
    /// `caddy.ownership-sentinel-conflict`
    CaddyOwnershipSentinelConflict,
    /// `caddy.reconnected`
    CaddyReconnected,
    /// `caddy.unreachable`
    CaddyUnreachable,

    // Configuration apply (T1.1)
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
    /// `config.drift-deferred` — explicit operator deferral
    DriftDeferred,
    /// `config.drift-auto-deferred` — auto-deferred after 3 consecutive conflict retries
    DriftAutoDeferred,
    /// `config.rebased`
    ConfigRebased,

    // Mutation lifecycle (T1.6)
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

    // Secrets (T1.15)
    /// `secrets.revealed`
    SecretsRevealed,
    /// `secrets.master-key-rotated`
    SecretsMasterKeyRotated,

    // Policy presets
    /// `policy-preset.attached`
    PolicyPresetAttached,
    /// `policy-preset.detached`
    PolicyPresetDetached,
    /// `policy-preset.upgraded`
    PolicyPresetUpgraded,
    /// `policy.registry-mismatch`
    PolicyRegistryMismatch,

    // Import / export
    /// `import.caddyfile`
    ImportCaddyfile,
    /// `export.bundle`
    ExportBundle,
    /// `export.caddy-json`
    ExportCaddyJson,
    /// `export.caddyfile`
    ExportCaddyfile,

    // Tool gateway
    /// `tool-gateway.session-opened`
    ToolGatewaySessionOpened,
    /// `tool-gateway.session-closed`
    ToolGatewaySessionClosed,
    /// `tool-gateway.tool-invoked`
    ToolGatewayInvoked,

    // Docker
    /// `docker.socket-trust-grant`
    DockerSocketTrustGrant,

    // Proposals
    /// `proposal.approved`
    ProposalApproved,
    /// `proposal.rejected`
    ProposalRejected,
    /// `proposal.expired`
    ProposalExpired,
}

/// Error returned when a string does not map to any known [`AuditEvent`].
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AuditEventParseError {
    /// The provided string is not in the architecture §6.6 vocabulary.
    #[error("audit kind {0:?} is not in the architecture §6.6 vocabulary")]
    Unknown(String),
}

impl AuditEvent {
    /// Wire form (dotted lowercase) recorded in `audit_log.kind`.
    pub const fn kind_str(&self) -> &'static str {
        match self {
            Self::AuthLoginSucceeded => "auth.login-succeeded",
            Self::AuthLoginFailed => "auth.login-failed",
            Self::AuthLogout => "auth.logout",
            Self::AuthSessionRevoked => "auth.session-revoked",
            Self::AuthBootstrapCredentialsRotated => "auth.bootstrap-credentials-rotated",
            Self::AuthBootstrapCredentialsCreated => "auth.bootstrap-credentials-created",
            Self::CaddyCapabilityProbeCompleted => "caddy.capability-probe-completed",
            Self::CaddyOwnershipSentinelConflict => "caddy.ownership-sentinel-conflict",
            Self::CaddyReconnected => "caddy.reconnected",
            Self::CaddyUnreachable => "caddy.unreachable",
            Self::ApplySucceeded => "config.applied",
            Self::ApplyFailed => "config.apply-failed",
            Self::DriftDetected => "config.drift-detected",
            Self::DriftResolved => "config.drift-resolved",
            Self::ConfigRolledBack => "config.rolled-back",
            Self::DriftDeferred => "config.drift-deferred",
            Self::DriftAutoDeferred => "config.drift-auto-deferred",
            Self::ConfigRebased => "config.rebased",
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
            Self::SecretsRevealed => "secrets.revealed",
            Self::SecretsMasterKeyRotated => "secrets.master-key-rotated",
            Self::PolicyPresetAttached => "policy-preset.attached",
            Self::PolicyPresetDetached => "policy-preset.detached",
            Self::PolicyPresetUpgraded => "policy-preset.upgraded",
            Self::PolicyRegistryMismatch => "policy.registry-mismatch",
            Self::ImportCaddyfile => "import.caddyfile",
            Self::ExportBundle => "export.bundle",
            Self::ExportCaddyJson => "export.caddy-json",
            Self::ExportCaddyfile => "export.caddyfile",
            Self::ToolGatewaySessionOpened => "tool-gateway.session-opened",
            Self::ToolGatewaySessionClosed => "tool-gateway.session-closed",
            Self::ToolGatewayInvoked => "tool-gateway.tool-invoked",
            Self::DockerSocketTrustGrant => "docker.socket-trust-grant",
            Self::ProposalApproved => "proposal.approved",
            Self::ProposalRejected => "proposal.rejected",
            Self::ProposalExpired => "proposal.expired",
        }
    }
}

impl fmt::Display for AuditEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind_str())
    }
}

impl FromStr for AuditEvent {
    type Err = AuditEventParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auth.login-succeeded" => Ok(Self::AuthLoginSucceeded),
            "auth.login-failed" => Ok(Self::AuthLoginFailed),
            "auth.logout" => Ok(Self::AuthLogout),
            "auth.session-revoked" => Ok(Self::AuthSessionRevoked),
            "auth.bootstrap-credentials-rotated" => Ok(Self::AuthBootstrapCredentialsRotated),
            "auth.bootstrap-credentials-created" => Ok(Self::AuthBootstrapCredentialsCreated),
            "caddy.capability-probe-completed" => Ok(Self::CaddyCapabilityProbeCompleted),
            "caddy.ownership-sentinel-conflict" => Ok(Self::CaddyOwnershipSentinelConflict),
            "caddy.reconnected" => Ok(Self::CaddyReconnected),
            "caddy.unreachable" => Ok(Self::CaddyUnreachable),
            "config.applied" => Ok(Self::ApplySucceeded),
            "config.apply-failed" => Ok(Self::ApplyFailed),
            "config.drift-detected" => Ok(Self::DriftDetected),
            "config.drift-resolved" => Ok(Self::DriftResolved),
            "config.rolled-back" => Ok(Self::ConfigRolledBack),
            "config.drift-deferred" => Ok(Self::DriftDeferred),
            "config.drift-auto-deferred" => Ok(Self::DriftAutoDeferred),
            "config.rebased" => Ok(Self::ConfigRebased),
            "mutation.proposed" => Ok(Self::MutationProposed),
            "mutation.submitted" => Ok(Self::MutationSubmitted),
            "mutation.applied" => Ok(Self::MutationApplied),
            "mutation.conflicted" => Ok(Self::MutationConflicted),
            "mutation.rejected" => Ok(Self::MutationRejected),
            "mutation.rejected.missing-expected-version" => {
                Ok(Self::MutationRejectedMissingExpectedVersion)
            }
            "mutation.rebased.auto" => Ok(Self::MutationRebasedAuto),
            "mutation.rebased.manual" => Ok(Self::MutationRebasedManual),
            "mutation.rebase.expired" => Ok(Self::MutationRebaseExpired),
            "secrets.revealed" => Ok(Self::SecretsRevealed),
            "secrets.master-key-rotated" => Ok(Self::SecretsMasterKeyRotated),
            "policy-preset.attached" => Ok(Self::PolicyPresetAttached),
            "policy-preset.detached" => Ok(Self::PolicyPresetDetached),
            "policy-preset.upgraded" => Ok(Self::PolicyPresetUpgraded),
            "policy.registry-mismatch" => Ok(Self::PolicyRegistryMismatch),
            "import.caddyfile" => Ok(Self::ImportCaddyfile),
            "export.bundle" => Ok(Self::ExportBundle),
            "export.caddy-json" => Ok(Self::ExportCaddyJson),
            "export.caddyfile" => Ok(Self::ExportCaddyfile),
            "tool-gateway.session-opened" => Ok(Self::ToolGatewaySessionOpened),
            "tool-gateway.session-closed" => Ok(Self::ToolGatewaySessionClosed),
            "tool-gateway.tool-invoked" => Ok(Self::ToolGatewayInvoked),
            "docker.socket-trust-grant" => Ok(Self::DockerSocketTrustGrant),
            "proposal.approved" => Ok(Self::ProposalApproved),
            "proposal.rejected" => Ok(Self::ProposalRejected),
            "proposal.expired" => Ok(Self::ProposalExpired),
            other => Err(AuditEventParseError::Unknown(other.to_owned())),
        }
    }
}

/// Compile-time regex of the §6.6 dotted form. Every `kind_str()` MUST match.
pub const AUDIT_KIND_REGEX: &str = r"^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+$";

/// Canonical audit event kind vocabulary (§6.6).
///
/// Every [`AuditEvent`] variant's `Display` output must appear in this list.
/// Phase 6's audit-row writer imports this constant to populate the `kind`
/// column without re-declaring the vocabulary.
pub const AUDIT_KIND_VOCAB: &[&str] = &[
    "auth.bootstrap-credentials-created",
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
    "config.drift-auto-deferred",
    "config.drift-deferred",
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn all_variants() -> Vec<AuditEvent> {
        use AuditEvent::*;
        vec![
            AuthLoginSucceeded,
            AuthLoginFailed,
            AuthLogout,
            AuthSessionRevoked,
            AuthBootstrapCredentialsRotated,
            AuthBootstrapCredentialsCreated,
            CaddyCapabilityProbeCompleted,
            CaddyOwnershipSentinelConflict,
            CaddyReconnected,
            CaddyUnreachable,
            ApplySucceeded,
            ApplyFailed,
            DriftDetected,
            DriftResolved,
            ConfigRolledBack,
            DriftDeferred,
            DriftAutoDeferred,
            ConfigRebased,
            MutationProposed,
            MutationSubmitted,
            MutationApplied,
            MutationConflicted,
            MutationRejected,
            MutationRejectedMissingExpectedVersion,
            MutationRebasedAuto,
            MutationRebasedManual,
            MutationRebaseExpired,
            SecretsRevealed,
            SecretsMasterKeyRotated,
            PolicyPresetAttached,
            PolicyPresetDetached,
            PolicyPresetUpgraded,
            PolicyRegistryMismatch,
            ImportCaddyfile,
            ExportBundle,
            ExportCaddyJson,
            ExportCaddyfile,
            ToolGatewaySessionOpened,
            ToolGatewaySessionClosed,
            ToolGatewayInvoked,
            DockerSocketTrustGrant,
            ProposalApproved,
            ProposalRejected,
            ProposalExpired,
        ]
    }

    #[test]
    fn display_round_trip_every_variant() {
        for v in all_variants() {
            let kind = v.kind_str();
            let parsed =
                AuditEvent::from_str(kind).expect("from_str should succeed for every kind_str()");
            assert_eq!(
                parsed, v,
                "round-trip mismatch for {v:?}: from_str({kind:?}) returned {parsed:?}"
            );
        }
    }

    /// Returns true if `s` matches the §6.6 dotted kind pattern
    /// `^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+$` without requiring a regex crate.
    fn matches_audit_kind_regex(s: &str) -> bool {
        // Must contain at least one dot.
        if !s.contains('.') {
            return false;
        }
        // Every segment (split by '.') must be non-empty, start with [a-z],
        // and consist only of [a-z0-9-].
        s.split('.').all(|seg| {
            let mut chars = seg.chars();
            let Some(first) = chars.next() else {
                return false;
            };
            first.is_ascii_lowercase()
                && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
    }

    #[test]
    fn kind_strings_match_section_6_6_regex() {
        for v in all_variants() {
            let kind = v.kind_str();
            assert!(
                matches_audit_kind_regex(kind),
                "kind_str for {v:?} ({kind:?}) does not match AUDIT_KIND_REGEX ({AUDIT_KIND_REGEX})"
            );
        }
    }

    #[test]
    fn unknown_kind_rejected() {
        let result = AuditEvent::from_str("not.a.kind");
        assert_eq!(
            result,
            Err(AuditEventParseError::Unknown("not.a.kind".to_owned()))
        );
    }

    #[test]
    fn tier_1_set_complete() {
        let kinds: HashSet<&str> = all_variants().iter().map(AuditEvent::kind_str).collect();
        // Assert each spec-listed Tier 1 variant is present.
        let tier_1 = [
            "auth.login-succeeded",
            "auth.login-failed",
            "auth.logout",
            "auth.session-revoked",
            "auth.bootstrap-credentials-rotated",
            "caddy.capability-probe-completed",
            "caddy.ownership-sentinel-conflict",
            "caddy.reconnected",
            "caddy.unreachable",
            "config.applied",
            "config.apply-failed",
            "config.drift-detected",
            "config.drift-resolved",
            "config.rolled-back",
            "mutation.proposed",
            "mutation.submitted",
            "mutation.applied",
            "mutation.conflicted",
            "mutation.rejected",
            "mutation.rejected.missing-expected-version",
            "secrets.revealed",
            "secrets.master-key-rotated",
        ];
        for kind in tier_1 {
            assert!(
                kinds.contains(kind),
                "Tier 1 kind {kind:?} missing from AuditEvent"
            );
        }
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
        let unique: HashSet<&str> = variants.iter().map(AuditEvent::kind_str).collect();
        assert_eq!(
            unique.len(),
            variant_count,
            "duplicate Display strings detected among AuditEvent variants"
        );
    }
}
