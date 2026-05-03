//! Identifier newtypes for domain entities.
//!
//! Each identifier is a ULID-bearing newtype with serialization support.

use serde::{Deserialize, Serialize};

/// Macro for creating ULID-based identifier types.
macro_rules! id_newtype {
    ($name:ident) => {
        #[doc = concat!("Unique identifier for ", stringify!($name), ".")]
        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            Serialize,
            Deserialize,
            schemars::JsonSchema,
        )]
        pub struct $name(pub String);

        impl $name {
            /// Generate a new identifier using a ULID.
            pub fn new() -> Self {
                Self(ulid::Ulid::new().to_string())
            }

            /// Get a string reference to the identifier.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

id_newtype!(RouteId);
id_newtype!(UpstreamId);
id_newtype!(PolicyId);
id_newtype!(PresetId);
id_newtype!(MutationId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulid_format() {
        let id = RouteId::new();
        let s = &id.0;
        assert_eq!(s.len(), 26, "ULID should be 26 characters");
        assert!(
            s.chars().all(|c| c.is_ascii_alphanumeric()),
            "ULID should contain only ASCII alphanumeric characters"
        );
        // Verify all characters are in the base32 alphabet [0-9A-Z]
        assert!(
            s.chars().all(|c| matches!(c, '0'..='9' | 'A'..='Z')),
            "ULID should match [0-9A-Z]{{26}}"
        );
    }

    #[test]
    fn route_id_new_creates_unique_ids() {
        let id1 = RouteId::new();
        let id2 = RouteId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn upstream_id_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let id = UpstreamId::new();
        let json = serde_json::to_string(&id)?;
        let deserialized: UpstreamId = serde_json::from_str(&json)?;
        assert_eq!(id, deserialized);
        Ok(())
    }

    #[test]
    fn id_as_str_returns_reference() {
        let id = PolicyId::new();
        let s = id.as_str();
        assert_eq!(s, &id.0);
        assert_eq!(s.len(), 26);
    }

    #[test]
    fn id_default_creates_new() {
        let id1 = PresetId::default();
        let id2 = PresetId::default();
        assert_ne!(id1, id2);
    }

    #[test]
    fn id_display_impl() {
        let id = MutationId::new();
        let displayed = format!("{id}");
        assert_eq!(displayed, id.0);
    }
}
