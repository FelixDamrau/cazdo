use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Default protected branch patterns (main/master)
pub const DEFAULT_PROTECTED_PATTERNS: &[&str] = &["main", "master"];

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub azure_devops: AzureDevOpsConfig,
    #[serde(default)]
    pub branches: BranchConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AzureDevOpsConfig {
    pub organization_url: String,
}

/// Branch-related configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BranchConfig {
    /// Patterns for protected branches (supports * wildcard)
    /// Default: ["main", "master"]
    #[serde(default)]
    pub protected: Vec<String>,
}

impl BranchConfig {
    /// Get protected patterns, falling back to defaults if not configured
    pub fn protected_patterns(&self) -> Vec<String> {
        if self.protected.is_empty() {
            DEFAULT_PROTECTED_PATTERNS
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            self.protected.clone()
        }
    }
}

impl Config {
    pub fn new(organization_url: String) -> Self {
        Self {
            azure_devops: AzureDevOpsConfig { organization_url },
            branches: BranchConfig::default(),
        }
    }

    pub fn config_path() -> Result<PathBuf> {
        let proj_dirs =
            ProjectDirs::from("", "", "cazdo").context("Failed to determine config directory")?;

        Ok(proj_dirs.config_dir().join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            bail!(
                "Configuration file not found at {}\n\nRun 'cazdo config' to set up your configuration.",
                config_path.display()
            );
        }

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", config_path.display()))?;

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        // Create parent directories if they don't exist
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create config directory: {}", parent.display())
            })?;
        }

        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;

        fs::write(&config_path, content)
            .with_context(|| format!("Failed to write config file: {}", config_path.display()))?;

        Ok(())
    }

    pub fn get_pat() -> Result<String> {
        std::env::var("CAZDO_PAT").context(
            "CAZDO_PAT environment variable not set.\n\n\
            Set your Azure DevOps Personal Access Token:\n  \
            export CAZDO_PAT=\"your-personal-access-token\"\n\n\
            The PAT needs 'Work Items (Read)' permission.",
        )
    }
}
