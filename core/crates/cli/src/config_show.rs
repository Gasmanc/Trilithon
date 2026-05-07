//! Implementation of the `config show` subcommand.

use std::io::Write as _;
use std::path::Path;

use trilithon_core::exit::ExitCode;

/// Run the `config show` subcommand.
///
/// Resolves the configuration via `load_config`, renders `DaemonConfig::redacted()`
/// as TOML, and prints the result to stdout.
///
/// On error, writes to stderr and returns the appropriate exit code.
#[allow(clippy::print_stdout)]
// zd:phase-01 expires:2026-08-01 reason: config show is a user-facing display command; stdout is correct
pub fn run(config_path: &Path) -> ExitCode {
    let env = trilithon_adapters::env_provider::StdEnvProvider;
    let config = match trilithon_adapters::config_loader::load_config(config_path, &env) {
        Ok(cfg) => cfg,
        Err(e) => {
            // Tracing subscriber not yet installed — write directly to stderr.
            let mut stderr = std::io::stderr().lock();
            let _ = writeln!(stderr, "trilithon: config error: {e}");
            return ExitCode::ConfigError;
        }
    };

    // Install subscriber with values from loaded config; ignore AlreadyInstalled
    // (tests) and BadFilter (don't break config show due to a tracing misconfiguration).
    let _ = crate::observability::init(&config.tracing);

    let redacted = config.redacted();
    match toml::to_string_pretty(&redacted) {
        Ok(rendered) => {
            println!("{rendered}");
            ExitCode::CleanShutdown
        }
        Err(e) => {
            tracing::error!(error = %e, "config.show.serialization-failed");
            ExitCode::ConfigError
        }
    }
}
