use ratatui::style::Color;
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

impl WorkItem {
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

