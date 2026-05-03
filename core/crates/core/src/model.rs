//! Domain model types and identifiers.

pub mod header;
pub mod identifiers;
pub mod matcher;
pub mod primitive;
pub mod redirect;
pub mod route;
pub mod upstream;

// Re-exports for convenience
pub use header::{HeaderOp, HeaderRules};
pub use identifiers::{MutationId, PolicyId, PresetId, RouteId, UpstreamId};
pub use matcher::{CidrMatcher, HeaderMatcher, HttpMethod, MatcherSet, PathMatcher, QueryMatcher};
pub use primitive::{CaddyModule, JsonPointer, UnixSeconds};
pub use redirect::RedirectRule;
pub use route::{HostPattern, HostnameError, Route, RoutePolicyAttachment, validate_hostname};
pub use upstream::{Upstream, UpstreamDestination, UpstreamProbe};
