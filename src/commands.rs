use crate::azure_devops::AzureDevOpsClient;
use crate::config::Config;
use crate::git::GitRepo;
use crate::tui::{App, BranchInfo, run_app};
use crate::ui;
use anyhow::{Context, Result, bail};

pub async fn interactive() -> Result<()> {
    let repo = GitRepo::open_current_dir().context("Failed to open git repository")?;
    let current_branch = repo
        .current_branch()
        .context("Failed to get current branch")?;
    let branches = repo.list_branches().context("Failed to list branches")?;

    let branch_infos: Vec<BranchInfo> = branches
        .into_iter()
        .map(|name| {
            let wi_id = repo.extract_work_item_number(&name);
            let is_current = name == current_branch;
            BranchInfo {
                name,
                work_item_id: wi_id,
                is_current,
            }
        })
        .collect();

    if branch_infos.is_empty() {
        bail!("No branches found in repository");
    }

    let app = App::new(branch_infos);
    run_app(app, repo).await?;

    Ok(())
}

pub async fn wi_info() -> Result<()> {
    let config = Config::load().context("Failed to load configuration")?;

    let repo = GitRepo::open_current_dir().context("Failed to open git repository")?;
    let branch = repo
        .current_branch()
        .context("Failed to get current branch")?;

    let wi_number = match repo.extract_work_item_number(&branch) {
        Some(n) => n,
        None => {
            ui::render_branch_only(&branch)?;
            return Ok(());
        }
    };

    let client = AzureDevOpsClient::new(&config)?;
    let work_item = match client.get_work_item(wi_number).await {
        Ok(wi) => wi,
        Err(e) => {
            ui::render_error(&format!("Failed to fetch work item #{}: {}", wi_number, e))?;
            return Ok(());
        }
    };

    ui::render_work_item(&work_item, &branch)?;

    Ok(())
}

pub fn config_show() -> Result<()> {
    let config = Config::load().context("Failed to load configuration")?;
    println!("Configuration file: {}", Config::config_path()?.display());
    println!();
    println!(
        "Azure DevOps Organization URL: {}",
        config.azure_devops.organization_url
    );
    println!(
        "PAT: {}",
        if std::env::var("CAZDO_PAT").is_ok() {
            "(set via CAZDO_PAT)"
        } else {
            "(not set)"
        }
    );
    Ok(())
}

pub fn config_interactive() -> Result<()> {
    use std::io::{self, Write};

    let config_path = Config::config_path()?;

    println!("cazdo configuration");
    println!("===================");
    println!();
    println!("Config file will be saved to: {}", config_path.display());
    println!();

    print!("Azure DevOps Organization URL (e.g., https://dev.azure.com/myorg): ");
    io::stdout().flush()?;

    let mut org_url = String::new();
    io::stdin().read_line(&mut org_url)?;
    let org_url = org_url.trim();

    if org_url.is_empty() {
        bail!("Organization URL cannot be empty");
    }

    let config = Config::new(org_url.to_string());
    config.save()?;

    println!();
    println!("Configuration saved!");
    println!();
    println!("Don't forget to set your PAT:");
    println!("  export CAZDO_PAT=\"your-personal-access-token\"");

    Ok(())  
}
