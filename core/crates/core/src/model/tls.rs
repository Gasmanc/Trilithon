//! TLS configuration value types.

use serde::{Deserialize, Serialize};

use crate::model::primitive::double_option;

/// Global TLS configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TlsConfig {
    /// ACME contact email.
    pub email: Option<String>,
    /// Whether on-demand TLS provisioning is enabled.
    pub on_demand_enabled: bool,
    /// URL Caddy will query to decide whether to obtain a certificate on-demand.
    pub on_demand_ask_url: Option<String>,
    /// Default certificate issuer.
    pub default_issuer: Option<TlsIssuer>,
}

/// Partial update for [`TlsConfig`].
///
/// The `Option<Option<T>>` pattern distinguishes three states:
/// - outer `None` — field unchanged
/// - outer `Some(None)` — clear the field
/// - outer `Some(Some(v))` — set to `v`
// The three-state patch pattern requires Option<Option<T>> by design.
#[allow(clippy::option_option)] // zd:patch-triple-state expires:2027-01-01 reason:intentional absent/clear/set distinction
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TlsConfigPatch {
    /// Set, clear, or leave unchanged the ACME contact email.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "double_option::deserialize"
    )]
    pub email: Option<Option<String>>,
    /// Set or leave unchanged on-demand TLS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_demand_enabled: Option<bool>,
    /// Set, clear, or leave unchanged the on-demand ask URL.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "double_option::deserialize"
    )]
    pub on_demand_ask_url: Option<Option<String>>,
    /// Set, clear, or leave unchanged the default issuer.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "double_option::deserialize"
    )]
    pub default_issuer: Option<Option<TlsIssuer>>,
}

/// Certificate issuer variant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "issuer", rename_all = "snake_case")]
pub enum TlsIssuer {
    /// ACME protocol issuer (e.g. Let's Encrypt).
    Acme {
        /// ACME directory URL.
        directory_url: String,
    },
    /// Internal/self-signed issuer.
    Internal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_distinguishes_unset_and_clear() -> Result<(), Box<dyn std::error::Error>> {
        // State 1: field absent from patch (outer None — unchanged)
        let unset = TlsConfigPatch::default();
        let json = serde_json::to_string(&unset)?;
        assert!(
            !json.contains("email"),
            "absent field must not appear in JSON"
        );

        // State 2: outer Some(None) — clear the field
        let clear = TlsConfigPatch {
            email: Some(None),
            ..Default::default()
        };
        let json = serde_json::to_string(&clear)?;
        assert!(
            json.contains("\"email\":null"),
            "clear must serialise as null"
        );
        let rt: TlsConfigPatch = serde_json::from_str(&json)?;
        assert_eq!(rt.email, Some(None));

        // State 3: outer Some(Some(v)) — set to a value
        let set = TlsConfigPatch {
            email: Some(Some("admin@example.com".to_owned())),
            ..Default::default()
        };
        let json = serde_json::to_string(&set)?;
        assert!(json.contains("admin@example.com"));
        let rt: TlsConfigPatch = serde_json::from_str(&json)?;
        assert_eq!(rt.email, Some(Some("admin@example.com".to_owned())));

        Ok(())
    }
}
