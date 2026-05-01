//! Trilithon CLI entry point.
//!
//! `unreachable_pub` is suppressed for the binary crate: items are `pub`
//! within private modules for clarity, but can never be exported externally.
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

    tracing::info!("daemon.started");

    let code = dispatch(cli);
    exit::to_process_exit(code)
}

fn dispatch(cli: Cli) -> trilithon_core::exit::ExitCode {
    let Cli { config: _, command } = cli;
    match command {
        Command::Version => print_version(),
        Command::Run => run_daemon(),
        Command::Config {
            action: ConfigAction::Show,
        } => config_show::placeholder(),
    }
}

/// Spin up the Tokio runtime and run the daemon until a signal arrives.
fn run_daemon() -> trilithon_core::exit::ExitCode {
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

    match rt.block_on(run::run_with_shutdown()) {
        Ok(code) => code,
        Err(e) => {
            tracing::error!(error = %e, "daemon.fatal");
            trilithon_core::exit::ExitCode::StartupPreconditionFailure
        }
    }
}

#[allow(clippy::print_stdout)]
fn print_version() -> trilithon_core::exit::ExitCode {
    println!(
        "trilithon {} ({}) {}",
        env!("CARGO_PKG_VERSION"),
        env!("TRILITHON_GIT_SHORT_HASH"),
        env!("TRILITHON_RUSTC_VERSION"),
    );
    trilithon_core::exit::ExitCode::CleanShutdown
}
