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
        #[arg(long)]
        show: bool,
    },
}
