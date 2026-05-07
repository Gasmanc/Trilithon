//! Loopback-only policy enforcement for the Caddy admin endpoint.
//!
//! Any [`CaddyEndpoint`] that resolves to a host outside the loopback set
//! (`127.0.0.1`, `::1`) or that is not a Unix socket is rejected with
//! [`EndpointPolicyError::NonLoopback`].
//!
//! `localhost` is explicitly rejected: DNS resolution of `localhost` is
//! platform-dependent and can resolve to non-loopback addresses in
//! containerised environments. Use an explicit loopback IP instead.
//!
//! ADR-0011 mandates this restriction for V1.

use std::net::Ipv6Addr;

use trilithon_core::config::CaddyEndpoint;

/// Errors returned by [`validate_loopback_only`].
#[derive(Debug, thiserror::Error)]
pub enum EndpointPolicyError {
    /// The admin endpoint resolves to a non-loopback host, which is forbidden
    /// in V1 (ADR-0011).
    #[error("non-loopback admin endpoint host {host} is forbidden in V1 (ADR-0011)")]
    NonLoopback {
        /// The rejected host string.
        host: String,
    },
}

/// Validate that `endpoint` refers only to the loopback interface or a Unix
/// socket.
///
/// `localhost` is rejected even though it typically resolves to a loopback
/// address, because DNS resolution is platform-dependent. Use `127.0.0.1`
/// or `[::1]` directly.
///
/// # Errors
///
/// Returns [`EndpointPolicyError::NonLoopback`] if the endpoint's host is
/// not one of `127.0.0.1` or `::1`.
pub fn validate_loopback_only(endpoint: &CaddyEndpoint) -> Result<(), EndpointPolicyError> {
    match endpoint {
        // Unix-domain sockets are local by definition.
        CaddyEndpoint::Unix { .. } => Ok(()),
        CaddyEndpoint::LoopbackTls { url, .. } => {
            let parsed = url
                .parse::<url::Url>()
                .map_err(|_| EndpointPolicyError::NonLoopback { host: url.clone() })?;
            match parsed.host() {
                Some(url::Host::Ipv4(addr)) if addr.is_loopback() => Ok(()),
                Some(url::Host::Ipv6(addr)) if addr == Ipv6Addr::LOCALHOST => Ok(()),
                Some(host) => Err(EndpointPolicyError::NonLoopback {
                    host: host.to_string(),
                }),
                None => Err(EndpointPolicyError::NonLoopback {
                    host: String::new(),
                }),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn loopback_tls(url: &str) -> CaddyEndpoint {
        CaddyEndpoint::LoopbackTls {
            url: url.to_owned(),
            mtls_cert_path: PathBuf::from("/etc/certs/client.crt"),
            mtls_key_path: PathBuf::from("/etc/certs/client.key"),
            mtls_ca_path: PathBuf::from("/etc/certs/ca.crt"),
        }
    }

    #[test]
    fn unix_ok() {
        let ep = CaddyEndpoint::Unix {
            path: PathBuf::from("/run/caddy/admin.sock"),
        };
        assert!(validate_loopback_only(&ep).is_ok());
    }

    #[test]
    fn loopback_v4_ok() {
        assert!(validate_loopback_only(&loopback_tls("https://127.0.0.1:2019")).is_ok());
    }

    #[test]
    fn loopback_v6_ok() {
        assert!(validate_loopback_only(&loopback_tls("https://[::1]:2019")).is_ok());
    }

    #[test]
    fn loopback_localhost_rejected() {
        // localhost DNS resolution is platform-dependent; require explicit IPs.
        let ep = loopback_tls("https://localhost:2019");
        assert!(matches!(
            validate_loopback_only(&ep).unwrap_err(),
            EndpointPolicyError::NonLoopback { ref host } if host == "localhost"
        ));
    }

    #[test]
    fn external_host_rejected() {
        let ep = loopback_tls("https://192.168.1.10:2019");
        let err = validate_loopback_only(&ep).unwrap_err();
        assert!(
            matches!(err, EndpointPolicyError::NonLoopback { ref host } if host == "192.168.1.10")
        );
    }
}
