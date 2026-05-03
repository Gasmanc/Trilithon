//! Matcher types for route request matching.

use serde::{Deserialize, Serialize};

/// A set of conditions that a request must match for a route to apply.
///
/// All non-empty fields are AND-combined. An empty `MatcherSet` matches everything.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct MatcherSet {
    /// Path prefix or exact patterns.
    pub paths: Vec<PathMatcher>,
    /// Allowed HTTP methods.
    pub methods: Vec<HttpMethod>,
    /// Query parameter conditions.
    pub query: Vec<QueryMatcher>,
    /// Request header conditions.
    pub headers: Vec<HeaderMatcher>,
    /// Remote address CIDR conditions.
    pub remote: Vec<CidrMatcher>,
}

/// Matches a request path (exact or prefix).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, schemars::JsonSchema)]
pub struct PathMatcher(pub String);

/// HTTP request methods supported for matching.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, schemars::JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    /// HTTP GET
    Get,
    /// HTTP POST
    Post,
    /// HTTP PUT
    Put,
    /// HTTP PATCH
    Patch,
    /// HTTP DELETE
    Delete,
    /// HTTP HEAD
    Head,
    /// HTTP OPTIONS
    Options,
}

/// Matches a query parameter by key, and optionally by value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, schemars::JsonSchema)]
pub struct QueryMatcher {
    /// Query parameter name.
    pub key: String,
    /// If present, the value must match exactly.
    pub value: Option<String>,
}

/// Matches a request header by name, and optionally by value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, schemars::JsonSchema)]
pub struct HeaderMatcher {
    /// Header name (case-insensitive per HTTP spec).
    pub name: String,
    /// If present, the header value must match exactly.
    pub value: Option<String>,
}

/// Matches a remote IP address against a CIDR range (IPv4 or IPv6).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, schemars::JsonSchema)]
pub struct CidrMatcher(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matcher_set_serde_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let set = MatcherSet {
            paths: vec![PathMatcher("/api/v1".into())],
            methods: vec![HttpMethod::Get, HttpMethod::Post],
            query: vec![QueryMatcher {
                key: "foo".into(),
                value: Some("bar".into()),
            }],
            headers: vec![HeaderMatcher {
                name: "X-Custom".into(),
                value: None,
            }],
            remote: vec![CidrMatcher("10.0.0.0/8".into())],
        };

        let json = serde_json::to_string(&set)?;
        let deserialized: MatcherSet = serde_json::from_str(&json)?;
        assert_eq!(set, deserialized);
        Ok(())
    }
}
