use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

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

        write_config_file(&config_path, &content)?;

        Ok(())
    }

    /// Redact the PAT in raw config file text for display (e.g. `cazdo config show`).
    ///
    /// Operates on the raw TOML so comments, formatting, and unrelated fields are
    /// preserved; only a `pat` assignment inside the `[azure_devops]` section is masked.
    pub fn redact_for_display(content: &str) -> String {
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
                let indent_len =
                    line_without_newline.len() - line_without_newline.trim_start().len();
                let indent = &line_without_newline[..indent_len];
                redacted.push_str(&format!("{indent}pat = \"***redacted***\"{newline}"));
                continue;
            }

            redacted.push_str(line);
        }

        redacted
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
            .is_some_and(|(key, _)| toml_key_name(key.trim()) == Some("pat"))
}

fn toml_key_name(key: &str) -> Option<&str> {
    if let Some(stripped) = key.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Some(stripped.trim());
    }

    if let Some(stripped) = key.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return Some(stripped.trim());
    }

    if key.is_empty() { None } else { Some(key) }
}

fn write_config_file(path: &Path, content: &str) -> Result<()> {
    #[cfg(unix)]
    {
        // Use 0o600 on create as a best-effort default; umask may still make the
        // initial mode more restrictive, so set_permissions below enforces 0o600.
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;

        file.write_all(content.as_bytes())
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;

        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).with_context(|| {
            format!(
                "Failed to set permissions on config file: {}",
                path.display()
            )
        })?;

        Ok(())
    }

    #[cfg(not(unix))]
    {
        fs::write(path, content)
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("cazdo-config-test-{}-{unique}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[cfg(unix)]
    fn file_mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

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

    #[test]
    fn write_config_file_writes_content_to_target_path() {
        let temp_dir = TestDir::new();
        let config_path = temp_dir.path().join("config.toml");

        write_config_file(&config_path, "hello = \"world\"\n").unwrap();

        assert_eq!(
            fs::read_to_string(&config_path).unwrap(),
            "hello = \"world\"\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_config_file_creates_unix_config_with_owner_only_permissions() {
        let temp_dir = TestDir::new();
        let config_path = temp_dir.path().join("config.toml");

        write_config_file(&config_path, "hello = \"world\"\n").unwrap();

        assert_eq!(file_mode(&config_path), 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn write_config_file_tightens_existing_unix_permissions_on_overwrite() {
        let temp_dir = TestDir::new();
        let config_path = temp_dir.path().join("config.toml");

        fs::write(&config_path, "old = true\n").unwrap();
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o644)).unwrap();

        write_config_file(&config_path, "new = true\n").unwrap();

        assert_eq!(fs::read_to_string(&config_path).unwrap(), "new = true\n");
        assert_eq!(file_mode(&config_path), 0o600);
    }

    #[test]
    fn redact_for_display_redacts_pat_in_azure_devops_section() {
        let input = "[azure_devops]\npat = \"secret-token\"\n";
        let expected = "[azure_devops]\npat = \"***redacted***\"\n";

        assert_eq!(Config::redact_for_display(input), expected);
    }

    #[test]
    fn redact_for_display_keeps_commented_pat_example() {
        let input = "[azure_devops]\n# pat = \"example-token\"\n";

        assert_eq!(Config::redact_for_display(input), input);
    }

    #[test]
    fn redact_for_display_does_not_touch_other_sections() {
        let input = "[branches]\npat = \"not-a-real-pat-setting\"\n";

        assert_eq!(Config::redact_for_display(input), input);
    }

    #[test]
    fn redact_for_display_returns_unchanged_when_no_pat_exists() {
        let input = "[azure_devops]\norganization_url = \"https://dev.azure.com/test\"\n";

        assert_eq!(Config::redact_for_display(input), input);
    }

    #[test]
    fn redact_for_display_stops_redacting_after_section_switch() {
        let input =
            "[azure_devops]\npat = \"secret-token\"\n[branches]\npat = \"leave-me-alone\"\n";
        let expected =
            "[azure_devops]\npat = \"***redacted***\"\n[branches]\npat = \"leave-me-alone\"\n";

        assert_eq!(Config::redact_for_display(input), expected);
    }

    #[test]
    fn redact_for_display_handles_inline_comment_on_section_header() {
        let input = "[azure_devops] # local settings\npat = \"secret-token\"\n";
        let expected = "[azure_devops] # local settings\npat = \"***redacted***\"\n";

        assert_eq!(Config::redact_for_display(input), expected);
    }

    #[test]
    fn redact_for_display_redacts_double_quoted_pat_key() {
        let input = "[azure_devops]\n\"pat\" = \"secret-token\"\n";
        let expected = "[azure_devops]\npat = \"***redacted***\"\n";

        assert_eq!(Config::redact_for_display(input), expected);
    }

    #[test]
    fn redact_for_display_redacts_single_quoted_pat_key() {
        let input = "[azure_devops]\n'pat' = \"secret-token\"\n";
        let expected = "[azure_devops]\npat = \"***redacted***\"\n";

        assert_eq!(Config::redact_for_display(input), expected);
    }

    #[test]
    fn redact_for_display_redacts_quoted_pat_key_with_inner_whitespace() {
        let input = "[azure_devops]\n\" pat \" = \"secret-token\"\n";
        let expected = "[azure_devops]\npat = \"***redacted***\"\n";

        assert_eq!(Config::redact_for_display(input), expected);
    }
}
