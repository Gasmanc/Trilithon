//! Upstream destination and probe types.

use serde::{Deserialize, Serialize};

use crate::model::identifiers::UpstreamId;

/// An upstream backend that a route can forward traffic to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct Upstream {
    /// Unique identifier for this upstream.
    pub id: UpstreamId,
    /// Where to send traffic.
    pub destination: UpstreamDestination,
    /// Health-check probe configuration.
    pub probe: UpstreamProbe,
    /// Relative weight for load balancing (higher = more traffic).
    pub weight: u16,
    /// Maximum size of a proxied request body in bytes.
    pub max_request_bytes: Option<u64>,
}

/// The network destination for an upstream.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpstreamDestination {
    /// A plain TCP address (host + port).
    TcpAddr {
        /// Hostname or IP address.
        host: String,
        /// TCP port number.
        port: u16,
    },
    /// A Unix domain socket.
    UnixSocket {
        /// Filesystem path to the socket file.
        path: String,
    },
    /// A Docker container reachable by container ID and port.
    DockerContainer {
        /// Docker container ID or name.
        container_id: String,
        /// Port exposed by the container.
        port: u16,
    },
}

/// Health-check probe configuration for an upstream.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpstreamProbe {
    /// TCP-level connectivity check.
    Tcp,
    /// HTTP check: GET `path` and assert the response status.
    Http {
        /// Path to request (e.g. `/healthz`).
        path: String,
        /// HTTP status code considered healthy.
        expected_status: u16,
    },
    /// No health checks; upstream is always considered healthy.
    Disabled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_tagged_serde() -> Result<(), Box<dyn std::error::Error>> {
        let dest = UpstreamDestination::TcpAddr {
            host: "127.0.0.1".into(),
            port: 8080,
        };
        let json = serde_json::to_string(&dest)?;
        let deserialized: UpstreamDestination = serde_json::from_str(&json)?;
        assert_eq!(dest, deserialized);
        Ok(())
    }
}
