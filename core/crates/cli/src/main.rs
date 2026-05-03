//! Trilithon CLI entry point.
//!
//! `unreachable_pub` is suppressed for the binary crate: items are `pub`
//! within private modules for clarity, but can never be exported externally.
// zd:phase-01 expires:2026-08-01 reason: binary crate pub-in-private-module pattern is intentional
#![allow(unreachable_pub)]

use std::io::Write as _;

use clap::Parser;

mod cli;
mod config_show;
mod exit;
mod observability;
mod run;
mod shutdown;

use cli::{Cli, Command, ConfigAction};

fn main() -> std::process::ExitCode {
    // Pre-tracing line — must appear before subscriber installation.
    {
        let mut stderr = std::io::stderr().lock();
        // Ignore write/flush errors: if stderr is broken there is nothing
        // sensible to do before the subscriber is up.
        let _ = stderr.write_all(b"trilithon: starting (pre-tracing)\n");
        let _ = stderr.flush();
    }

    let cli = Cli::parse();

    // Initialise the tracing subscriber with a best-effort default config.
    // If the subscriber is already installed (e.g. in tests) we continue
    // without failing; any other error is logged to stderr and we carry on
    // with a no-op subscriber.
    let tracing_config = trilithon_core::config::TracingConfig {
        log_filter: "info,trilithon=info".into(),
        format: trilithon_core::config::LogFormat::Pretty,
    };
    if let Err(e) = observability::init(&tracing_config) {
        match e {
            observability::ObsError::AlreadyInstalled => {}
            observability::ObsError::BadFilter { .. } => {
                let mut stderr = std::io::stderr().lock();
                let _ = writeln!(stderr, "trilithon: tracing init warning: {e}");
            }
        }
    }

    let code = dispatch(cli);
    exit::to_process_exit(code)
}

fn dispatch(cli: Cli) -> trilithon_core::exit::ExitCode {
    let Cli {
        config,
        allow_remote_admin,
        command,
    } = cli;

    if allow_remote_admin {
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(
            stderr,
            "--allow-remote-admin is OUT OF SCOPE FOR V1; remove the flag and rerun."
        );
        return trilithon_core::exit::ExitCode::ConfigError;
    }

    match command {
        Command::Version => print_version(),
        Command::Run { takeover } => run_daemon(&config, takeover),
        Command::Config {
            action: ConfigAction::Show,
        } => config_show::run(&config),
    }
}

/// Spin up the Tokio runtime and run the daemon until a signal arrives.
fn run_daemon(config_path: &std::path::Path, takeover: bool) -> trilithon_core::exit::ExitCode {
    // Load and validate config before starting the runtime so that config
    // errors produce exit code 2 without spinning up Tokio.
    let env = trilithon_adapters::env_provider::StdEnvProvider;
    let config = match trilithon_adapters::config_loader::load_config(config_path, &env) {
        Ok(cfg) => cfg,
        Err(e) => {
            let mut stderr = std::io::stderr().lock();
            let _ = writeln!(stderr, "trilithon: {e}");
            return trilithon_core::exit::ExitCode::ConfigError;
        }
    };

    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            let mut stderr = std::io::stderr().lock();
            let _ = writeln!(stderr, "trilithon: failed to build Tokio runtime: {e}");
            return trilithon_core::exit::ExitCode::StartupPreconditionFailure;
        }
    };

    match rt.block_on(run::run_with_shutdown(config, takeover)) {
        Ok(code) => code,
        Err(e) => {
            tracing::error!(error = %e, "daemon.fatal");
            trilithon_core::exit::ExitCode::StartupPreconditionFailure
        }
    }
}

#[allow(clippy::print_stdout)]
// zd:phase-01 expires:2026-08-01 reason: version output must go to stdout per CLI convention
fn print_version() -> trilithon_core::exit::ExitCode {
    println!(
        "trilithon {} ({}) {}",
        env!("CARGO_PKG_VERSION"),
        env!("TRILITHON_GIT_SHORT_HASH"),
        env!("TRILITHON_RUSTC_VERSION"),
    );
    trilithon_core::exit::ExitCode::CleanShutdown
}
