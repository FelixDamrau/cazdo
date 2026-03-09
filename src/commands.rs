use crate::azure_devops::AzureDevOpsClient;
use crate::config::{Config, PatSource};
use crate::git::{GitRepo, RepoBranch, extract_work_item_number};
use crate::pattern::is_protected;
use crate::tui::render_html;
use crate::tui::{App, BranchInfo, run_app};
use anyhow::{Context, Result, bail};
use crossterm::style::Stylize;

pub async fn interactive() -> Result<()> {
    let repo = GitRepo::open_current_dir().context("Failed to open git repository")?;
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
        .map(|branch| branch_info(branch, &protected_patterns))
        .collect();

    if branch_infos.is_empty() {
        bail!("No branches found in repository");
    }

    let app = App::new(branch_infos, protected_patterns);
    run_app(app, repo).await?;

    Ok(())
}

fn branch_info(branch: RepoBranch, protected_patterns: &[String]) -> BranchInfo {
    let is_current = branch.is_current;
    let is_protected_branch = is_protected(&branch.branch_name, protected_patterns);
    let wi_id = if is_protected_branch {
        None
    } else {
        extract_work_item_number(&branch.branch_name)
    };

    BranchInfo {
        key: branch.key,
        display_name: branch.display_name,
        branch_name: branch.branch_name,
        remote_name: branch.remote_name,
        scope: branch.scope,
        work_item_id: wi_id,
        is_current,
        is_protected: is_protected_branch,
    }
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
    print!("{}", redact_config_for_display(&content));

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
            bail!(
                "PAT is missing. Cannot verify organization URL/auth without a PAT.\nSet CAZDO_PAT or [azure_devops].pat, then run `cazdo config verify` again."
            );
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

pub async fn show_work_item(id: Option<u32>) -> Result<()> {
    let wi_id = match id {
        Some(id) => id,
        None => {
            let repo = GitRepo::open_current_dir().context("Failed to open git repository")?;

            let branch_name = match repo.current_branch() {
                Ok(branch) => branch,
                Err(_) => {
                    bail!("No current branch found.");
                }
            };

            match extract_work_item_number(&branch_name) {
                Some(id) => id,
                None => {
                    bail!(
                        "No work item number found in current branch '{}'.",
                        branch_name
                    );
                }
            }
        }
    };

    let config = Config::load()?;
    let client = AzureDevOpsClient::new(&config)?;
    let wi = client.get_work_item(wi_id).await?;

    let wi_label = format!("#{}", wi.id);
    let linked_wi = wi
        .url
        .as_deref()
        .map(|url| terminal_link(&wi_label, url))
        .unwrap_or(wi_label);

    let state = wi.state.display_name();

    println!(
        "{}  {} - {}",
        linked_wi,
        wi.work_item_type.display_name(),
        state
    );
    println!("{} {}", "Title:".bold(), wi.title);

    if let Some(assigned_to) = wi.assigned_to.as_deref() {
        println!("{} {}", "Assigned:".bold(), assigned_to);
    }

    let description_html = wi
        .rich_text_fields
        .iter()
        .find(|field| field.name == "Description")
        .map(|field| field.value.as_str());

    let description = description_html
        .map(|html| compact_text_preview(html, 220))
        .unwrap_or_else(|| "(none)".to_string());

    println!("{} {}", "Description:".bold(), description);

    Ok(())
}

fn compact_text_preview(html: &str, max_chars: usize) -> String {
    let plain = render_html(html, 120)
        .into_iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ");

    let collapsed = plain.split_whitespace().collect::<Vec<_>>().join(" ");

    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }

    let mut truncated = collapsed
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

fn terminal_link(label: &str, url: &str) -> String {
    format!("\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", url, label)
}

fn redact_config_for_display(content: &str) -> String {
    let mut redacted = String::with_capacity(content.len());
    let mut in_azure_devops_section = false;

    for line in content.split_inclusive('\n') {
        let line_without_newline = line.trim_end_matches(['\r', '\n']);
        let newline = &line[line_without_newline.len()..];

        if let Some(section) = section_name(line_without_newline) {
            in_azure_devops_section = section == "azure_devops";
            redacted.push_str(line);
            continue;
        }

        if in_azure_devops_section && is_pat_assignment(line_without_newline) {
            let indent_len = line_without_newline.len() - line_without_newline.trim_start().len();
            let indent = &line_without_newline[..indent_len];
            redacted.push_str(&format!("{indent}pat = \"***redacted***\"{newline}"));
            continue;
        }

        redacted.push_str(line);
    }

    redacted
}

fn section_name(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') {
        return None;
    }

    let end = trimmed.find(']')?;
    let rest = trimmed[end + 1..].trim_start();
    if !rest.is_empty() && !rest.starts_with('#') {
        return None;
    }

    Some(trimmed[1..end].trim())
}

fn is_pat_assignment(line_without_newline: &str) -> bool {
    let trimmed_start = line_without_newline.trim_start();
    !trimmed_start.starts_with('#')
        && trimmed_start
            .split_once('=')
            .is_some_and(|(key, _)| key.trim() == "pat")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_text_preview_keeps_short_text() {
        let preview = compact_text_preview("<p>Hello <b>world</b></p>", 50);
        assert_eq!(preview, "Hello world");
    }

    #[test]
    fn compact_text_preview_truncates_long_text() {
        let preview = compact_text_preview("<p>abcdefghijklmnopqrstuvwxyz</p>", 10);
        assert_eq!(preview, "abcdefg...");
    }

    #[test]
    fn compact_text_preview_collapses_whitespace() {
        let preview = compact_text_preview("Hello&nbsp;&nbsp; <b>world</b>\n<p>again</p>", 80);
        assert_eq!(preview, "Hello world again");
    }

    #[test]
    fn compact_text_preview_handles_tiny_limits() {
        let preview = compact_text_preview("<p>Hello world</p>", 2);
        assert_eq!(preview, "...");
    }

    #[test]
    fn terminal_link_uses_osc8_format() {
        let out = terminal_link("#123", "https://example.com/wi/123");
        assert_eq!(
            out,
            "\x1b]8;;https://example.com/wi/123\x1b\\#123\x1b]8;;\x1b\\"
        );
    }

    #[test]
    fn redact_config_for_display_redacts_pat_in_azure_devops_section() {
        let input = "[azure_devops]\npat = \"secret-token\"\n";
        let expected = "[azure_devops]\npat = \"***redacted***\"\n";

        assert_eq!(redact_config_for_display(input), expected);
    }

    #[test]
    fn redact_config_for_display_keeps_commented_pat_example() {
        let input = "[azure_devops]\n# pat = \"example-token\"\n";

        assert_eq!(redact_config_for_display(input), input);
    }

    #[test]
    fn redact_config_for_display_does_not_touch_other_sections() {
        let input = "[branches]\npat = \"not-a-real-pat-setting\"\n";

        assert_eq!(redact_config_for_display(input), input);
    }

    #[test]
    fn redact_config_for_display_returns_unchanged_when_no_pat_exists() {
        let input = "[azure_devops]\norganization_url = \"https://dev.azure.com/test\"\n";

        assert_eq!(redact_config_for_display(input), input);
    }

    #[test]
    fn redact_config_for_display_stops_redacting_after_section_switch() {
        let input =
            "[azure_devops]\npat = \"secret-token\"\n[branches]\npat = \"leave-me-alone\"\n";
        let expected =
            "[azure_devops]\npat = \"***redacted***\"\n[branches]\npat = \"leave-me-alone\"\n";

        assert_eq!(redact_config_for_display(input), expected);
    }

    #[test]
    fn redact_config_for_display_handles_inline_comment_on_section_header() {
        let input = "[azure_devops] # local settings\npat = \"secret-token\"\n";
        let expected = "[azure_devops] # local settings\npat = \"***redacted***\"\n";

        assert_eq!(redact_config_for_display(input), expected);
    }
}
