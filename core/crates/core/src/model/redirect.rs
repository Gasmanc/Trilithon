//! Redirect rule for route-level HTTP redirects.

use serde::{Deserialize, Serialize};

/// Configures an HTTP redirect for a route.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RedirectRule {
    /// The destination URL or path to redirect to.
    pub to: String,
    /// HTTP status code to use (e.g. 301, 302, 307, 308).
    pub status: u16,
}
