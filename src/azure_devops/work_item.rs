use anyhow::{Context, Result};
use serde_json::Value;

#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields will be used in future features
pub struct WorkItem {
    pub id: u32,
    pub title: String,
    pub work_item_type: WorkItemType,
    pub state: WorkItemState,
    pub assigned_to: Option<String>,
    pub url: Option<String>,
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

impl WorkItemType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "bug" => Self::Bug,
            "product backlog item" => Self::ProductBacklogItem,
            "user story" => Self::UserStory,
            "task" => Self::Task,
            "feature" => Self::Feature,
            "epic" => Self::Epic,
            _ => Self::Other(s.to_string()),
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Bug => "🐛",
            Self::ProductBacklogItem => "📋",
            Self::UserStory => "📖",
            Self::Task => "📌",
            Self::Feature => "🎯",
            Self::Epic => "🏔️",
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
    Active,
    Resolved,
    Closed,
    Removed,
    Done,
    Other(String),
}

impl WorkItemState {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "new" => Self::New,
            "active" => Self::Active,
            "resolved" => Self::Resolved,
            "closed" => Self::Closed,
            "removed" => Self::Removed,
            "done" => Self::Done,
            _ => Self::Other(s.to_string()),
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::New => "🆕",
            Self::Active => "🔵",
            Self::Resolved => "✅",
            Self::Closed => "✔️",
            Self::Removed => "🗑️",
            Self::Done => "✅",
            Self::Other(_) => "⚪",
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::New => "New".to_string(),
            Self::Active => "Active".to_string(),
            Self::Resolved => "Resolved".to_string(),
            Self::Closed => "Closed".to_string(),
            Self::Removed => "Removed".to_string(),
            Self::Done => "Done".to_string(),
            Self::Other(s) => s.clone(),
        }
    }
}

impl WorkItem {
    pub fn from_json(json: &Value, id: u32) -> Result<Self> {
        let fields = json.get("fields").context("Missing 'fields' in work item response")?;

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

        Ok(Self {
            id,
            title,
            work_item_type: WorkItemType::from_str(work_item_type_str),
            state: WorkItemState::from_str(state_str),
            assigned_to,
            url,
        })
    }
}
