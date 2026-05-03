//! Mutation types for desired-state patches.

#![allow(clippy::mod_module_files)]

pub mod envelope;
pub mod patches;
pub mod types;

pub use envelope::{EnvelopeError, MutationEnvelope, parse_envelope};
pub use patches::{ParsedCaddyfile, RoutePatch, UpstreamPatch};
pub use types::{Mutation, MutationKind};
