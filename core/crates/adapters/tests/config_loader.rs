//! Integration tests for [`trilithon_adapters::config_loader`].

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: integration test file; panics and expect are the correct failure mode

use std::fs;
use std::path::{Path, PathBuf};

use trilithon_adapters::config_loader::{ConfigError, load_config};
use trilithon_core::config::{EnvError, EnvProvider};

// ---------------------------------------------------------------------------
// Test double
// ---------------------------------------------------------------------------

/// An in-memory [`EnvProvider`] for deterministic tests.
struct MapEnvProvider {
    vars: Vec<(String, String)>,
}

impl MapEnvProvider {
    fn new(vars: impl IntoIterator<Item = (&'static str, &'static str)>) -> Self {
        Self {
            vars: vars
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        }
    }

    const fn empty() -> Self {
        Self { vars: Vec::new() }
    }
}

impl EnvProvider for MapEnvProvider {
    fn var(&self, key: &str) -> Result<String, EnvError> {
        self.vars
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| EnvError::NotPresent { key: key.into() })
    }

    fn vars_with_prefix(&self, prefix: &str) -> Vec<(String, String)> {
        self.vars
            .iter()
            .filter_map(|(k, v)| k.strip_prefix(prefix).map(|s| (s.to_string(), v.clone())))
            .collect()
    }
}

impl MapEnvProvider {
    fn with_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.vars.push((key.into(), value.into()));
        self
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Create a writable temp directory and return a [`MapEnvProvider`] that
/// injects it as `TRILITHON_STORAGE__DATA_DIR` plus any extra vars.
///
/// The caller must hold the returned [`tempfile::TempDir`] for the duration of
/// the test; dropping it removes the directory.
fn env_with_tempdir(
    extra_vars: impl IntoIterator<Item = (&'static str, &'static str)>,
) -> (tempfile::TempDir, MapEnvProvider) {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let data_dir = tmp.path().to_str().expect("UTF-8 path").to_owned();
    let provider =
        MapEnvProvider::new(extra_vars).with_var("TRILITHON_STORAGE__DATA_DIR", data_dir);
    (tmp, provider)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn happy_path_minimal() {
    let (_tmp, env) = env_with_tempdir([]);
    let cfg =
        load_config(&fixture("minimal.toml"), &env).expect("minimal.toml must load successfully");
    assert_eq!(cfg.server.bind.to_string(), "127.0.0.1:7878", "server.bind");
    assert_eq!(
        cfg.concurrency.rebase_token_ttl_minutes, 30,
        "rebase_token_ttl_minutes default"
    );
}

#[test]
fn missing_file() {
    let path = PathBuf::from("/nonexistent-trilithon-config.toml");
    let result = load_config(&path, &MapEnvProvider::empty());
    assert!(
        matches!(result, Err(ConfigError::Missing { .. })),
        "expected Missing, got {result:?}"
    );
}

#[test]
fn malformed_toml() {
    let result = load_config(&fixture("malformed.toml"), &MapEnvProvider::empty());
    match result {
        Err(ConfigError::MalformedToml { line, column, .. }) => {
            assert!(line >= 1, "line must be >= 1, got {line}");
            assert!(column >= 1, "column must be >= 1, got {column}");
        }
        other => panic!("expected MalformedToml, got {other:?}"),
    }
}

#[test]
fn env_override_applied() {
    let (_tmp, env) = env_with_tempdir([("TRILITHON_SERVER__BIND", "127.0.0.1:9090")]);
    let cfg = load_config(&fixture("minimal.toml"), &env).expect("env override must succeed");
    assert_eq!(
        cfg.server.bind.to_string(),
        "127.0.0.1:9090",
        "server.bind must be overridden"
    );
}

#[test]
fn rebase_ttl_boundary_low() {
    let env = MapEnvProvider::new([("TRILITHON_CONCURRENCY__REBASE_TOKEN_TTL_MINUTES", "4")]);
    let result = load_config(&fixture("minimal.toml"), &env);
    assert!(
        matches!(result, Err(ConfigError::RebaseTtlOutOfBounds { value: 4 })),
        "expected RebaseTtlOutOfBounds(4), got {result:?}"
    );
}

#[test]
fn rebase_ttl_boundary_low_inclusive() {
    let (_tmp, env) = env_with_tempdir([("TRILITHON_CONCURRENCY__REBASE_TOKEN_TTL_MINUTES", "5")]);
    let result = load_config(&fixture("minimal.toml"), &env);
    assert!(result.is_ok(), "TTL=5 must be accepted, got {result:?}");
}

#[test]
fn rebase_ttl_boundary_high_inclusive() {
    let (_tmp, env) =
        env_with_tempdir([("TRILITHON_CONCURRENCY__REBASE_TOKEN_TTL_MINUTES", "1440")]);
    let result = load_config(&fixture("minimal.toml"), &env);
    assert!(result.is_ok(), "TTL=1440 must be accepted, got {result:?}");
}

#[test]
fn rebase_ttl_boundary_high() {
    let env = MapEnvProvider::new([("TRILITHON_CONCURRENCY__REBASE_TOKEN_TTL_MINUTES", "1441")]);
    let result = load_config(&fixture("minimal.toml"), &env);
    assert!(
        matches!(
            result,
            Err(ConfigError::RebaseTtlOutOfBounds { value: 1441 })
        ),
        "expected RebaseTtlOutOfBounds(1441), got {result:?}"
    );
}

#[test]
fn bind_address_invalid() {
    let env = MapEnvProvider::new([("TRILITHON_SERVER__BIND", "not-a-valid-addr")]);
    let result = load_config(&fixture("minimal.toml"), &env);
    assert!(
        matches!(result, Err(ConfigError::BindAddressInvalid { .. })),
        "expected BindAddressInvalid, got {result:?}"
    );
}

#[cfg(unix)]
#[test]
fn data_dir_not_writable() {
    use std::os::unix::fs::PermissionsExt;

    // Skip when running as root (chmod 000 has no effect for root).
    if nix::unistd::getuid().is_root() {
        return;
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let data_dir = tmp.path().join("readonly");
    fs::create_dir(&data_dir).expect("create dir");

    // Remove all permissions.
    fs::set_permissions(&data_dir, fs::Permissions::from_mode(0o000)).expect("chmod 000");

    // Build a minimal config pointing at the unwritable dir.
    let toml_content = format!(
        r#"
[server]
bind = "127.0.0.1:7878"

[caddy.admin_endpoint]
transport = "unix"
path = "/run/caddy/admin.sock"

[storage]
data_dir = "{}"

[secrets.master_key_backend]
backend = "keychain"

[concurrency]

[tracing]

[bootstrap]
"#,
        data_dir.display()
    );

    let config_file = tmp.path().join("config.toml");
    fs::write(&config_file, toml_content).expect("write config");

    let result = load_config(&config_file, &MapEnvProvider::empty());

    // Restore permissions so tempdir cleanup can succeed.
    let _restore = fs::set_permissions(&data_dir, fs::Permissions::from_mode(0o755));

    assert!(
        matches!(result, Err(ConfigError::DataDirNotWritable { .. })),
        "expected DataDirNotWritable, got {result:?}"
    );
}
