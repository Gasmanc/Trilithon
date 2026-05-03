//! Caddy admin API types and client trait.

pub mod capabilities;
pub mod client;
pub mod error;
pub mod types;

pub use capabilities::{CaddyCapabilities, CapabilitySet};
pub use client::CaddyClient;
pub use error::CaddyError;
pub use types::{
    CaddyConfig, CaddyJsonPointer, HealthState, JsonPatch, JsonPatchOp, LoadedModules,
    TlsCertificate, UpstreamHealth,
};
