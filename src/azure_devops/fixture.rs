use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use super::work_item::WorkItem;

/// In-memory stand-in for the live Azure DevOps API.
///
/// It stores raw, Azure-shaped JSON per work item id, so it answers exactly the
/// way live does: `get_work_item` decodes the stored JSON through the shared
/// [`codec`](super::codec), and `get_work_item_json` returns it verbatim. The
/// only difference from the live adapter is where the JSON comes from — a file
/// here, an HTTP response there.
#[derive(Clone)]
pub(super) struct FixtureAzureDevOpsClient {
    work_items: HashMap<u32, Value>,
}

impl FixtureAzureDevOpsClient {
    pub(super) fn from_path(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).with_context(|| {
            format!("Failed to read demo work item fixture: {}", path.display())
        })?;
        let entries: Vec<Value> = serde_json::from_str(&content).with_context(|| {
            format!("Failed to parse demo work item fixture: {}", path.display())
        })?;

        let mut work_items = HashMap::new();
        for entry in entries {
            let id = entry.get("id").and_then(Value::as_u64).with_context(|| {
                format!(
                    "Demo work item fixture entry missing numeric 'id': {}",
                    path.display()
                )
            })?;
            let id = u32::try_from(id).with_context(|| {
                format!(
                    "Demo work item fixture entry 'id' is out of u32 range: {}",
                    path.display()
                )
            })?;
            work_items.insert(id, entry);
        }

        Ok(Self { work_items })
    }

    pub(super) fn get_work_item(&self, id: u32) -> Result<WorkItem> {
        super::codec::decode(self.lookup(id)?, id)
    }

    pub(super) fn get_work_item_json(&self, id: u32) -> Result<Value> {
        self.lookup(id).cloned()
    }

    pub(super) fn verify_connection(&self) -> Result<()> {
        Ok(())
    }

    fn lookup(&self, id: u32) -> Result<&Value> {
        self.work_items
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("Work Item #{} not found", id))
    }
}

#[cfg(test)]
mod tests {
    use crate::azure_devops::AzureDevOpsClient;
    use tempfile::TempDir;

    fn write_fixture(temp_dir: &TempDir, content: &str) -> std::path::PathBuf {
        let path = temp_dir.path().join("demo-work-items.json");
        std::fs::write(&path, content).expect("fixture should be written");
        path
    }

    /// A minimal Azure-shaped fixture with a single item (id 101).
    const MINIMAL_FIXTURE: &str = r#"[
  {
    "id": 101,
    "fields": {
      "System.Title": "Show filter behavior in the demo",
      "System.WorkItemType": "Task",
      "System.State": "Committed"
    }
  }
]"#;

    #[tokio::test]
    async fn loads_work_item_from_demo_fixture_file() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let fixture_path = write_fixture(
            &temp_dir,
            r#"[
  {
    "id": 101,
    "fields": {
      "System.Title": "Show filter behavior in the demo",
      "System.WorkItemType": "Task",
      "System.State": "Committed",
      "System.AssignedTo": { "displayName": "Demo User" },
      "System.Tags": "Docs; Demo",
      "System.Description": "<p>Use this item to show that parsed branch IDs resolve to Azure DevOps-style details.</p>"
    },
    "_links": { "html": { "href": "https://example.test/items/101" } },
    "relations": []
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
        let fixture_path = write_fixture(&temp_dir, MINIMAL_FIXTURE);

        let client = AzureDevOpsClient::new_fixture(&fixture_path)
            .expect("fixture-backed client should initialize");

        let error = client
            .get_work_item(999)
            .await
            .expect_err("missing fixture item should error");

        assert_eq!(error.to_string(), "Work Item #999 not found");
    }

    #[tokio::test]
    async fn returns_stored_json_verbatim_from_demo_fixture_file() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let fixture_path = write_fixture(
            &temp_dir,
            r#"[
  {
    "id": 101,
    "fields": {
      "System.Title": "Show filter behavior in the demo",
      "System.WorkItemType": "Task",
      "System.State": "Committed",
      "System.AssignedTo": { "displayName": "Demo User" },
      "System.Tags": "Docs; Demo",
      "System.Description": "<p>Use this item to show JSON output.</p>"
    },
    "_links": { "html": { "href": "https://example.test/items/101" } },
    "relations": []
  }
]"#,
        );

        let client = AzureDevOpsClient::new_fixture(&fixture_path)
            .expect("fixture-backed client should initialize");

        let json = client
            .get_work_item_json(101)
            .await
            .expect("fixture item json should load");

        assert_eq!(json["id"], 101);
        assert_eq!(
            json["fields"]["System.Title"],
            "Show filter behavior in the demo"
        );
        assert_eq!(json["fields"]["System.WorkItemType"], "Task");
        assert_eq!(json["fields"]["System.State"], "Committed");
        assert_eq!(
            json["fields"]["System.AssignedTo"]["displayName"],
            "Demo User"
        );
        assert_eq!(json["fields"]["System.Tags"], "Docs; Demo");
        assert_eq!(
            json["fields"]["System.Description"],
            "<p>Use this item to show JSON output.</p>"
        );
        assert_eq!(
            json["_links"]["html"]["href"],
            "https://example.test/items/101"
        );
    }

    #[tokio::test]
    async fn returns_not_found_for_missing_demo_fixture_json_item() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let fixture_path = write_fixture(&temp_dir, MINIMAL_FIXTURE);

        let client = AzureDevOpsClient::new_fixture(&fixture_path)
            .expect("fixture-backed client should initialize");

        let error = client
            .get_work_item_json(999)
            .await
            .expect_err("missing fixture item json should error");

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
    fn rejects_fixture_entry_without_id() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let fixture_path = write_fixture(
            &temp_dir,
            r#"[
  {
    "fields": {
      "System.Title": "Missing id",
      "System.WorkItemType": "Task",
      "System.State": "Committed"
    }
  }
]"#,
        );

        let error = AzureDevOpsClient::new_fixture(&fixture_path)
            .err()
            .expect("entry without id should fail to load");

        assert!(error.to_string().contains("missing numeric 'id'"));
    }

    #[test]
    fn rejects_fixture_entry_with_oversized_id() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let fixture_path = write_fixture(
            &temp_dir,
            r#"[
  {
    "id": 4294967296,
    "fields": {
      "System.Title": "Oversized id",
      "System.WorkItemType": "Task",
      "System.State": "Committed"
    }
  }
]"#,
        );

        let error = AzureDevOpsClient::new_fixture(&fixture_path)
            .err()
            .expect("entry with out-of-range id should fail to load");

        assert!(error.to_string().contains("out of u32 range"));
    }

    #[test]
    fn new_fixture_uses_fixture_provider() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let fixture_path = write_fixture(&temp_dir, MINIMAL_FIXTURE);

        let client = AzureDevOpsClient::new_fixture(&fixture_path)
            .expect("fixture-backed client should initialize");

        assert!(client.uses_demo_fixture());
    }

    #[tokio::test]
    async fn fixture_verify_connection_does_not_depend_on_file_after_load() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let fixture_path = write_fixture(&temp_dir, MINIMAL_FIXTURE);

        let client = AzureDevOpsClient::new_fixture(&fixture_path)
            .expect("fixture-backed client should initialize");
        std::fs::remove_file(&fixture_path).expect("fixture file should be removed");

        client
            .verify_connection()
            .await
            .expect("loaded fixture client should still verify");
    }
}
