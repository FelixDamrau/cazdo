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
    /// Show a bounded work item preview in the console
    Wi {
        /// Work item ID (if omitted, uses the current branch)
        id: Option<u32>,
        /// Show a longer, still bounded description preview
        #[arg(long)]
        long: bool,
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
            Some(Commands::Wi { id, long }) => {
                assert_eq!(id, None);
                assert!(!long);
            }
            _ => panic!("expected wi command without id"),
        }
    }

    #[test]
    fn parses_wi_with_id() {
        let cli = Cli::parse_from(["cazdo", "wi", "120"]);

        match cli.command {
            Some(Commands::Wi { id, long }) => {
                assert_eq!(id, Some(120));
                assert!(!long);
            }
            _ => panic!("expected wi command with id"),
        }
    }

    #[test]
    fn parses_wi_with_long_flag() {
        let cli = Cli::parse_from(["cazdo", "wi", "--long"]);

        match cli.command {
            Some(Commands::Wi { id, long }) => {
                assert_eq!(id, None);
                assert!(long);
            }
            _ => panic!("expected wi command with long flag"),
        }
    }

    #[test]
    fn parses_wi_with_id_and_long_flag() {
        let cli = Cli::parse_from(["cazdo", "wi", "120", "--long"]);

        match cli.command {
            Some(Commands::Wi { id, long }) => {
                assert_eq!(id, Some(120));
                assert!(long);
            }
            _ => panic!("expected wi command with id and long flag"),
        }
    }

    #[test]
    fn parses_wi_with_long_flag_before_id() {
        let cli = Cli::parse_from(["cazdo", "wi", "--long", "120"]);

        match cli.command {
            Some(Commands::Wi { id, long }) => {
                assert_eq!(id, Some(120));
                assert!(long);
            }
            _ => panic!("expected wi command with long flag before id"),
        }
    }
}
