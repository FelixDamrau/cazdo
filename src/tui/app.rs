use crate::azure_devops::WorkItem;
use crate::git::BranchStatus;
use std::collections::HashMap;
use std::time::Instant;

/// Application mode for modal dialogs
#[derive(Debug, Clone)]
pub enum AppMode {
    Normal,
    ConfirmDelete(String), // branch name to delete
}

/// Deleted branch info for summary on exit
#[derive(Debug, Clone)]
pub struct DeletedBranch {
    pub name: String,
    pub commit_sha: String,
}

/// Status message with expiration
#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub text: String,
    pub is_error: bool,
    pub expires_at: Instant,
}

/// Branch info with optional work item
#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub work_item_id: Option<u32>,
    pub is_current: bool,
}

/// Work item fetch status
#[derive(Debug, Clone)]
pub enum WorkItemStatus {
    NotFetched,
    Loading,
    Loaded(WorkItem),
    Error(String),
}

/// Application state
pub struct App {
    pub branches: Vec<BranchInfo>,
    pub selected_index: usize,
    pub work_items: HashMap<u32, WorkItemStatus>,
    pub branch_statuses: HashMap<String, BranchStatus>,
    pub should_quit: bool,
    pub scroll_offset: u16,
    pub content_height: u16, // Total height of content for scroll bounds
    pub mode: AppMode,
    pub status_message: Option<StatusMessage>,
    pub deleted_branches: Vec<DeletedBranch>,
}

impl App {
    pub fn new(branches: Vec<BranchInfo>) -> Self {
        Self {
            branches,
            selected_index: 0,
            work_items: HashMap::new(),
            branch_statuses: HashMap::new(),
            should_quit: false,
            scroll_offset: 0,
            content_height: 0,
            mode: AppMode::Normal,
            status_message: None,
            deleted_branches: Vec::new(),
        }
    }

    pub fn selected_branch(&self) -> Option<&BranchInfo> {
        self.branches.get(self.selected_index)
    }

    pub fn next(&mut self) {
        if !self.branches.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.branches.len();
            self.scroll_offset = 0; // Reset scroll when changing branch
        }
    }

    pub fn previous(&mut self) {
        if !self.branches.is_empty() {
            self.selected_index = if self.selected_index == 0 {
                self.branches.len() - 1
            } else {
                self.selected_index - 1
            };
            self.scroll_offset = 0; // Reset scroll when changing branch
        }
    }

    pub fn scroll_down(&mut self, amount: u16, visible_height: u16) {
        let max_scroll = self.content_height.saturating_sub(visible_height);
        self.scroll_offset = (self.scroll_offset + amount).min(max_scroll);
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn set_content_height(&mut self, height: u16) {
        self.content_height = height;
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn get_work_item_status(&self, id: u32) -> &WorkItemStatus {
        self.work_items
            .get(&id)
            .unwrap_or(&WorkItemStatus::NotFetched)
    }

    pub fn set_work_item_loading(&mut self, id: u32) {
        self.work_items.insert(id, WorkItemStatus::Loading);
    }

    pub fn set_work_item_loaded(&mut self, id: u32, work_item: WorkItem) {
        self.work_items
            .insert(id, WorkItemStatus::Loaded(work_item));
    }

    pub fn set_work_item_error(&mut self, id: u32, error: String) {
        self.work_items.insert(id, WorkItemStatus::Error(error));
    }

    /// Reset a work item status to allow refresh
    pub fn reset_work_item(&mut self, id: u32) {
        self.work_items.remove(&id);
    }

    /// Check if current branch has a work item (for showing refresh hint)
    pub fn current_branch_has_work_item(&self) -> bool {
        self.selected_branch()
            .and_then(|b| b.work_item_id)
            .is_some()
    }

    /// Get cached branch status
    pub fn get_branch_status(&self, name: &str) -> Option<&BranchStatus> {
        self.branch_statuses.get(name)
    }

    /// Cache branch status
    pub fn set_branch_status(&mut self, name: String, status: BranchStatus) {
        self.branch_statuses.insert(name, status);
    }

    /// Check if branch status needs to be fetched
    pub fn needs_branch_status(&self, name: &str) -> bool {
        !self.branch_statuses.contains_key(name)
    }

    /// Enter delete confirmation mode for the selected branch
    pub fn enter_delete_mode(&mut self) {
        if let Some(branch) = self.selected_branch() {
            self.mode = AppMode::ConfirmDelete(branch.name.clone());
        }
    }

    /// Cancel any modal and return to normal mode
    pub fn cancel_mode(&mut self) {
        self.mode = AppMode::Normal;
    }

    /// Check if we're in normal mode
    pub fn is_normal_mode(&self) -> bool {
        matches!(self.mode, AppMode::Normal)
    }

    /// Set a status message that expires after a duration
    pub fn set_status_message(&mut self, text: String, is_error: bool, duration_secs: u64) {
        self.status_message = Some(StatusMessage {
            text,
            is_error,
            expires_at: Instant::now() + std::time::Duration::from_secs(duration_secs),
        });
    }

    /// Get the current status message if not expired
    pub fn get_status_message(&self) -> Option<&StatusMessage> {
        self.status_message
            .as_ref()
            .filter(|m| m.expires_at > Instant::now())
    }

    /// Clear expired status message
    pub fn clear_expired_status(&mut self) {
        if let Some(ref msg) = self.status_message
            && msg.expires_at <= Instant::now()
        {
            self.status_message = None;
        }
    }

    /// Record a deleted branch for summary on exit
    pub fn record_deleted_branch(&mut self, name: String, commit_sha: String) {
        self.deleted_branches
            .push(DeletedBranch { name, commit_sha });
    }

    /// Remove a branch from the list by name and adjust selected index
    pub fn remove_branch(&mut self, name: &str) {
        if let Some(pos) = self.branches.iter().position(|b| b.name == name) {
            self.branches.remove(pos);
            // Adjust selected index if needed
            if self.selected_index >= self.branches.len() && !self.branches.is_empty() {
                self.selected_index = self.branches.len() - 1;
            }
        }
    }

    /// Check if the selected branch can be deleted
    /// Returns Ok(()) if deletable, Err(reason) if not
    pub fn can_delete_selected(&self) -> Result<(), String> {
        let Some(branch) = self.selected_branch() else {
            return Err("No branch selected".to_string());
        };

        if branch.is_current {
            return Err("Cannot delete the current branch".to_string());
        }

        let protected = ["main", "master"];
        if protected.contains(&branch.name.as_str()) {
            return Err(format!("Cannot delete protected branch '{}'", branch.name));
        }

        Ok(())
    }
}
