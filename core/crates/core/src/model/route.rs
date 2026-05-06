//! Route model and hostname pattern types.

use serde::{Deserialize, Serialize};

use crate::model::{
    header::HeaderRules,
    identifiers::{PresetId, RouteId, UpstreamId},
    matcher::MatcherSet,
    primitive::UnixSeconds,
    redirect::RedirectRule,
};

/// A routing rule that maps incoming requests to one or more upstreams.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Route {
    /// Unique identifier.
    pub id: RouteId,
    /// Hostname patterns this route matches.
    pub hostnames: Vec<HostPattern>,
    /// Upstreams traffic is forwarded to.
    pub upstreams: Vec<UpstreamId>,
    /// Additional request-matching conditions.
    pub matchers: MatcherSet,
    /// Header manipulation rules.
    pub headers: HeaderRules,
    /// Optional redirect instead of proxying.
    pub redirects: Option<RedirectRule>,
    /// Optional policy preset attachment.
    pub policy_attachment: Option<RoutePolicyAttachment>,
    /// Whether this route is active.
    pub enabled: bool,
    /// Creation timestamp (Unix seconds). Set when the route is first created.
    pub created_at: UnixSeconds,
    /// Last-updated timestamp (Unix seconds).
    ///
    /// Note: `apply_mutation` does not update this field — it is set by the
    /// persistence layer when writing to storage (Phase 5+). Until then,
    /// this field reflects the value carried in the mutation payload only.
    pub updated_at: UnixSeconds,
}

/// A hostname pattern: either an exact hostname or a single-level wildcard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum HostPattern {
    /// An exact hostname match, e.g. `example.com`.
    Exact(String),
    /// A wildcard match for one subdomain level, e.g. `*.example.com`.
    Wildcard(String),
}

/// Attaches a policy preset (with a pinned version) to a route.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RoutePolicyAttachment {
    /// The preset being applied.
    pub preset_id: PresetId,
    /// The version of the preset at attachment time.
    pub preset_version: u32,
}

/// Errors produced by [`validate_hostname`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HostnameError {
    /// The input string was empty.
    #[error("hostname is empty")]
    Empty,
    /// A label starts or ends with a hyphen.
    #[error("hostname label {label} starts or ends with hyphen")]
    HyphenBoundary {
        /// The offending label.
        label: String,
    },
    /// A label exceeds 63 characters.
    #[error("hostname label {label} exceeds 63 characters")]
    LabelTooLong {
        /// The offending label.
        label: String,
    },
    /// The total hostname length exceeds 253 characters.
    #[error("hostname total length exceeds 253 characters")]
    TotalTooLong,
    /// The hostname contains a character that is not allowed by RFC 952/1123.
    #[error("hostname contains invalid character {found}")]
    InvalidCharacter {
        /// The invalid character encountered.
        found: char,
    },
    /// A wildcard pattern is not of the required form `*.example.com`.
    #[error("wildcard {pattern} must be of the form '*.example.com'")]
    InvalidWildcard {
        /// The invalid wildcard pattern.
        pattern: String,
    },
}

/// Validate a hostname string and return the appropriate [`HostPattern`].
///
/// Accepts plain hostnames (`example.com`) and single-level wildcards
/// (`*.example.com`).  Validation follows RFC 952 and RFC 1123 rules:
/// - total length ≤ 253
/// - each label: 1-63 characters, ASCII alphanumeric or hyphen, not starting
///   or ending with a hyphen.
///
/// # Errors
///
/// Returns [`HostnameError`] describing the first validation failure found.
pub fn validate_hostname(s: &str) -> Result<HostPattern, HostnameError> {
    if s.is_empty() {
        return Err(HostnameError::Empty);
    }

    // Check total length before any processing — applies to wildcards too.
    if s.len() > 253 {
        return Err(HostnameError::TotalTooLong);
    }

    // Handle wildcard prefix.
    if let Some(rest) = s.strip_prefix("*.") {
        // Reject nested wildcards like `*.*.example.com` — rest must not contain `*`.
        if rest.contains('*') {
            return Err(HostnameError::InvalidWildcard { pattern: s.into() });
        }
        // The remainder must contain at least one dot (i.e. at least two labels).
        if rest.is_empty() || !rest.contains('.') {
            return Err(HostnameError::InvalidWildcard { pattern: s.into() });
        }
        // Validate the non-wildcard portion.
        validate_labels(rest)?;
        return Ok(HostPattern::Wildcard(s.into()));
    }

    // Reject patterns that contain `*` but didn't match `*.` prefix.
    if s.contains('*') {
        return Err(HostnameError::InvalidWildcard { pattern: s.into() });
    }

    validate_labels(s)?;
    Ok(HostPattern::Exact(s.into()))
}

/// Validate individual labels in a dot-separated hostname.
fn validate_labels(s: &str) -> Result<(), HostnameError> {
    if s.len() > 253 {
        return Err(HostnameError::TotalTooLong);
    }

    for label in s.split('.') {
        if label.is_empty() || label.starts_with('-') || label.ends_with('-') {
            return Err(HostnameError::HyphenBoundary {
                label: label.into(),
            });
        }
        if label.len() > 63 {
            return Err(HostnameError::LabelTooLong {
                label: label.into(),
            });
        }
        for ch in label.chars() {
            if !ch.is_ascii_alphanumeric() && ch != '-' {
                return Err(HostnameError::InvalidCharacter { found: ch });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_exact_host() {
        let result = validate_hostname("example.com");
        assert_eq!(result, Ok(HostPattern::Exact("example.com".into())));
    }

    #[test]
    fn valid_wildcard_host() {
        let result = validate_hostname("*.example.com");
        assert_eq!(result, Ok(HostPattern::Wildcard("*.example.com".into())));
    }

    #[test]
    fn reject_double_wildcard() {
        let result = validate_hostname("*.*.example.com");
        assert_eq!(
            result,
            Err(HostnameError::InvalidWildcard {
                pattern: "*.*.example.com".into()
            })
        );
    }

    #[test]
    fn reject_label_64_chars() {
        let long_label = "a".repeat(64);
        let hostname = format!("{long_label}.example.com");
        let result = validate_hostname(&hostname);
        assert_eq!(
            result,
            Err(HostnameError::LabelTooLong { label: long_label })
        );
    }

    #[test]
    fn reject_total_254_chars() {
        // Build a hostname that is exactly 254 characters long.
        // Each label can be at most 63 chars; use labels of 63 + dot = 64 each.
        // 3 * 64 = 192 chars so far; we need 254 total.
        // 254 - 192 = 62 chars remaining for the last segment (no trailing dot).
        let label63 = "a".repeat(63);
        let label62 = "b".repeat(62);
        let hostname = format!("{label63}.{label63}.{label63}.{label62}");
        assert_eq!(hostname.len(), 254);
        let result = validate_hostname(&hostname);
        assert_eq!(result, Err(HostnameError::TotalTooLong));
    }

    #[test]
    fn reject_label_starting_hyphen() {
        let result = validate_hostname("-foo.example.com");
        assert_eq!(
            result,
            Err(HostnameError::HyphenBoundary {
                label: "-foo".into()
            })
        );
    }
}
