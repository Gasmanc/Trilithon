//! Global proxy configuration value types.

use serde::{Deserialize, Serialize};

use crate::model::primitive::double_option;

/// Global proxy configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalConfig {
    /// Address the Caddy admin API listens on.
    pub admin_listen: Option<String>,
    /// Default SNI hostname to use when none is matched.
    pub default_sni: Option<String>,
    /// Log verbosity level.
    pub log_level: Option<String>,
}

/// Partial update for [`GlobalConfig`].
///
/// The `Option<Option<T>>` pattern distinguishes three states:
/// - outer `None` — field unchanged
/// - outer `Some(None)` — clear the field
/// - outer `Some(Some(v))` — set to `v`
// The three-state patch pattern requires Option<Option<T>> by design.
#[allow(clippy::option_option)] // zd:patch-triple-state expires:2027-01-01 reason:intentional absent/clear/set distinction
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalConfigPatch {
    /// Set, clear, or leave unchanged the admin listen address.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "double_option::deserialize"
    )]
    pub admin_listen: Option<Option<String>>,
    /// Set, clear, or leave unchanged the default SNI.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "double_option::deserialize"
    )]
    pub default_sni: Option<Option<String>>,
    /// Set, clear, or leave unchanged the log level.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "double_option::deserialize"
    )]
    pub log_level: Option<Option<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_default_is_all_none() {
        let patch = GlobalConfigPatch::default();
        assert!(patch.admin_listen.is_none());
        assert!(patch.default_sni.is_none());
        assert!(patch.log_level.is_none());
    }
}
