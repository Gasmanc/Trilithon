//! The closed §6.6 audit `kind` vocabulary.
//!
//! Every string in this slice is a valid value for `AuditEventRow::kind`.
//! Production callers and test doubles validate against this list; the list
//! lives here once so both sides agree on the canonical vocabulary.

/// The complete V1 audit `kind` vocabulary from architecture §6.6.
///
/// Values match the regex `^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+$` and are
/// sorted alphabetically.
pub const AUDIT_KINDS: &[&str] = &[
    "auth.bootstrap-credentials-created",
    "auth.bootstrap-credentials-rotated",
    "auth.login-failed",
    "auth.login-succeeded",
    "auth.logout",
    "auth.session-revoked",
    "caddy.capability-probe-completed",
    "caddy.ownership-sentinel-conflict",
    "caddy.ownership-sentinel-takeover",
    "caddy.reconnected",
    "caddy.unreachable",
    "config.applied",
    "config.apply-failed",
    "config.drift-auto-deferred",
    "config.drift-deferred",
    "config.drift-detected",
    "config.drift-resolved",
    "config.exported",
    "config.imported",
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
    "secrets.master-key-fallback-engaged",
    "secrets.master-key-rotated",
    "secrets.revealed",
    "system.restore-applied",
    "system.restore-cross-machine",
    "tool-gateway.session-closed",
    "tool-gateway.session-opened",
];
