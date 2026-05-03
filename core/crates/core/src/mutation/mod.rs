//! Mutation types for desired-state patches.

#![allow(clippy::mod_module_files)]

pub mod capability;
pub mod envelope;
pub mod error;
pub mod outcome;
pub mod patches;
pub mod types;

pub use capability::check_capabilities;
pub use envelope::{EnvelopeError, MutationEnvelope, parse_envelope};
pub use error::{ForbiddenReason, MutationError, SchemaErrorKind, ValidationRule};
pub use outcome::{Diff, DiffChange, MutationOutcome};
pub use patches::{ParsedCaddyfile, RoutePatch, UpstreamPatch};
pub use types::{Mutation, MutationKind};
