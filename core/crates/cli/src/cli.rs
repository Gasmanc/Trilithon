//! Clap-derive command surface for the Trilithon CLI.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// The top-level CLI arguments.
#[derive(Debug, Parser)]
#[command(name = "trilithon", version, about = "Trilithon daemon")]
pub struct Cli {
    /// Path to the daemon configuration file.
    #[arg(long, default_value = "/etc/trilithon/config.toml", global = true)]
    pub config: PathBuf,

    /// Subcommand to execute.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the Trilithon daemon.
    Run,
    /// Configuration inspection subcommands.
    Config {
        /// Config action to perform.
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Print the build version line and exit.
    Version,
}

/// Actions available under `config`.
#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Resolve and print the configuration with secrets elided.
    Show,
}

#[cfg(test)]
mod tests {
    use super::{Cli, Command, ConfigAction};
    use clap::Parser;

    #[test]
    fn parses_three_subcommands() -> Result<(), clap::Error> {
        // version
        let cli = Cli::try_parse_from(["trilithon", "version"])?;
        assert!(matches!(cli.command, Command::Version));

        // run
        let cli = Cli::try_parse_from(["trilithon", "run"])?;
        assert!(matches!(cli.command, Command::Run));

        // config show
        let cli = Cli::try_parse_from(["trilithon", "config", "show"])?;
        assert!(matches!(
            cli.command,
            Command::Config {
                action: ConfigAction::Show
            }
        ));

        Ok(())
    }
}
