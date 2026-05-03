//! Primitive value types for the domain model.

use serde::{Deserialize, Serialize};

/// Unix timestamp in seconds.
pub type UnixSeconds = i64;

/// JSON Pointer as defined by RFC 6901.
///
/// Represents a reference to a specific value within a JSON document.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct JsonPointer(pub String);

impl JsonPointer {
    /// Create a root pointer (empty string).
    #[must_use]
    pub const fn root() -> Self {
        Self(String::new())
    }

    /// Append a segment to this pointer, escaping special characters per RFC 6901.
    ///
    /// In JSON Pointer:
    /// - `~` is escaped as `~0`
    /// - `/` is escaped as `~1`
    #[must_use]
    pub fn push(&self, segment: &str) -> Self {
        // Escape ~ first, then /
        let escaped = segment.replace('~', "~0").replace('/', "~1");

        // Append to the pointer with a leading slash
        let mut result = self.0.clone();
        result.push('/');
        result.push_str(&escaped);

        Self(result)
    }

    /// Get the pointer as a string reference.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for JsonPointer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for JsonPointer {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Caddy module identifier.
///
/// Represents a module name in Caddy's module ecosystem.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CaddyModule(pub String);

impl CaddyModule {
    /// Create a new Caddy module identifier.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Get the module name as a string reference.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CaddyModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for CaddyModule {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_pointer_root() {
        let ptr = JsonPointer::root();
        assert_eq!(ptr.0, "");
    }

    #[test]
    fn json_pointer_push_simple() {
        let ptr = JsonPointer::root().push("foo");
        assert_eq!(ptr.0, "/foo");
    }

    #[test]
    fn json_pointer_escapes_slash() {
        let ptr = JsonPointer::root().push("foo/bar");
        assert_eq!(ptr.0, "/foo~1bar");
    }

    #[test]
    fn json_pointer_escapes_tilde() {
        let ptr = JsonPointer::root().push("a~b");
        assert_eq!(ptr.0, "/a~0b");
    }

    #[test]
    fn json_pointer_escapes_both() {
        let ptr = JsonPointer::root().push("a~b/c");
        assert_eq!(ptr.0, "/a~0b~1c");
    }

    #[test]
    fn json_pointer_multiple_segments() {
        let ptr = JsonPointer::root().push("foo").push("bar").push("baz");
        assert_eq!(ptr.0, "/foo/bar/baz");
    }

    #[test]
    fn json_pointer_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let ptr = JsonPointer::root().push("test");
        let json = serde_json::to_string(&ptr)?;
        let deserialized: JsonPointer = serde_json::from_str(&json)?;
        assert_eq!(ptr, deserialized);
        Ok(())
    }

    #[test]
    fn caddy_module_new() {
        let module = CaddyModule::new("http.handlers.static_files");
        assert_eq!(module.0, "http.handlers.static_files");
    }

    #[test]
    fn caddy_module_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let module = CaddyModule::new("some.module");
        let json = serde_json::to_string(&module)?;
        let deserialized: CaddyModule = serde_json::from_str(&json)?;
        assert_eq!(module, deserialized);
        Ok(())
    }

    #[test]
    fn unix_seconds_type() {
        // Just verify the type compiles and works
        let _ = 1_609_459_200_i64 as UnixSeconds;
    }
}
