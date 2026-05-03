//! Domain model types and identifiers.

pub mod desired_state;
pub mod global;
pub mod header;
pub mod identifiers;
pub mod matcher;
pub mod policy;
pub mod primitive;
pub mod redirect;
pub mod route;
pub mod tls;
pub mod upstream;

// Re-exports for convenience
pub use desired_state::DesiredState;
pub use global::{GlobalConfig, GlobalConfigPatch};
pub use header::{HeaderOp, HeaderRules};
pub use identifiers::{MutationId, PolicyId, PresetId, RouteId, UpstreamId};
pub use matcher::{CidrMatcher, HeaderMatcher, HttpMethod, MatcherSet, PathMatcher, QueryMatcher};
pub use policy::{PolicyAttachment, PresetVersion};
pub use primitive::{CaddyModule, JsonPointer, UnixSeconds};
pub use redirect::RedirectRule;
pub use route::{HostPattern, HostnameError, Route, RoutePolicyAttachment, validate_hostname};
pub use tls::{TlsConfig, TlsConfigPatch, TlsIssuer};
pub use upstream::{Upstream, UpstreamDestination, UpstreamProbe};
