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
    let config_path = Config::config_path()?;

    if !config_path.exists() {
        bail!(
            "Configuration file not found at {}\n\nRun 'cazdo config init' to create a default configuration.",
            config_path.display()
        );
    }

    println!("# {}", config_path.display());
    println!();

    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;
    print!("{}", content);

    println!();
    println!(
        "# PAT: {}",
        if std::env::var("CAZDO_PAT").is_ok() {
            "(set via CAZDO_PAT)"
        } else {
            "(not set)"
        }
    );
    Ok(())
}

pub fn config_init() -> Result<()> {
    use std::io::{self, Write};

    let config_path = Config::config_path()?;

    if config_path.exists() {
        print!(
            "Config already exists at {}. Overwrite? [y/N] ",
            config_path.display()
        );
        io::stdout().flush()?;

        let mut response = String::new();
        io::stdin().read_line(&mut response)?;
        let response = response.trim().to_lowercase();

        if response != "y" && response != "yes" {
            println!("Aborted.");
            return Ok(());
        }
    }

    let config = Config::default();
    config.save()?;

    println!("Configuration initialized with defaults!");
    println!();
    println!("Config location: {}", config_path.display());
    println!();
    println!("Edit the config file to set:");
    println!("  - Azure DevOps organization URL");
    println!("  - Protected branch patterns");
    if !std::env::var("CAZDO_PAT").is_ok() {
        println!();
        println!("Don't forget to set your PAT:");
        println!("  export CAZDO_PAT=\"your-personal-access-token\"");
    }

    Ok(())
}
