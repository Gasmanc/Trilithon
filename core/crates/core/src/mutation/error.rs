//! Mutation error types.

use crate::model::primitive::{CaddyModule, JsonPointer};
use crate::mutation::types::MutationKind;

/// All reasons a mutation can be rejected.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum MutationError {
    /// The mutation payload failed domain validation.
    #[error("validation failed: {hint} (rule: {rule:?}, path: {path:?})")]
    Validation {
        /// Which rule was violated.
        rule: ValidationRule,
        /// JSON Pointer to the offending field.
        path: JsonPointer,
        /// Human-readable explanation.
        hint: String,
    },

    /// A required Caddy module is not available.
    #[error("capability missing: {module:?} required by {required_by:?}")]
    CapabilityMissing {
        /// The missing module.
        module: CaddyModule,
        /// The mutation kind that requires it.
        required_by: MutationKind,
    },

    /// Optimistic-concurrency version mismatch.
    #[error(
        "optimistic conflict: observed version {observed_version}, mutation expected {expected_version}"
    )]
    Conflict {
        /// The version the store actually holds.
        observed_version: i64,
        /// The version the mutation claimed to expect.
        expected_version: i64,
    },

    /// The mutation payload violates the schema.
    #[error("schema error at {field:?}: {kind:?}")]
    Schema {
        /// JSON Pointer to the offending field.
        field: JsonPointer,
        /// The specific schema violation.
        kind: SchemaErrorKind,
    },

    /// The operation is not permitted in the current state.
    #[error("forbidden: {reason}")]
    Forbidden {
        /// Why the operation is forbidden.
        reason: ForbiddenReason,
    },
}

/// Domain validation rules that a mutation can violate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationRule {
    /// A hostname does not conform to RFC 1123.
    HostnameInvalid,
    /// A referenced upstream does not exist in the desired state.
    UpstreamReferenceMissing,
    /// A referenced policy preset does not exist in the registry.
    PolicyPresetMissing,
    /// A route id is already present in the desired state.
    DuplicateRouteId,
    /// An upstream id is already present in the desired state.
    /// Added for Slice 4.9 (`CreateUpstream` / `ImportFromCaddyfile` pre-conditions).
    DuplicateUpstreamId,
    /// A route does not exist in the desired state.
    /// Added for Slice 4.9 (`DeleteRoute` / `UpdateRoute` / `DetachPolicy` pre-conditions).
    RouteMissing,
    /// A route references a policy attachment that does not exist.
    PolicyAttachmentMissing,
    /// The upstream being deleted is still referenced by one or more routes.
    /// Added for Slice 4.9 (`DeleteUpstream` referential integrity).
    UpstreamStillReferenced,
    /// Every field in a patch is `None`, so the mutation would make no change.
    NoOpMutation,
    /// A policy preset exists in the registry but not at the requested version.
    PolicyPresetVersionMismatch,
    /// A preset version of 0 was supplied; preset versions start at 1.
    PolicyPresetVersionZero,
    /// A redirect URL has an invalid or disallowed scheme (only http/https are accepted).
    RedirectUrlInvalid,
    /// The on-demand TLS ask URL has an invalid scheme or disallowed destination.
    OnDemandAskUrlInvalid,
    /// A CIDR matcher string is not valid CIDR notation.
    CidrInvalid,
}

/// Kinds of schema violations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaErrorKind {
    /// The field name is not recognised by the schema.
    UnknownField,
    /// The field value has the wrong type.
    TypeMismatch {
        /// The type that was expected.
        expected: String,
        /// The type that was found.
        found: String,
    },
    /// A numeric field value lies outside its allowed range.
    OutOfRange {
        /// The value that was supplied.
        value: String,
        /// A human-readable description of the allowed range.
        bounds: String,
    },
}

/// Reasons a mutation may be forbidden.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ForbiddenReason {
    /// The rollback target snapshot is not known to the store.
    #[error("rollback target snapshot is not available in the store")]
    RollbackTargetUnknown,
    /// The requested policy change would downgrade the effective security posture.
    #[error("policy version downgrade is not permitted; use UpgradePolicy to advance versions")]
    PolicyDowngrade,
    /// The route does not have a policy attachment — cannot upgrade a non-existent attachment.
    #[error("route has no policy attachment; attach a policy with AttachPolicy first")]
    PolicyAttachmentMissing,
    /// The `DesiredState.version` counter has reached `i64::MAX` and cannot be incremented.
    #[error("state version counter overflow; version has reached i64::MAX")]
    VersionOverflow,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn validation_error_display_contains_hint() {
        let err = MutationError::Validation {
            rule: ValidationRule::HostnameInvalid,
            path: JsonPointer::root().push("hostnames").push("0"),
            hint: "not a valid RFC 1123 hostname".to_owned(),
        };
        let s = err.to_string();
        assert!(s.contains("not a valid RFC 1123 hostname"), "got: {s}");
        assert!(s.contains("HostnameInvalid"), "got: {s}");
    }

    #[test]
    fn conflict_error_display_contains_versions() {
        let err = MutationError::Conflict {
            observed_version: 10,
            expected_version: 9,
        };
        let s = err.to_string();
        assert!(s.contains("10"), "got: {s}");
        assert!(s.contains('9'), "got: {s}");
    }

    #[test]
    fn capability_missing_display() {
        let err = MutationError::CapabilityMissing {
            module: CaddyModule::new("http.handlers.rate_limit"),
            required_by: MutationKind::AttachPolicy,
        };
        let s = err.to_string();
        assert!(s.contains("http.handlers.rate_limit"), "got: {s}");
    }
}
