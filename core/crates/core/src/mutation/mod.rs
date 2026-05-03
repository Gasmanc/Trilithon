//! Mutation types for desired-state patches.

#![allow(clippy::mod_module_files)]

pub mod patches;

pub use patches::{ParsedCaddyfile, RoutePatch, UpstreamPatch};
