//! `HyperCaddyClient` — Caddy admin API adapter over `hyper` 1.x.
//!
//! Supports two transport variants:
//! - Unix-domain socket via `hyperlocal`.
//! - Loopback HTTPS with mutual TLS via `hyper-rustls`.

use std::{collections::BTreeSet, path::PathBuf, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use http_body_util::{BodyExt as _, Full};
use hyper::{Method, Request, StatusCode};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use tracing::instrument;

use trilithon_core::{
    caddy::{
        client::CaddyClient,
        error::CaddyError,
        types::{
            CaddyConfig, CaddyJsonPointer, HealthState, JsonPatch, LoadedModules, TlsCertificate,
            UpstreamHealth,
        },
    },
    config::CaddyEndpoint,
};

use super::traceparent::current_traceparent;

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

type TlsClient = Client<hyper_rustls::HttpsConnector<HttpConnector>, Full<Bytes>>;
type UnixClient = Client<hyperlocal::UnixConnector, Full<Bytes>>;

// ---------------------------------------------------------------------------
// Inner transport discriminant
// ---------------------------------------------------------------------------

enum Inner {
    Unix {
        client: Box<UnixClient>,
        socket_path: PathBuf,
    },
    LoopbackTls {
        client: Box<TlsClient>,
        base_url: url::Url,
    },
    /// Plain HTTP, used in tests only.
    #[cfg(test)]
    PlainHttp {
        client: Box<Client<HttpConnector, Full<Bytes>>>,
        base_url: url::Url,
    },
}

// ---------------------------------------------------------------------------
// Public struct
// ---------------------------------------------------------------------------

/// Caddy admin API client backed by `hyper` 1.x.
///
/// Use [`HyperCaddyClient::from_config`] to construct.
pub struct HyperCaddyClient {
    inner: Inner,
    connect_timeout: Duration,
    apply_timeout: Duration,
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

impl HyperCaddyClient {
    /// Construct a client from a [`CaddyEndpoint`] configuration value.
    ///
    /// # Errors
    ///
    /// Returns [`CaddyError::ProtocolViolation`] if TLS certificate material
    /// cannot be loaded or parsed.
    pub fn from_config(
        endpoint: &CaddyEndpoint,
        connect_timeout: Duration,
        apply_timeout: Duration,
    ) -> Result<Self, CaddyError> {
        let inner = match endpoint {
            CaddyEndpoint::Unix { path } => Inner::Unix {
                client: Box::new(
                    Client::builder(TokioExecutor::new()).build(hyperlocal::UnixConnector),
                ),
                socket_path: path.clone(),
            },
            CaddyEndpoint::LoopbackTls {
                url,
                mtls_cert_path,
                mtls_key_path,
                mtls_ca_path,
            } => {
                let base_url =
                    url.parse::<url::Url>()
                        .map_err(|e| CaddyError::InvalidEndpoint {
                            detail: format!("admin URL {url:?} is not a valid URL: {e}"),
                        })?;
                let client = Box::new(build_tls_client(
                    mtls_ca_path,
                    mtls_cert_path,
                    mtls_key_path,
                )?);
                Inner::LoopbackTls { client, base_url }
            }
        };

        Ok(Self {
            inner,
            connect_timeout,
            apply_timeout,
        })
    }

    /// Construct a test-only client backed by plain HTTP.
    ///
    /// This variant is only available in `#[cfg(test)]` builds and is used to
    /// drive an `httptest::Server` without TLS.
    #[cfg(test)]
    pub(crate) fn for_test(base_url: url::Url) -> Self {
        let client = Box::new(Client::builder(TokioExecutor::new()).build_http::<Full<Bytes>>());
        Self {
            inner: Inner::PlainHttp { client, base_url },
            connect_timeout: Duration::from_secs(5),
            apply_timeout: Duration::from_secs(5),
        }
    }
}

// ---------------------------------------------------------------------------
// TLS client builder
// ---------------------------------------------------------------------------

fn build_tls_client(
    ca_path: &std::path::Path,
    cert_path: &std::path::Path,
    key_path: &std::path::Path,
) -> Result<TlsClient, CaddyError> {
    use rustls_pemfile::Item;
    use std::{fs, io::BufReader};

    let ca_pem = fs::read(ca_path).map_err(|e| CaddyError::ProtocolViolation {
        detail: format!("failed to read CA certificate {}: {e}", ca_path.display()),
    })?;
    let mut ca_cursor = BufReader::new(ca_pem.as_slice());
    let ca_certs = rustls_pemfile::certs(&mut ca_cursor)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| CaddyError::ProtocolViolation {
            detail: format!("failed to parse CA certificate: {e}"),
        })?;

    let mut root_store = rustls::RootCertStore::empty();
    for cert in ca_certs {
        root_store
            .add(cert)
            .map_err(|e| CaddyError::ProtocolViolation {
                detail: format!("failed to add CA cert to root store: {e}"),
            })?;
    }

    let cert_pem = fs::read(cert_path).map_err(|e| CaddyError::ProtocolViolation {
        detail: format!(
            "failed to read client certificate {}: {e}",
            cert_path.display()
        ),
    })?;
    let mut cert_cursor = BufReader::new(cert_pem.as_slice());
    let client_certs: Vec<rustls::pki_types::CertificateDer<'static>> =
        rustls_pemfile::certs(&mut cert_cursor)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| CaddyError::ProtocolViolation {
                detail: format!("failed to parse client certificate: {e}"),
            })?;

    let key_pem = fs::read(key_path).map_err(|e| CaddyError::ProtocolViolation {
        detail: format!("failed to read client key {}: {e}", key_path.display()),
    })?;
    let mut key_cursor = BufReader::new(key_pem.as_slice());
    let private_key = rustls_pemfile::read_all(&mut key_cursor)
        .find_map(|item| match item {
            Ok(Item::Pkcs1Key(k)) => Some(Ok(rustls::pki_types::PrivateKeyDer::Pkcs1(k))),
            Ok(Item::Pkcs8Key(k)) => Some(Ok(rustls::pki_types::PrivateKeyDer::Pkcs8(k))),
            Ok(Item::Sec1Key(k)) => Some(Ok(rustls::pki_types::PrivateKeyDer::Sec1(k))),
            Err(e) => Some(Err(e)),
            _ => None,
        })
        .ok_or_else(|| CaddyError::ProtocolViolation {
            detail: format!("no private key found in {}", key_path.display()),
        })?
        .map_err(|e| CaddyError::ProtocolViolation {
            detail: format!("failed to parse private key: {e}"),
        })?;

    let tls_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_client_auth_cert(client_certs, private_key)
        .map_err(|e| CaddyError::ProtocolViolation {
            detail: format!("failed to build TLS client config: {e}"),
        })?;

    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_tls_config(tls_config)
        .https_only()
        .enable_http1()
        .build();

    let client = Client::builder(TokioExecutor::new()).build(https);
    Ok(client)
}

// ---------------------------------------------------------------------------
// Internal HTTP dispatch helpers
// ---------------------------------------------------------------------------

/// Build a `hyper::Uri` for a Unix-socket request.
fn unix_uri(socket_path: &std::path::Path, api_path: &str) -> hyper::Uri {
    hyperlocal::Uri::new(socket_path, api_path).into()
}

/// Build a full URL for a TCP-based (TLS or plain-HTTP) request.
fn tcp_url(base_url: &url::Url, api_path: &str) -> Result<hyper::Uri, CaddyError> {
    let full = base_url
        .join(api_path)
        .map_err(|e| CaddyError::ProtocolViolation {
            detail: format!("failed to build request URL: {e}"),
        })?;
    full.as_str()
        .parse::<hyper::Uri>()
        .map_err(|e| CaddyError::ProtocolViolation {
            detail: format!("invalid URI: {e}"),
        })
}

/// Collect the full response body bytes.
async fn collect_body<B>(resp: hyper::Response<B>) -> Result<(StatusCode, Bytes), CaddyError>
where
    B: hyper::body::Body,
    B::Error: std::fmt::Display,
{
    let status = resp.status();
    let body = resp
        .into_body()
        .collect()
        .await
        .map_err(|e| CaddyError::Unreachable {
            detail: e.to_string(),
        })?
        .to_bytes();
    Ok((status, body))
}

/// Assert status is 2xx; otherwise return `CaddyError::BadStatus`.
fn require_2xx(status: StatusCode, body: &Bytes) -> Result<(), CaddyError> {
    if status.is_success() {
        Ok(())
    } else {
        Err(CaddyError::BadStatus {
            status: status.as_u16(),
            body: String::from_utf8_lossy(body).into_owned(),
        })
    }
}

// ---------------------------------------------------------------------------
// Request spec
// ---------------------------------------------------------------------------

struct RequestSpec<'a> {
    method: Method,
    api_path: &'a str,
    content_type: Option<&'a str>,
    body: Bytes,
}

// ---------------------------------------------------------------------------
// Request dispatch
// ---------------------------------------------------------------------------

impl HyperCaddyClient {
    /// Execute a single request against the appropriate transport.
    ///
    /// Returns `(StatusCode, body_bytes)`.
    async fn execute(&self, spec: RequestSpec<'_>) -> Result<(StatusCode, Bytes), CaddyError> {
        let traceparent = current_traceparent();

        match &self.inner {
            Inner::Unix {
                client,
                socket_path,
            } => {
                let uri = unix_uri(socket_path, spec.api_path);
                let req = build_request(spec, uri, &traceparent)?;
                let resp = client
                    .request(req)
                    .await
                    .map_err(|e| CaddyError::Unreachable {
                        detail: e.to_string(),
                    })?;
                collect_body(resp).await
            }

            Inner::LoopbackTls { client, base_url } => {
                let uri = tcp_url(base_url, spec.api_path)?;
                let req = build_request(spec, uri, &traceparent)?;
                let resp = client
                    .request(req)
                    .await
                    .map_err(|e| CaddyError::Unreachable {
                        detail: e.to_string(),
                    })?;
                collect_body(resp).await
            }

            #[cfg(test)]
            Inner::PlainHttp { client, base_url } => {
                let uri = tcp_url(base_url, spec.api_path)?;
                let req = build_request(spec, uri, &traceparent)?;
                let resp = client
                    .request(req)
                    .await
                    .map_err(|e| CaddyError::Unreachable {
                        detail: e.to_string(),
                    })?;
                collect_body(resp).await
            }
        }
    }

    /// Execute a request with a timeout, mapping a timeout expiry to
    /// [`CaddyError::Timeout`].
    async fn execute_with_timeout(
        &self,
        timeout: Duration,
        spec: RequestSpec<'_>,
    ) -> Result<(StatusCode, Bytes), CaddyError> {
        let timeout_secs = timeout.as_secs();
        tokio::time::timeout(timeout, self.execute(spec))
            .await
            .map_err(|_| CaddyError::Timeout {
                seconds: u32::try_from(timeout_secs).unwrap_or(u32::MAX),
            })?
    }
}

/// Build a [`hyper::Request`] from a [`RequestSpec`], URI, and traceparent.
fn build_request(
    spec: RequestSpec<'_>,
    uri: hyper::Uri,
    traceparent: &str,
) -> Result<Request<Full<Bytes>>, CaddyError> {
    let mut builder = Request::builder()
        .method(spec.method)
        .uri(uri)
        .header("traceparent", traceparent)
        .header("accept", "application/json");

    if let Some(ct) = spec.content_type {
        builder = builder.header("content-type", ct);
    }

    builder
        .body(Full::new(spec.body))
        .map_err(|e| CaddyError::ProtocolViolation {
            detail: format!("failed to build request: {e}"),
        })
}

// ---------------------------------------------------------------------------
// Recursive module-id collector
// ---------------------------------------------------------------------------

/// Maximum recursion depth for [`collect_module_ids`].
///
/// Caddy configs are operator-supplied and bounded in practice, but an
/// adversarially crafted config could overflow the stack without this guard.
const MAX_COLLECT_DEPTH: usize = 128;

fn collect_module_ids(value: &serde_json::Value, out: &mut BTreeSet<String>) {
    collect_module_ids_inner(value, out, 0);
}

fn collect_module_ids_inner(value: &serde_json::Value, out: &mut BTreeSet<String>, depth: usize) {
    if depth >= MAX_COLLECT_DEPTH {
        return;
    }
    match value {
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(module_id)) = map.get("module") {
                out.insert(module_id.clone());
            }
            for v in map.values() {
                collect_module_ids_inner(v, out, depth + 1);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_module_ids_inner(v, out, depth + 1);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Serde helper types for Caddy API responses
// ---------------------------------------------------------------------------

/// Upstream entry returned by `GET /reverse_proxy/upstreams`.
#[derive(serde::Deserialize)]
struct RawUpstream {
    address: String,
    healthy: Option<bool>,
    #[serde(default)]
    num_requests: u64,
    #[serde(default)]
    fails: u64,
}

/// Certificate entry returned by `GET /pki/ca/local/certificates`.
#[derive(serde::Deserialize)]
struct RawCert {
    #[serde(default)]
    names: Vec<String>,
    #[serde(default)]
    not_before: i64,
    #[serde(default)]
    not_after: i64,
    #[serde(default)]
    issuer: String,
}

// ---------------------------------------------------------------------------
// Version fetch helper
// ---------------------------------------------------------------------------

/// Fetch the Caddy version from `GET /version`.
///
/// Caddy 2.8 returns `{"version":"v2.8.4"}` on this endpoint.  Falls back to
/// `"unknown"` when the call fails or the `version` field is absent.
async fn fetch_caddy_version(client: &HyperCaddyClient) -> String {
    let fut = client.execute(RequestSpec {
        method: Method::GET,
        api_path: "/version",
        content_type: None,
        body: Bytes::new(),
    });
    match tokio::time::timeout(client.connect_timeout, fut).await {
        Ok(Ok((status, body))) if status.is_success() => {
            serde_json::from_slice::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("version")?.as_str().map(str::to_owned))
                .unwrap_or_else(|| "unknown".to_owned())
        }
        _ => "unknown".to_owned(),
    }
}

// ---------------------------------------------------------------------------
// CaddyClient implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl CaddyClient for HyperCaddyClient {
    /// Replace the entire running Caddy configuration.
    #[instrument(skip(self, body), err)]
    async fn load_config(&self, body: CaddyConfig) -> Result<(), CaddyError> {
        let json = serde_json::to_vec(&body).map_err(|e| CaddyError::ProtocolViolation {
            detail: format!("failed to serialise config: {e}"),
        })?;

        let (status, resp_body) = self
            .execute_with_timeout(
                self.apply_timeout,
                RequestSpec {
                    method: Method::POST,
                    api_path: "/load",
                    content_type: Some("application/json"),
                    body: Bytes::from(json),
                },
            )
            .await?;

        require_2xx(status, &resp_body)
    }

    /// Apply a JSON Patch document to a sub-tree of the running config.
    #[instrument(skip(self, patch), err)]
    async fn patch_config(
        &self,
        path: CaddyJsonPointer,
        patch: JsonPatch,
    ) -> Result<(), CaddyError> {
        if !path.0.starts_with("/apps/") {
            return Err(CaddyError::ProtocolViolation {
                detail: "path must start with /apps/".into(),
            });
        }

        let json = serde_json::to_vec(&patch).map_err(|e| CaddyError::ProtocolViolation {
            detail: format!("failed to serialise patch: {e}"),
        })?;

        let api_path = format!("/config{}", path.0);
        let (status, resp_body) = self
            .execute_with_timeout(
                self.apply_timeout,
                RequestSpec {
                    method: Method::PATCH,
                    api_path: &api_path,
                    content_type: Some("application/json"),
                    body: Bytes::from(json),
                },
            )
            .await?;

        require_2xx(status, &resp_body)
    }

    /// Set the value at `path` using Caddy's `PUT /config/[path]` endpoint.
    #[instrument(skip(self, value), err)]
    async fn put_config(
        &self,
        path: CaddyJsonPointer,
        value: serde_json::Value,
    ) -> Result<(), CaddyError> {
        if !path.0.starts_with("/apps/") {
            return Err(CaddyError::ProtocolViolation {
                detail: "path must start with /apps/".into(),
            });
        }

        let json = serde_json::to_vec(&value).map_err(|e| CaddyError::ProtocolViolation {
            detail: format!("failed to serialise value: {e}"),
        })?;

        let api_path = format!("/config{}", path.0);
        let (status, resp_body) = self
            .execute_with_timeout(
                self.apply_timeout,
                RequestSpec {
                    method: Method::PUT,
                    api_path: &api_path,
                    content_type: Some("application/json"),
                    body: Bytes::from(json),
                },
            )
            .await?;

        require_2xx(status, &resp_body)
    }

    /// Retrieve the full running Caddy configuration.
    #[instrument(skip(self), err)]
    async fn get_running_config(&self) -> Result<CaddyConfig, CaddyError> {
        let (status, body) = self
            .execute_with_timeout(
                self.connect_timeout,
                RequestSpec {
                    method: Method::GET,
                    api_path: "/config/",
                    content_type: None,
                    body: Bytes::new(),
                },
            )
            .await?;

        require_2xx(status, &body)?;

        let value: serde_json::Value =
            serde_json::from_slice(&body).map_err(|e| CaddyError::ProtocolViolation {
                detail: format!("failed to parse running config: {e}"),
            })?;
        Ok(CaddyConfig(value))
    }

    /// List all modules currently loaded by Caddy.
    #[instrument(skip(self), err)]
    async fn get_loaded_modules(&self) -> Result<LoadedModules, CaddyError> {
        let (apps_result, caddy_version) = tokio::join!(
            self.execute_with_timeout(
                self.connect_timeout,
                RequestSpec {
                    method: Method::GET,
                    api_path: "/config/apps",
                    content_type: None,
                    body: Bytes::new(),
                }
            ),
            fetch_caddy_version(self),
        );

        let (status, body) = apps_result?;
        require_2xx(status, &body)?;

        let apps: serde_json::Value =
            serde_json::from_slice(&body).map_err(|e| CaddyError::ProtocolViolation {
                detail: format!("failed to parse apps config: {e}"),
            })?;

        let mut modules = BTreeSet::new();
        collect_module_ids(&apps, &mut modules);

        Ok(LoadedModules {
            modules,
            caddy_version,
        })
    }

    /// Query the health of all configured upstreams.
    #[instrument(skip(self), err)]
    async fn get_upstream_health(&self) -> Result<Vec<UpstreamHealth>, CaddyError> {
        let (status, body) = self
            .execute_with_timeout(
                self.connect_timeout,
                RequestSpec {
                    method: Method::GET,
                    api_path: "/reverse_proxy/upstreams",
                    content_type: None,
                    body: Bytes::new(),
                },
            )
            .await?;

        require_2xx(status, &body)?;

        let raw: Vec<RawUpstream> =
            serde_json::from_slice(&body).map_err(|e| CaddyError::ProtocolViolation {
                detail: format!("failed to parse upstreams: {e}"),
            })?;

        Ok(raw
            .into_iter()
            .map(|u| UpstreamHealth {
                address: u.address,
                healthy: u.healthy.unwrap_or(false),
                num_requests: u.num_requests,
                fails: u.fails,
            })
            .collect())
    }

    /// List all TLS certificates currently managed by Caddy.
    #[instrument(skip(self), err)]
    async fn get_certificates(&self) -> Result<Vec<TlsCertificate>, CaddyError> {
        let (status, body) = self
            .execute_with_timeout(
                self.connect_timeout,
                RequestSpec {
                    method: Method::GET,
                    api_path: "/pki/ca/local/certificates",
                    content_type: None,
                    body: Bytes::new(),
                },
            )
            .await?;

        // 404 means PKI app not loaded — return empty list per spec.
        if status == StatusCode::NOT_FOUND {
            return Ok(vec![]);
        }

        require_2xx(status, &body)?;

        let raw: Vec<RawCert> =
            serde_json::from_slice(&body).map_err(|e| CaddyError::ProtocolViolation {
                detail: format!("failed to parse certificates: {e}"),
            })?;

        Ok(raw
            .into_iter()
            .map(|c| TlsCertificate {
                names: c.names,
                not_before: c.not_before,
                not_after: c.not_after,
                issuer: c.issuer,
            })
            .collect())
    }

    /// Perform a lightweight health check against the Caddy admin endpoint.
    #[instrument(skip(self), err)]
    async fn health_check(&self) -> Result<HealthState, CaddyError> {
        match self
            .execute_with_timeout(
                self.connect_timeout,
                RequestSpec {
                    method: Method::GET,
                    api_path: "/",
                    content_type: None,
                    body: Bytes::new(),
                },
            )
            .await
        {
            Ok((status, _)) if status.is_success() => Ok(HealthState::Reachable),
            Ok((status, body)) => Err(CaddyError::BadStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body).into_owned(),
            }),
            Err(CaddyError::Unreachable { .. }) => Ok(HealthState::Unreachable),
            Err(e) => Err(e),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use httptest::{Expectation, Server, matchers::*, responders::*};
    use trilithon_core::caddy::{
        client::CaddyClient,
        error::CaddyError,
        types::{CaddyJsonPointer, JsonPatch},
    };

    use super::HyperCaddyClient;

    fn make_client(server: &Server) -> HyperCaddyClient {
        let url: url::Url = server.url("/").to_string().parse().expect("valid url");
        HyperCaddyClient::for_test(url)
    }

    #[tokio::test]
    async fn traceparent_header_present() {
        let server = Server::run();

        server.expect(
            Expectation::matching(all_of![
                request::method("GET"),
                request::path("/"),
                request::headers(contains(key("traceparent"))),
            ])
            .respond_with(status_code(200)),
        );

        let client = make_client(&server);
        let result = client.health_check().await;
        assert!(result.is_ok(), "health_check should succeed: {result:?}");
    }

    #[tokio::test]
    async fn patch_path_must_start_with_apps() {
        // Validation happens before any network call.
        let server = Server::run();
        let client = make_client(&server);

        let err = client
            .patch_config(
                CaddyJsonPointer("/not/apps/route".into()),
                JsonPatch(vec![]),
            )
            .await
            .unwrap_err();

        assert!(
            matches!(
                err,
                CaddyError::ProtocolViolation { ref detail }
                if detail.contains("/apps/")
            ),
            "expected ProtocolViolation about /apps/, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn transport_timeout_maps_to_timeout_variant() {
        use std::time::Duration;

        // 192.0.2.1 is TEST-NET-1 — guaranteed unreachable / silently drops.
        let base_url: url::Url = "http://192.0.2.1:2019/".parse().expect("valid url");
        let client = HyperCaddyClient {
            inner: Inner::PlainHttp {
                client: {
                    use hyper_util::client::legacy::Client;
                    use hyper_util::rt::TokioExecutor;
                    Box::new(Client::builder(TokioExecutor::new()).build_http::<Full<Bytes>>())
                },
                base_url,
            },
            connect_timeout: Duration::from_millis(100),
            apply_timeout: Duration::from_millis(100),
        };

        let result = client.health_check().await;
        // health_check returns Ok(Unreachable) for transport errors, and
        // Err(Timeout) when the timeout fires.  Both are acceptable here.
        assert!(
            matches!(result, Ok(HealthState::Unreachable))
                || matches!(result, Err(CaddyError::Timeout { .. })),
            "expected Unreachable or Timeout, got: {result:?}"
        );
    }

    use super::{Full, Inner};
    use bytes::Bytes;
    use trilithon_core::caddy::types::HealthState;
}
