use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Default protected branch patterns (main/master)
pub const DEFAULT_PROTECTED_PATTERNS: &[&str] = &["main", "master"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatSource {
    Env,
    Config,
    Missing,
    InvalidEnvWhitespace,
    InvalidConfigWhitespace,
}

enum PatResolution {
    Valid { source: PatSource, token: String },
    Missing,
    InvalidEnvWhitespace,
    InvalidConfigWhitespace,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub azure_devops: AzureDevOpsConfig,
    #[serde(default)]
    pub branches: BranchConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AzureDevOpsConfig {
    pub organization_url: String,
    #[serde(default)]
    pub pat: Option<String>,
}

impl Default for AzureDevOpsConfig {
    fn default() -> Self {
        Self {
            organization_url: "https://dev.azure.com/your-organization".to_string(),
            pat: None,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            azure_devops: AzureDevOpsConfig::default(),
            branches: BranchConfig {
                protected: DEFAULT_PROTECTED_PATTERNS
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            },
        }
    }
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
    pub fn config_path() -> Result<PathBuf> {
        let proj_dirs =
            ProjectDirs::from("", "", "cazdo").context("Failed to determine config directory")?;

        Ok(proj_dirs.config_dir().join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            bail!(
                "Configuration file not found at {}\n\nRun 'cazdo config init' to create a default configuration.",
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

    pub fn get_pat(&self) -> Result<String> {
        // Read from actual environment or use fallback logic
        self.resolve_pat(std::env::var("CAZDO_PAT").ok())
    }

    pub fn pat_source(&self) -> PatSource {
        self.resolve_pat_source(std::env::var("CAZDO_PAT").ok())
    }

    /// Helper for tests to abstract env::var("CAZDO_PAT")
    fn resolve_pat(&self, env_pat: Option<String>) -> Result<String> {
        match self.resolve_pat_resolution(env_pat) {
            PatResolution::Valid { token, .. } => Ok(token),
            PatResolution::InvalidEnvWhitespace => {
                bail!(
                    "CAZDO_PAT is set but empty/whitespace. Set a valid token or unset CAZDO_PAT to use config value."
                )
            }
            PatResolution::InvalidConfigWhitespace => {
                bail!(
                    "Config value [azure_devops].pat is empty/whitespace. Set a valid token or remove the field."
                )
            }
            PatResolution::Missing => anyhow::bail!(
                "Azure DevOps PAT not found.\n\n\
                You can set it in two ways (checked in order):\n\
                1. Environment variable: export CAZDO_PAT=\"your-token\"\n\
                2. Config file: Add 'pat = \"your-token\"' under [azure_devops] section in config.toml\n\n\
                The PAT needs 'Work Items (Read)' permission."
            ),
        }
    }

    /// Helper for status display and tests.
    fn resolve_pat_source(&self, env_pat: Option<String>) -> PatSource {
        match self.resolve_pat_resolution(env_pat) {
            PatResolution::Valid { source, .. } => source,
            PatResolution::Missing => PatSource::Missing,
            PatResolution::InvalidEnvWhitespace => PatSource::InvalidEnvWhitespace,
            PatResolution::InvalidConfigWhitespace => PatSource::InvalidConfigWhitespace,
        }
    }

    fn resolve_pat_resolution(&self, env_pat: Option<String>) -> PatResolution {
        if let Some(pat) = env_pat {
            let trimmed = pat.trim();
            if trimmed.is_empty() {
                return PatResolution::InvalidEnvWhitespace;
            }
            return PatResolution::Valid {
                source: PatSource::Env,
                token: trimmed.to_string(),
            };
        }

        if let Some(pat) = &self.azure_devops.pat {
            let trimmed = pat.trim();
            if trimmed.is_empty() {
                return PatResolution::InvalidConfigWhitespace;
            }
            return PatResolution::Valid {
                source: PatSource::Config,
                token: trimmed.to_string(),
            };
        }

        PatResolution::Missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_pat_precedence() {
        let config = Config {
            azure_devops: AzureDevOpsConfig {
                organization_url: "https://dev.azure.com/test".to_string(),
                pat: Some("config-pat".to_string()),
            },
            branches: BranchConfig::default(),
        };

        // Case 1: Env var set (should override config)
        let pat = config.resolve_pat(Some("env-pat".to_string())).unwrap();
        assert_eq!(pat, "env-pat");

        // Case 2: Env var empty (should fallback to config)
        let pat = config.resolve_pat(None).unwrap();
        assert_eq!(pat, "config-pat");

        // Case 3: Env var with surrounding whitespace is trimmed
        let pat = config.resolve_pat(Some("  env-pat  ".to_string())).unwrap();
        assert_eq!(pat, "env-pat");
    }

    #[test]
    fn test_get_pat_rejects_whitespace_sources() {
        let config_with_pat = Config {
            azure_devops: AzureDevOpsConfig {
                organization_url: "https://dev.azure.com/test".to_string(),
                pat: Some("config-pat".to_string()),
            },
            branches: BranchConfig::default(),
        };

        // Whitespace env is treated as invalid (no fallback)
        assert!(
            config_with_pat
                .resolve_pat(Some("   \t\n".to_string()))
                .is_err()
        );

        let config_whitespace = Config {
            azure_devops: AzureDevOpsConfig {
                organization_url: "https://dev.azure.com/test".to_string(),
                pat: Some("   ".to_string()),
            },
            branches: BranchConfig::default(),
        };
        assert!(config_whitespace.resolve_pat(None).is_err());
    }

    #[test]
    fn test_pat_source_resolution() {
        let config = Config {
            azure_devops: AzureDevOpsConfig {
                organization_url: "https://dev.azure.com/test".to_string(),
                pat: Some("config-pat".to_string()),
            },
            branches: BranchConfig::default(),
        };

        assert_eq!(
            config.resolve_pat_source(Some("env-pat".to_string())),
            PatSource::Env
        );
        assert_eq!(
            config.resolve_pat_source(Some("   ".to_string())),
            PatSource::InvalidEnvWhitespace
        );
        assert_eq!(config.resolve_pat_source(None), PatSource::Config);

        let no_pat_config = Config {
            azure_devops: AzureDevOpsConfig {
                organization_url: "https://dev.azure.com/test".to_string(),
                pat: None,
            },
            branches: BranchConfig::default(),
        };
        assert_eq!(no_pat_config.resolve_pat_source(None), PatSource::Missing);

        let whitespace_config = Config {
            azure_devops: AzureDevOpsConfig {
                organization_url: "https://dev.azure.com/test".to_string(),
                pat: Some("   ".to_string()),
            },
            branches: BranchConfig::default(),
        };
        assert_eq!(
            whitespace_config.resolve_pat_source(None),
            PatSource::InvalidConfigWhitespace
        );
    }

    #[test]
    fn test_get_pat_from_env_only() {
        let config = Config {
            azure_devops: AzureDevOpsConfig {
                organization_url: "https://dev.azure.com/test".to_string(),
                pat: None,
            },
            branches: BranchConfig::default(),
        };

        let pat = config.resolve_pat(Some("env-pat".to_string())).unwrap();
        assert_eq!(pat, "env-pat");
    }

    #[test]
    fn test_get_pat_missing() {
        let config = Config {
            azure_devops: AzureDevOpsConfig {
                organization_url: "https://dev.azure.com/test".to_string(),
                pat: None,
            },
            branches: BranchConfig::default(),
        };

        assert!(config.resolve_pat(None).is_err());
    }
}
