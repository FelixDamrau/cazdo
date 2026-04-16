use anyhow::{Context, Result};
use ratatui::style::Color;
use serde_json::Value;
use std::str::FromStr;

/// A rich text field from Azure DevOps (usually contains HTML)
#[derive(Debug, Clone)]
pub struct RichTextField {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WorkItem {
    pub id: u32,
    pub title: String,
    pub work_item_type: WorkItemType,
    pub state: WorkItemState,
    pub assigned_to: Option<String>,
    pub url: Option<String>,
    pub tags: Vec<String>,
    /// Dynamic rich text fields (Description, Acceptance Criteria, Repro Steps, etc.)
    pub rich_text_fields: Vec<RichTextField>,
}

pub(crate) struct WorkItemParts<'a> {
    pub id: u32,
    pub title: String,
    pub work_item_type: &'a str,
    pub state: &'a str,
    pub assigned_to: Option<String>,
    pub url: Option<String>,
    pub tags: Vec<String>,
    pub rich_text_fields: Vec<RichTextField>,
}

#[derive(Debug, Clone)]
pub enum WorkItemType {
    Bug,
    ProductBacklogItem,
    UserStory,
    Task,
    Feature,
    Epic,
    Other(String),
}

impl FromStr for WorkItemType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "bug" => Self::Bug,
            "product backlog item" => Self::ProductBacklogItem,
            "user story" => Self::UserStory,
            "task" => Self::Task,
            "feature" => Self::Feature,
            "epic" => Self::Epic,
            _ => Self::Other(s.to_string()),
        })
    }
}

impl WorkItemType {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Bug => "🐞",
            Self::ProductBacklogItem => "📘",
            Self::UserStory => "📖",
            Self::Task => "📒",
            Self::Feature => "🏆",
            Self::Epic => "👑",
            Self::Other(_) => "📄",
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::Bug => "Bug",
            Self::ProductBacklogItem => "Product Backlog Item",
            Self::UserStory => "User Story",
            Self::Task => "Task",
            Self::Feature => "Feature",
            Self::Epic => "Epic",
            Self::Other(s) => s,
        }
    }
}

#[derive(Debug, Clone)]
pub enum WorkItemState {
    New,
    Approved,
    Committed,
    Active,
    Resolved,
    Closed,
    Removed,
    Done,
    Other(String),
}

impl FromStr for WorkItemState {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "new" => Self::New,
            "approved" => Self::Approved,
            "committed" => Self::Committed,
            "active" => Self::Active,
            "resolved" => Self::Resolved,
            "closed" => Self::Closed,
            "removed" => Self::Removed,
            "done" => Self::Done,
            _ => Self::Other(s.to_string()),
        })
    }
}

impl WorkItemState {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::New => "🆕",
            Self::Approved => "👍",
            Self::Committed => "🎯",
            Self::Active => "🔵",
            Self::Resolved => "☑️",
            Self::Closed => "✔️",
            Self::Removed => "🗑️",
            Self::Done => "✅",
            Self::Other(_) => "⚪",
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::New => "New",
            Self::Approved => "Approved",
            Self::Committed => "Committed",
            Self::Active => "Active",
            Self::Resolved => "Resolved",
            Self::Closed => "Closed",
            Self::Removed => "Removed",
            Self::Done => "Done",
            Self::Other(s) => s,
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Self::New | Self::Approved => Color::Gray,
            Self::Committed => Color::Blue,
            Self::Active => Color::Cyan,
            Self::Resolved => Color::Yellow,
            Self::Closed | Self::Done => Color::Green,
            Self::Removed => Color::DarkGray,
            Self::Other(_) => Color::White,
        }
    }
}

/// Known rich text fields in Azure DevOps
const RICH_TEXT_FIELDS: &[(&str, &str)] = &[
    ("System.Description", "Description"),
    (
        "Microsoft.VSTS.Common.AcceptanceCriteria",
        "Acceptance Criteria",
    ),
    ("Microsoft.VSTS.TCM.ReproSteps", "Repro Steps"),
    ("Microsoft.VSTS.TCM.SystemInfo", "System Info"),
    ("Microsoft.VSTS.Common.Resolution", "Resolution"),
    ("Microsoft.VSTS.Build.FoundIn", "Found In"),
    ("Microsoft.VSTS.Build.IntegrationBuild", "Integration Build"),
];

impl WorkItem {
    pub fn from_json(json: &Value, id: u32) -> Result<Self> {
        let fields = json
            .get("fields")
            .context("Missing 'fields' in work item response")?;

        let title = fields
            .get("System.Title")
            .and_then(|v| v.as_str())
            .context("Missing 'System.Title' field")?
            .to_string();

        let work_item_type_str = fields
            .get("System.WorkItemType")
            .and_then(|v| v.as_str())
            .context("Missing 'System.WorkItemType' field")?;

        let state_str = fields
            .get("System.State")
            .and_then(|v| v.as_str())
            .context("Missing 'System.State' field")?;

        let assigned_to = fields
            .get("System.AssignedTo")
            .and_then(|v| v.get("displayName"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let url = json
            .get("_links")
            .and_then(|l| l.get("html"))
            .and_then(|h| h.get("href"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Parse tags (comma-separated string)
        let tags: Vec<String> = fields
            .get("System.Tags")
            .and_then(|v| v.as_str())
            .map(|s| {
                s.split(';')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        // Parse all rich text fields that have values
        let mut rich_text_fields = Vec::new();
        for (field_name, display_name) in RICH_TEXT_FIELDS {
            if let Some(value) = fields.get(*field_name).and_then(|v| v.as_str())
                && !value.trim().is_empty()
            {
                rich_text_fields.push(RichTextField {
                    name: display_name.to_string(),
                    value: value.to_string(),
                });
            }
        }

        Ok(Self::from_parts(WorkItemParts {
            id,
            title,
            work_item_type: work_item_type_str,
            state: state_str,
            assigned_to,
            url,
            tags,
            rich_text_fields,
        }))
    }

    pub(crate) fn from_parts(parts: WorkItemParts<'_>) -> Self {
        Self {
            id: parts.id,
            title: parts.title,
            work_item_type: parts.work_item_type.parse().unwrap(),
            state: parts.state.parse().unwrap(),
            assigned_to: parts.assigned_to,
            url: parts.url,
            tags: parts.tags,
            rich_text_fields: parts.rich_text_fields,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn from_json_extracts_core_fields_and_rich_text() {
        let json = json!({
            "fields": {
                "System.Title": "Fix login flow",
                "System.WorkItemType": "Bug",
                "System.State": "Active",
                "System.AssignedTo": {
                    "displayName": "Ada Lovelace"
                },
                "System.Tags": "Auth; Urgent ; ;",
                "System.Description": "<p>Broken on mobile</p>",
                "Microsoft.VSTS.Common.AcceptanceCriteria": "<p>Works again</p>"
            },
            "_links": {
                "html": {
                    "href": "https://example.test/items/123"
                }
            }
        });

        let work_item = WorkItem::from_json(&json, 123).expect("work item should parse");

        assert_eq!(work_item.id, 123);
        assert_eq!(work_item.title, "Fix login flow");
        assert_eq!(work_item.work_item_type.display_name(), "Bug");
        assert_eq!(work_item.state.display_name(), "Active");
        assert_eq!(work_item.assigned_to.as_deref(), Some("Ada Lovelace"));
        assert_eq!(
            work_item.url.as_deref(),
            Some("https://example.test/items/123")
        );
        assert_eq!(work_item.tags, vec!["Auth", "Urgent"]);
        assert_eq!(work_item.rich_text_fields.len(), 2);
        assert_eq!(work_item.rich_text_fields[0].name, "Description");
        assert_eq!(work_item.rich_text_fields[1].name, "Acceptance Criteria");
    }

    #[test]
    fn from_json_ignores_missing_optional_fields_and_blank_rich_text() {
        let json = json!({
            "fields": {
                "System.Title": "Add reporting",
                "System.WorkItemType": "Feature",
                "System.State": "New",
                "System.Description": "   ",
                "Microsoft.VSTS.Common.AcceptanceCriteria": ""
            }
        });

        let work_item = WorkItem::from_json(&json, 55).expect("work item should parse");

        assert_eq!(work_item.assigned_to, None);
        assert_eq!(work_item.url, None);
        assert!(work_item.tags.is_empty());
        assert!(work_item.rich_text_fields.is_empty());
    }

    #[test]
    fn from_json_requires_title() {
        let json = json!({
            "fields": {
                "System.WorkItemType": "Task",
                "System.State": "Committed"
            }
        });

        let error = WorkItem::from_json(&json, 9).expect_err("missing title should fail");

        assert_eq!(error.to_string(), "Missing 'System.Title' field");
    }
}
