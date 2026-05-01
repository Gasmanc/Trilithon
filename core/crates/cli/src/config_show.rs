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
pub fn run(config_path: &Path) -> ExitCode {
    match run_inner(config_path) {
        Ok(()) => ExitCode::CleanShutdown,
        Err(e) => {
            let mut stderr = std::io::stderr().lock();
            let _ = writeln!(stderr, "trilithon: config error: {e}");
            ExitCode::ConfigError
        }
    }
}

#[allow(clippy::print_stdout)]
// zd:phase-01 expires:2026-08-01 reason: config show is a user-facing display command; stdout is correct
fn run_inner(config_path: &Path) -> Result<(), anyhow::Error> {
    let env = trilithon_adapters::env_provider::StdEnvProvider;
    let config = trilithon_adapters::config_loader::load_config(config_path, &env)
        .map_err(anyhow::Error::from)?;
    let redacted = config.redacted();
    let rendered = toml::to_string_pretty(&redacted)?;
    println!("{rendered}");
    Ok(())
}
