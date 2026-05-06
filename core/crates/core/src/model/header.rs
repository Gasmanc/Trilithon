//! Header manipulation rules for request and response processing.

use serde::{Deserialize, Serialize};

/// Header operations applied to requests and responses.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HeaderRules {
    /// Operations applied to the proxied request before forwarding upstream.
    pub request: Vec<HeaderOp>,
    /// Operations applied to the response before sending to the client.
    pub response: Vec<HeaderOp>,
}

/// A single header manipulation operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum HeaderOp {
    /// Set a header, replacing any existing value.
    Set {
        /// Header name.
        name: String,
        /// Header value to set.
        value: String,
    },
    /// Add a header value, preserving any existing values.
    Add {
        /// Header name.
        name: String,
        /// Header value to add.
        value: String,
    },
    /// Delete a header entirely.
    Delete {
        /// Header name to remove.
        name: String,
    },
}
