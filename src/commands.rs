use crate::config::Config;
use crate::git::{GitRepo, extract_work_item_number};
use crate::pattern::is_protected;
use crate::tui::{App, BranchInfo, run_app};
use anyhow::{Context, Result, bail};

pub async fn interactive() -> Result<()> {
    let repo = GitRepo::open_current_dir().context("Failed to open git repository")?;
    let current_branch = repo
        .current_branch()
        .context("Failed to get current branch")?;
    let branches = repo.list_branches().context("Failed to list branches")?;

    // Load protected patterns from config (with fallback to defaults)
    let protected_patterns = Config::load()
        .map(|c| c.branches.protected_patterns())
        .unwrap_or_else(|_| {
            crate::config::DEFAULT_PROTECTED_PATTERNS
                .iter()
                .map(|s| s.to_string())
                .collect()
        });

    let branch_infos: Vec<BranchInfo> = branches
        .into_iter()
        .map(|name| {
            let is_current = name == current_branch;
            let is_protected_branch = is_protected(&name, &protected_patterns);
            // Don't extract work item from protected branches (they're version names, not work items)
            let wi_id = if is_protected_branch {
                None
            } else {
                extract_work_item_number(&name)
            };
            BranchInfo {
                name,
                work_item_id: wi_id,
                is_current,
                is_protected: is_protected_branch,
            }
        })
        .collect();

    if branch_infos.is_empty() {
        bail!("No branches found in repository");
    }

    let app = App::new(branch_infos, protected_patterns);
    run_app(app, repo).await?;

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
