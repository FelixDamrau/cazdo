use super::work_item::WorkItem;
use crate::config::Config;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct AzureDevOpsClient {
    provider: WorkItemProvider,
}

#[derive(Clone)]
enum WorkItemProvider {
    Live(LiveAzureDevOpsClient),
    Fixture(FixtureAzureDevOpsClient),
}

#[derive(Clone)]
struct LiveAzureDevOpsClient {
    client: Client,
    base_url: String,
    pat: String,
}

#[derive(Clone)]
struct FixtureAzureDevOpsClient {
    fixture_path: PathBuf,
    work_items: HashMap<u32, WorkItem>,
}

#[derive(Debug, Deserialize)]
struct FixtureWorkItem {
    id: u32,
    title: String,
    work_item_type: String,
    state: String,
    #[serde(default)]
    assigned_to: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    description: Option<String>,
}

impl AzureDevOpsClient {
    pub fn new(config: &Config) -> Result<Self> {
        Self::new_with_demo_fixture(
            config,
            std::env::var_os("CAZDO_DEMO_WORK_ITEMS").map(PathBuf::from),
        )
    }

    pub fn new_live(config: &Config) -> Result<Self> {
        let pat = config.get_pat()?;

        let client = Client::builder()
            .build()
            .context("Failed to create HTTP client")?;

        // Normalize the base URL (remove trailing slash)
        let base_url = config
            .azure_devops
            .organization_url
            .trim_end_matches('/')
            .to_string();

        Ok(Self {
            provider: WorkItemProvider::Live(LiveAzureDevOpsClient {
                client,
                base_url,
                pat,
            }),
        })
    }

    fn new_with_demo_fixture<T>(config: &Config, fixture_path: Option<T>) -> Result<Self>
    where
        T: AsRef<Path>,
    {
        if let Some(path) = fixture_path {
            return Ok(Self {
                provider: WorkItemProvider::Fixture(FixtureAzureDevOpsClient::from_path(
                    path.as_ref(),
                )?),
            });
        }

        Self::new_live(config)
    }

    #[cfg(test)]
    pub fn uses_demo_fixture(&self) -> bool {
        matches!(self.provider, WorkItemProvider::Fixture(_))
    }

    pub async fn get_work_item(&self, id: u32) -> Result<WorkItem> {
        match &self.provider {
            WorkItemProvider::Live(client) => client.get_work_item(id).await,
            WorkItemProvider::Fixture(client) => client.get_work_item(id),
        }
    }

    pub async fn verify_connection(&self) -> Result<()> {
        match &self.provider {
            WorkItemProvider::Live(client) => client.verify_connection().await,
            WorkItemProvider::Fixture(client) => client.verify_connection(),
        }
    }
}

impl LiveAzureDevOpsClient {
    async fn get_work_item(&self, id: u32) -> Result<WorkItem> {
        let url = format!(
            "{}/_apis/wit/workitems/{}?api-version=7.0",
            self.base_url, id
        );

        let response = self
            .client
            .get(&url)
            .basic_auth("", Some(&self.pat))
            .send()
            .await
            .context("Failed to send request to Azure DevOps")?;

        let status = response.status();
        if !status.is_success() || status == reqwest::StatusCode::NON_AUTHORITATIVE_INFORMATION {
            return Err(self.extract_api_error(response, id).await);
        }

        let json: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse work item response")?;

        WorkItem::from_json(&json, id)
    }

    async fn verify_connection(&self) -> Result<()> {
        let url = format!("{}/_apis/connectionData", self.base_url);

        let response = self
            .client
            .get(&url)
            .header(reqwest::header::ACCEPT, "application/json")
            .basic_auth("", Some(&self.pat))
            .send()
            .await
            .context("Failed to send verification request to Azure DevOps")?;

        let status = response.status();
        if status.is_success() {
            // 200 can still be a login HTML page in some setups.
            // Require JSON + authenticatedUser so verify checks real API auth.
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            if !content_type.contains("application/json") {
                return Err(anyhow::anyhow!(
                    "Verification returned non-JSON response ({}). This often means the request was redirected to a login page; check PAT/auth setup.",
                    content_type
                ));
            }

            let json: serde_json::Value = response
                .json()
                .await
                .context("Failed to parse Azure DevOps verification response")?;

            let has_authenticated_user = json
                .get("authenticatedUser")
                .and_then(|u| u.get("id"))
                .and_then(|id| id.as_str())
                .is_some();

            if !has_authenticated_user {
                return Err(anyhow::anyhow!(
                    "Verification response missing authenticated user details"
                ));
            }

            return Ok(());
        }

        Err(self.extract_verification_error(response).await)
    }

    async fn extract_api_error(&self, response: reqwest::Response, id: u32) -> anyhow::Error {
        let status = response.status();

        if status == reqwest::StatusCode::NOT_FOUND {
            return anyhow::anyhow!("Work Item #{} not found", id);
        }

        if status == reqwest::StatusCode::NON_AUTHORITATIVE_INFORMATION {
            return anyhow::anyhow!(
                "Authentication failed (Status 203). This is most likely due to an invalid PAT. Please check your PAT and Azure DevOps organization URL."
            );
        }

        let body = match response.text().await {
            Ok(t) => t,
            Err(e) => {
                return anyhow::anyhow!(
                    "Azure DevOps API error ({}): failed to read body: {}",
                    status,
                    e
                );
            }
        };

        // Try to extract clean message from JSON error response
        let error_msg = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|json| {
                json.get("message")
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| format!("Azure DevOps API error ({})", status));

        anyhow::anyhow!("{}", error_msg)
    }

    async fn extract_verification_error(&self, response: reqwest::Response) -> anyhow::Error {
        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
            || status == reqwest::StatusCode::NON_AUTHORITATIVE_INFORMATION
        {
            return anyhow::anyhow!(
                "Authentication failed (status {}). Check your PAT and organization URL.",
                status
            );
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            return anyhow::anyhow!(
                "Verification endpoint not found (404). Check the organization URL (including collection/path for Azure DevOps Server)."
            );
        }

        let body = match response.text().await {
            Ok(t) => t,
            Err(e) => {
                return anyhow::anyhow!(
                    "Azure DevOps verification error ({}): failed to read body: {}",
                    status,
                    e
                );
            }
        };

        let error_msg = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|json| {
                json.get("message")
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| format!("Azure DevOps verification failed ({})", status));

        anyhow::anyhow!("{}", error_msg)
    }
}

impl FixtureAzureDevOpsClient {
    fn from_path(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).with_context(|| {
            format!("Failed to read demo work item fixture: {}", path.display())
        })?;
        let fixture_items: Vec<FixtureWorkItem> =
            serde_json::from_str(&content).with_context(|| {
                format!("Failed to parse demo work item fixture: {}", path.display())
            })?;

        let work_items = fixture_items
            .into_iter()
            .map(|item| Ok((item.id, item.into_work_item()?)))
            .collect::<Result<HashMap<_, _>>>()?;

        Ok(Self {
            fixture_path: path.to_path_buf(),
            work_items,
        })
    }

    fn get_work_item(&self, id: u32) -> Result<WorkItem> {
        self.work_items
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Work Item #{} not found", id))
    }

    fn verify_connection(&self) -> Result<()> {
        if self.fixture_path.exists() {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Demo work item fixture is missing: {}",
                self.fixture_path.display()
            ))
        }
    }
}

impl FixtureWorkItem {
    fn into_work_item(self) -> Result<WorkItem> {
        let mut rich_text_fields = Vec::new();
        if let Some(description) = self.description
            && !description.trim().is_empty()
        {
            rich_text_fields.push(super::work_item::RichTextField {
                name: "Description".to_string(),
                value: description,
            });
        }

        Ok(WorkItem {
            id: self.id,
            title: self.title,
            work_item_type: self.work_item_type.parse().unwrap(),
            state: self.state.parse().unwrap(),
            assigned_to: self.assigned_to,
            url: self.url,
            tags: self.tags,
            rich_text_fields,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use tempfile::TempDir;

    fn test_config() -> Config {
        let mut config = Config::default();
        config.azure_devops.organization_url = "https://dev.azure.com/test-org".to_string();
        config.azure_devops.pat = Some("test-pat".to_string());
        config
    }

    fn write_fixture(temp_dir: &TempDir, content: &str) -> std::path::PathBuf {
        let path = temp_dir.path().join("demo-work-items.json");
        std::fs::write(&path, content).expect("fixture should be written");
        path
    }

    #[tokio::test]
    async fn loads_work_item_from_demo_fixture_file() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let fixture_path = write_fixture(
            &temp_dir,
            r#"[
  {
    "id": 101,
    "title": "Show filter behavior in the demo",
    "work_item_type": "Task",
    "state": "Committed",
    "assigned_to": "Demo User",
    "tags": ["Docs", "Demo"],
    "description": "<p>Use this item to show that parsed branch IDs resolve to Azure DevOps-style details.</p>",
    "url": "https://example.test/items/101"
  }
]"#,
        );

        let client = AzureDevOpsClient::new_with_demo_fixture(&test_config(), Some(&fixture_path))
            .expect("fixture-backed client should initialize");

        let work_item = client
            .get_work_item(101)
            .await
            .expect("fixture item should load");

        assert_eq!(work_item.id, 101);
        assert_eq!(work_item.title, "Show filter behavior in the demo");
        assert_eq!(work_item.work_item_type.display_name(), "Task");
        assert_eq!(work_item.state.display_name(), "Committed");
        assert_eq!(work_item.assigned_to.as_deref(), Some("Demo User"));
        assert_eq!(work_item.tags, vec!["Docs", "Demo"]);
        assert_eq!(work_item.rich_text_fields.len(), 1);
        assert_eq!(work_item.rich_text_fields[0].name, "Description");
    }

    #[tokio::test]
    async fn returns_not_found_for_missing_demo_fixture_item() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let fixture_path = write_fixture(
            &temp_dir,
            r#"[
  {
    "id": 101,
    "title": "Show filter behavior in the demo",
    "work_item_type": "Task",
    "state": "Committed",
    "description": "<p>Visible in the fixture.</p>"
  }
]"#,
        );

        let client = AzureDevOpsClient::new_with_demo_fixture(&test_config(), Some(&fixture_path))
            .expect("fixture-backed client should initialize");

        let error = client
            .get_work_item(999)
            .await
            .expect_err("missing fixture item should error");

        assert_eq!(error.to_string(), "Work Item #999 not found");
    }

    #[test]
    fn rejects_malformed_demo_fixture_file() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let fixture_path = write_fixture(&temp_dir, r#"{ "broken": true }"#);

        let error = AzureDevOpsClient::new_with_demo_fixture(&test_config(), Some(&fixture_path))
            .err()
            .expect("malformed fixture should fail to load");

        assert!(
            error
                .to_string()
                .contains("Failed to parse demo work item fixture")
        );
    }

    #[test]
    fn selects_demo_fixture_provider_when_env_var_is_set() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let fixture_path = write_fixture(
            &temp_dir,
            r#"[
  {
    "id": 101,
    "title": "Show filter behavior in the demo",
    "work_item_type": "Task",
    "state": "Committed",
    "description": "<p>Visible in the fixture.</p>"
  }
]"#,
        );

        let client = AzureDevOpsClient::new_with_demo_fixture(&test_config(), Some(&fixture_path))
            .expect("fixture-backed client should initialize");

        assert!(client.uses_demo_fixture());
    }
}
