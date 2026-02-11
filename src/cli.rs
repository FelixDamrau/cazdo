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
