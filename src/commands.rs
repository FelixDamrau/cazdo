use crate::azure_devops::AzureDevOpsClient;
use crate::config::{Config, PatSource};
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

    let config = Config::load()?;
    let pat_status = match config.pat_source() {
        PatSource::Env => "env (CAZDO_PAT)",
        PatSource::Config => "config ([azure_devops].pat)",
        PatSource::Missing => "missing",
        PatSource::InvalidEnvWhitespace => "invalid: CAZDO_PAT is whitespace-only",
        PatSource::InvalidConfigWhitespace => "invalid: [azure_devops].pat is whitespace-only",
    };

    println!();
    println!("# PAT source: {}", pat_status);
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
    if std::env::var("CAZDO_PAT").is_err() {
        println!();
        println!("Don't forget to set your PAT:");
        println!("  export CAZDO_PAT=\"your-personal-access-token\"");
    }

    Ok(())
}

pub async fn config_verify() -> Result<()> {
    let config = Config::load()?;
    let org_url = config.azure_devops.organization_url.trim();

    println!("Checking Azure DevOps configuration...");
    println!("  organization_url: {}", org_url);

    let pat_source = config.pat_source();
    match pat_source {
        PatSource::Missing => {
            println!("  PAT: missing");
            println!("Cannot verify organization URL/auth without a PAT.");
            println!("Set CAZDO_PAT or [azure_devops].pat, then run `cazdo config verify` again.");
            return Ok(());
        }
        PatSource::InvalidEnvWhitespace => {
            bail!("CAZDO_PAT is whitespace-only. Set a valid token or unset CAZDO_PAT.");
        }
        PatSource::InvalidConfigWhitespace => {
            bail!("[azure_devops].pat is whitespace-only. Set a valid token or remove the field.");
        }
        PatSource::Env => println!("  PAT source: env (CAZDO_PAT)"),
        PatSource::Config => println!("  PAT source: config ([azure_devops].pat)"),
    }

    let client = AzureDevOpsClient::new(&config)?;
    client.verify_connection().await?;

    println!("Verification successful: URL and PAT are working.");
    Ok(())
}
