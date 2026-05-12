use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use super::work_item::{WorkItem, WorkItemParts};

#[derive(Clone)]
pub(super) struct FixtureAzureDevOpsClient {
    work_items: HashMap<u32, FixtureWorkItem>,
}

#[derive(Clone, Debug, Deserialize)]
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

impl FixtureAzureDevOpsClient {
    pub(super) fn from_path(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).with_context(|| {
            format!("Failed to read demo work item fixture: {}", path.display())
        })?;
        let fixture_items: Vec<FixtureWorkItem> =
            serde_json::from_str(&content).with_context(|| {
                format!("Failed to parse demo work item fixture: {}", path.display())
            })?;

        let work_items = fixture_items
            .into_iter()
            .map(|item| (item.id, item))
            .collect::<HashMap<_, _>>();

        Ok(Self { work_items })
    }

    pub(super) fn get_work_item(&self, id: u32) -> Result<WorkItem> {
        self.work_items
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("Work Item #{} not found", id))?
            .clone()
            .into_work_item()
    }

    pub(super) fn verify_connection(&self) -> Result<()> {
        Ok(())
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

        Ok(WorkItem::from_parts(WorkItemParts {
            id: self.id,
            title: self.title,
            work_item_type: &self.work_item_type,
            state: &self.state,
            assigned_to: self.assigned_to,
            url: self.url,
            tags: self.tags,
            rich_text_fields,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azure_devops::AzureDevOpsClient;
    use serde_json::json;
    use tempfile::TempDir;

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

        let client = AzureDevOpsClient::new_fixture(&fixture_path)
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

        let client = AzureDevOpsClient::new_fixture(&fixture_path)
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

        let error = AzureDevOpsClient::new_fixture(&fixture_path)
            .err()
            .expect("malformed fixture should fail to load");

        assert!(
            error
                .to_string()
                .contains("Failed to parse demo work item fixture")
        );
    }

    #[test]
    fn new_fixture_uses_fixture_provider() {
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

        let client = AzureDevOpsClient::new_fixture(&fixture_path)
            .expect("fixture-backed client should initialize");

        assert!(client.uses_demo_fixture());
    }

    #[tokio::test]
    async fn fixture_verify_connection_does_not_depend_on_file_after_load() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let fixture_path = write_fixture(
            &temp_dir,
            r#"[
  {
    "id": 101,
    "title": "Improve branch filtering UX",
    "work_item_type": "Task",
    "state": "Committed",
    "description": "<p>Visible in the fixture.</p>"
  }
]"#,
        );

        let client = AzureDevOpsClient::new_fixture(&fixture_path)
            .expect("fixture-backed client should initialize");
        std::fs::remove_file(&fixture_path).expect("fixture file should be removed");

        client
            .verify_connection()
            .await
            .expect("loaded fixture client should still verify");
    }

    #[test]
    fn fixture_and_live_mapping_produce_matching_work_items() {
        let fixture_work_item = FixtureWorkItem {
            id: 321,
            title: "Keep fixture and live parsing aligned".to_string(),
            work_item_type: "Feature".to_string(),
            state: "Committed".to_string(),
            assigned_to: Some("Demo User".to_string()),
            url: Some("https://example.test/items/321".to_string()),
            tags: vec!["Demo".to_string(), "Parity".to_string()],
            description: Some("<p>Shared description</p>".to_string()),
        }
        .into_work_item()
        .expect("fixture item should parse");

        let live_json = json!({
            "fields": {
                "System.Title": "Keep fixture and live parsing aligned",
                "System.WorkItemType": "Feature",
                "System.State": "Committed",
                "System.AssignedTo": {
                    "displayName": "Demo User"
                },
                "System.Tags": "Demo; Parity",
                "System.Description": "<p>Shared description</p>"
            },
            "_links": {
                "html": {
                    "href": "https://example.test/items/321"
                }
            }
        });

        let live_work_item = WorkItem::from_json(&live_json, 321).expect("live item should parse");

        assert_eq!(fixture_work_item.id, live_work_item.id);
        assert_eq!(fixture_work_item.title, live_work_item.title);
        assert_eq!(
            fixture_work_item.work_item_type.display_name(),
            live_work_item.work_item_type.display_name()
        );
        assert_eq!(
            fixture_work_item.state.display_name(),
            live_work_item.state.display_name()
        );
        assert_eq!(fixture_work_item.assigned_to, live_work_item.assigned_to);
        assert_eq!(fixture_work_item.url, live_work_item.url);
        assert_eq!(fixture_work_item.tags, live_work_item.tags);
        assert_eq!(
            fixture_work_item.rich_text_fields.len(),
            live_work_item.rich_text_fields.len()
        );
        assert_eq!(fixture_work_item.rich_text_fields[0].name, "Description");
        assert_eq!(
            fixture_work_item.rich_text_fields[0].name,
            live_work_item.rich_text_fields[0].name
        );
        assert_eq!(
            fixture_work_item.rich_text_fields[0].value,
            live_work_item.rich_text_fields[0].value
        );
    }
}
