//! . CLI entry point.

use anyhow::Result;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = ".", version, about)]
struct Cli {
    /// Increase verbosity (-v info, -vv debug, -vvv trace)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

#[tokio::main]
#[allow(clippy::disallowed_methods)]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);
    trilithon_adapters::boot()?;
    tracing::info!(version = %trilithon_adapters::core::version(), ". starting");
    Ok(())
}

fn init_tracing(verbosity: u8) {
    let level = match verbosity {
        0 => tracing::Level::WARN,
        1 => tracing::Level::INFO,
        2 => tracing::Level::DEBUG,
        _ => tracing::Level::TRACE,
    };
    tracing_subscriber::fmt().with_max_level(level).init();
}
