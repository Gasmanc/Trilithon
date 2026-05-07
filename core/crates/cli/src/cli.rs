//! Clap-derive command surface for the Trilithon CLI.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// The top-level CLI arguments.
#[derive(Debug, Parser)]
#[command(
    name = "trilithon",
    about = "Trilithon daemon",
    disable_version_flag = true
)]
pub struct Cli {
    /// Path to the daemon configuration file.
    #[arg(long, default_value = "/etc/trilithon/config.toml", global = true)]
    pub config: PathBuf,

    /// Attempt to allow a non-loopback Caddy admin endpoint.
    ///
    /// This flag is OUT OF SCOPE FOR V1. The CLI will refuse and exit 2.
    #[arg(long, global = true)]
    pub allow_remote_admin: bool,

    /// Subcommand to execute.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the Trilithon daemon.
    Run {
        /// Take over ownership of the Caddy sentinel from another installation.
        ///
        /// By default, if Caddy's running config contains an ownership
        /// sentinel belonging to a different `installation_id`, Trilithon
        /// exits with code 3.  Pass `--takeover` to overwrite the sentinel
        /// and assume ownership, recording an audit event for Phase 6.
        #[arg(long, default_value_t = false)]
        takeover: bool,
    },
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

        // run (default — no takeover)
        let cli = Cli::try_parse_from(["trilithon", "run"])?;
        assert!(matches!(cli.command, Command::Run { takeover: false }));

        // run --takeover
        let cli = Cli::try_parse_from(["trilithon", "run", "--takeover"])?;
        assert!(matches!(cli.command, Command::Run { takeover: true }));

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
