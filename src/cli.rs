use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cazdo")]
#[command(
    author,
    version,
    about = "Azure DevOps CLI tool for work item and branch management",
    before_help = crate::art::LOGO
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Configure cazdo settings
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Show a compact work item preview in the console
    Wi {
        /// Work item ID (if omitted, uses the current branch)
        id: Option<u32>,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Initialize config with default values (overwrites existing)
    Init,
    /// Show current configuration
    Show,
    /// Verify Azure DevOps organization URL and PAT access
    Verify,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wi_without_id() {
        let cli = Cli::parse_from(["cazdo", "wi"]);

        match cli.command {
            Some(Commands::Wi { id }) => assert_eq!(id, None),
            _ => panic!("expected wi command without id"),
        }
    }

    #[test]
    fn parses_wi_with_id() {
        let cli = Cli::parse_from(["cazdo", "wi", "120"]);

        match cli.command {
            Some(Commands::Wi { id }) => assert_eq!(id, Some(120)),
            _ => panic!("expected wi command with id"),
        }
    }
}
