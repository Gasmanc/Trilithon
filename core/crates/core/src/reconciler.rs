//! Reconciler — converts a [`crate::model::desired_state::DesiredState`] into
//! a Caddy 2.x JSON configuration document.
//!
//! The renderer is pure-core: no I/O, no async, no Caddy reachability. Its
//! output is byte-identical for byte-identical inputs, making it suitable for
//! use in content-addressed snapshots.

pub mod applier;
pub mod render;

pub use applier::{AppliedState, ApplyError, ApplyFailureKind, ApplyOutcome, ReloadKind};
pub use render::{CaddyJsonRenderer, DefaultCaddyJsonRenderer, RenderError, canonical_json_bytes};
