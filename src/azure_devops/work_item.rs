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

    pub fn display_name(&self) -> String {
        match self {
            Self::Bug => "Bug".to_string(),
            Self::ProductBacklogItem => "Product Backlog Item".to_string(),
            Self::UserStory => "User Story".to_string(),
            Self::Task => "Task".to_string(),
            Self::Feature => "Feature".to_string(),
            Self::Epic => "Epic".to_string(),
            Self::Other(s) => s.clone(),
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

    pub fn display_name(&self) -> String {
        match self {
            Self::New => "New".to_string(),
            Self::Approved => "Approved".to_string(),
            Self::Committed => "Committed".to_string(),
            Self::Active => "Active".to_string(),
            Self::Resolved => "Resolved".to_string(),
            Self::Closed => "Closed".to_string(),
            Self::Removed => "Removed".to_string(),
            Self::Done => "Done".to_string(),
            Self::Other(s) => s.clone(),
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

        Ok(Self {
            id,
            title,
            work_item_type: work_item_type_str.parse().unwrap(),
            state: state_str.parse().unwrap(),
            assigned_to,
            url,
            tags,
            rich_text_fields,
        })
    }
}
