//! [`DiffEngine`] — structural diff between desired state and live Caddy
//! configuration.
//!
//! The trait is pure: no I/O, no async.  The concrete implementation lives in
//! `adapters`.  The applier calls `structural_diff` after a successful `POST
//! /load` to confirm that Caddy reflects the desired state.
//!
//! # Ignored paths
//!
//! Architecture §7.2 specifies a set of JSON pointer prefixes that are exempt
//! from the equivalence check (e.g. runtime-managed TLS state, `@id`).  The
//! ignore list is enforced by the implementation, not the caller.

use crate::caddy::CaddyConfig;
use crate::model::desired_state::DesiredState;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors returned by [`DiffEngine::structural_diff`].
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DiffError {
    /// The desired state or observed config could not be serialised/deserialised
    /// during the diff.
    #[error("diff serialisation error: {detail}")]
    Serialisation {
        /// Human-readable detail.
        detail: String,
    },
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Computes a structural diff between a rendered [`DesiredState`] and the
/// live config observed from Caddy after a `POST /load`.
///
/// Implementations MUST apply the §7.2 ignore list before returning
/// differences; paths on that list MUST NOT appear in the returned `Vec`.
pub trait DiffEngine: Send + Sync + 'static {
    /// Return the list of JSON pointer paths that differ between `desired`
    /// (as rendered) and `observed` (from `GET /config/`), after applying
    /// the §7.2 ignore list.
    ///
    /// An empty `Vec` means the configurations are equivalent.
    ///
    /// # Errors
    ///
    /// Returns [`DiffError::Serialisation`] if the desired state cannot be
    /// rendered to a comparable form.
    fn structural_diff(
        &self,
        desired: &DesiredState,
        observed: &CaddyConfig,
    ) -> Result<Vec<String>, DiffError>;
}

// ---------------------------------------------------------------------------
// No-op (always-equivalent) implementation for V1
// ---------------------------------------------------------------------------

/// A [`DiffEngine`] that always reports no differences.
///
/// Used in V1 where post-load equivalence checking is deliberately shallow —
/// Caddy accepting the config document is sufficient evidence that it was
/// applied.  A full structural diff lands in Phase 8.
pub struct NoOpDiffEngine;

impl DiffEngine for NoOpDiffEngine {
    fn structural_diff(
        &self,
        _desired: &DesiredState,
        _observed: &CaddyConfig,
    ) -> Result<Vec<String>, DiffError> {
        Ok(Vec::new())
    }
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
    use crate::caddy::CaddyConfig;
    use crate::model::desired_state::DesiredState;

    #[test]
    fn no_op_always_empty() {
        let engine = NoOpDiffEngine;
        let desired = DesiredState::empty();
        let observed = CaddyConfig(serde_json::json!({}));
        let diff = engine.structural_diff(&desired, &observed).expect("ok");
        assert!(
            diff.is_empty(),
            "NoOpDiffEngine must always return an empty diff"
        );
    }
}
