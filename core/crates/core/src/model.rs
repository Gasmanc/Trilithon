//! Domain model types and identifiers.

pub mod identifiers;
pub mod primitive;

// Re-exports for convenience
pub use identifiers::{MutationId, PolicyId, PresetId, RouteId, UpstreamId};
pub use primitive::{CaddyModule, JsonPointer, UnixSeconds};
