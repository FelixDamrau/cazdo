mod art;
mod azure_devops;
mod cli;
mod commands;
mod config;
mod git;
mod pattern;
mod tui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands, ConfigAction};
use commands::WorkItemOutput;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Config { action }) => match action {
            ConfigAction::Init => commands::config_init()?,
            ConfigAction::Show => commands::config_show()?,
            ConfigAction::Verify => commands::config_verify().await?,
        },
        Some(Commands::Wi { id, long, json }) => {
            let output = match (long, json) {
                (_, true) => WorkItemOutput::Json,
                (true, false) => WorkItemOutput::Long,
                (false, false) => WorkItemOutput::Preview,
            };
            commands::show_work_item(id, output).await?;
        }
        None => {
            // Default: launch interactive TUI
            commands::interactive().await?;
        }
    }

    Ok(())
}
