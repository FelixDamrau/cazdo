mod art;
mod azure_devops;
mod cli;
mod commands;
mod config;
mod git;
mod tui;
mod ui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::WiInfo) => {
            commands::wi_info().await?;
        }
        Some(Commands::Config { show }) => {
            if show {
                commands::config_show()?;
            } else {
                commands::config_interactive()?;
            }
        }
        None => {
            // Default: launch interactive TUI
            commands::interactive().await?;
        }
    }

    Ok(())
}
