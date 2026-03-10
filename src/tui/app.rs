use crate::azure_devops::WorkItem;
use crate::git::{BranchScope, BranchStatus};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// Branch info with optional work item
#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub key: String,
    pub display_name: String,
    pub branch_name: String,
    pub remote_name: Option<String>,
    pub scope: BranchScope,
    pub work_item_id: Option<u32>,
    pub is_current: bool,
    pub is_protected: bool,
    pub is_stale: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchView {
    Local,
    Remote,
}

impl BranchView {
    pub fn toggle(self) -> Self {
        match self {
            Self::Local => Self::Remote,
            Self::Remote => Self::Local,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::Remote => "Remote",
        }
    }
}

/// Application mode for modal dialogs
#[derive(Debug, Clone)]
pub enum AppMode {
    Normal,
    ConfirmDelete(BranchInfo),
    ErrorPopup(String),
}

/// Deleted branch info for summary on exit
#[derive(Debug, Clone)]
pub struct DeletedBranch {
    pub name: String,
    pub restore_hint: Option<String>,
}

/// Status message with expiration
#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub text: String,
    pub is_error: bool,
    pub expires_at: Instant,
}

/// Work item fetch status
#[derive(Debug, Clone)]
pub enum WorkItemStatus {
    NotFetched,
    Loading,
    Loaded(WorkItem),
    Error(String),
}

#[derive(Debug, Clone, Default)]
pub enum RemoteFreshness {
    #[default]
    NotChecked,
    Checking,
    Checked,
    Error(String),
}

/// Application state
pub struct App {
    pub branches: Vec<BranchInfo>,
    pub active_view: BranchView,
    pub local_selected_index: usize,
    pub remote_selected_index: usize,
    pub work_items: HashMap<u32, WorkItemStatus>,
    pub branch_statuses: HashMap<String, BranchStatus>,
    pub should_quit: bool,
    pub scroll_offset: u16,
    pub content_height: u16,
    pub visible_height: u16,
    pub mode: AppMode,
    pub status_message: Option<StatusMessage>,
    pub deleted_branches: Vec<DeletedBranch>,
    pub protected_patterns: Vec<String>,
    pub show_protected: bool,
    pub remote_freshness: RemoteFreshness,
}

impl App {
    pub fn new(branches: Vec<BranchInfo>, protected_patterns: Vec<String>) -> Self {
        Self {
            branches,
            active_view: BranchView::Local,
            local_selected_index: 0,
            remote_selected_index: 0,
            work_items: HashMap::new(),
            branch_statuses: HashMap::new(),
            should_quit: false,
            scroll_offset: 0,
            content_height: 0,
            visible_height: 0,
            mode: AppMode::Normal,
            status_message: None,
            deleted_branches: Vec::new(),
            protected_patterns,
            show_protected: false,
            remote_freshness: RemoteFreshness::NotChecked,
        }
    }

    pub fn selected_branch(&self) -> Option<&BranchInfo> {
        let visible = self.visible_branches();
        visible.get(self.selected_index()).copied()
    }

    pub fn selected_work_item_id(&self) -> Option<u32> {
        self.selected_branch().and_then(|b| b.work_item_id)
    }

    pub fn visible_branches(&self) -> Vec<&BranchInfo> {
        self.branches
            .iter()
            .filter(|b| self.matches_active_view(b))
            .filter(|b| self.show_protected || b.is_current || !b.is_protected)
            .collect()
    }

    pub fn visible_count(&self) -> usize {
        self.visible_branches().len()
    }

    pub fn selected_index(&self) -> usize {
        match self.active_view {
            BranchView::Local => self.local_selected_index,
            BranchView::Remote => self.remote_selected_index,
        }
    }

    pub fn next(&mut self) {
        let count = self.visible_count();
        if count > 0 {
            let next = (self.selected_index() + 1) % count;
            self.set_selected_index(next);
            self.scroll_offset = 0;
        }
    }

    pub fn previous(&mut self) {
        let count = self.visible_count();
        if count > 0 {
            let next = if self.selected_index() == 0 {
                count - 1
            } else {
                self.selected_index() - 1
            };
            self.set_selected_index(next);
            self.scroll_offset = 0;
        }
    }

    pub fn toggle_show_protected(&mut self) {
        let selected_key = self.selected_branch().map(|b| b.key.clone());
        self.show_protected = !self.show_protected;

        if let Some(key) = selected_key
            && let Some(new_idx) = self.visible_branches().iter().position(|b| b.key == key)
        {
            self.set_selected_index(new_idx);
            return;
        }

        self.clamp_selected_index();
    }

    pub fn toggle_view(&mut self) {
        self.active_view = self.active_view.toggle();
        self.scroll_offset = 0;
        self.clamp_selected_index();
    }

    pub fn should_check_remote_freshness(&self) -> bool {
        self.active_view == BranchView::Remote
            && matches!(self.remote_freshness, RemoteFreshness::NotChecked)
    }

    pub fn set_remote_freshness_checking(&mut self) {
        self.remote_freshness = RemoteFreshness::Checking;
    }

    pub fn set_remote_freshness(&mut self, live_branches: HashSet<String>) {
        for branch in &mut self.branches {
            if branch.scope == BranchScope::Remote {
                branch.is_stale = !live_branches.contains(&branch.branch_name);
            }
        }
        self.remote_freshness = RemoteFreshness::Checked;
    }

    pub fn set_remote_freshness_error(&mut self, error: String) {
        self.remote_freshness = RemoteFreshness::Error(error);
    }

    pub fn remote_freshness_message(&self) -> Option<&str> {
        match &self.remote_freshness {
            RemoteFreshness::Checking => Some("Checking origin..."),
            RemoteFreshness::Error(error) => Some(error.as_str()),
            _ => None,
        }
    }

    pub fn scroll_down(&mut self, amount: u16) {
        let max_scroll = self.content_height.saturating_sub(self.visible_height);
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

    pub fn reset_work_item(&mut self, id: u32) {
        self.work_items.remove(&id);
    }

    pub fn current_branch_has_work_item(&self) -> bool {
        self.selected_branch()
            .and_then(|b| b.work_item_id)
            .is_some()
    }

    pub fn get_branch_status(&self, key: &str) -> Option<&BranchStatus> {
        self.branch_statuses.get(key)
    }

    pub fn set_branch_status(&mut self, key: String, status: BranchStatus) {
        self.branch_statuses.insert(key, status);
    }

    pub fn needs_branch_status(&self, key: &str) -> bool {
        !self.branch_statuses.contains_key(key)
    }

    pub fn enter_delete_mode(&mut self) {
        if let Some(branch) = self.selected_branch() {
            self.mode = AppMode::ConfirmDelete(branch.clone());
        }
    }

    pub fn show_error_popup(&mut self, message: String) {
        self.mode = AppMode::ErrorPopup(message);
    }

    pub fn cancel_mode(&mut self) {
        self.mode = AppMode::Normal;
    }

    pub fn is_normal_mode(&self) -> bool {
        matches!(self.mode, AppMode::Normal)
    }

    pub fn set_status_message(&mut self, text: String, is_error: bool, duration_secs: u64) {
        self.status_message = Some(StatusMessage {
            text,
            is_error,
            expires_at: Instant::now() + std::time::Duration::from_secs(duration_secs),
        });
    }

    pub fn get_status_message(&self) -> Option<&StatusMessage> {
        self.status_message
            .as_ref()
            .filter(|m| m.expires_at > Instant::now())
    }

    pub fn clear_expired_status(&mut self) {
        if let Some(ref msg) = self.status_message
            && msg.expires_at <= Instant::now()
        {
            self.status_message = None;
        }
    }

    pub fn record_deleted_branch(&mut self, name: String, restore_hint: Option<String>) {
        self.deleted_branches
            .push(DeletedBranch { name, restore_hint });
    }

    pub fn remove_branch(&mut self, key: &str) {
        if let Some(pos) = self.branches.iter().position(|b| b.key == key) {
            self.branches.remove(pos);
            self.clamp_selected_index();
        }
    }

    pub fn update_current_branch(&mut self, new_current_branch: &str) {
        for branch in &mut self.branches {
            branch.is_current =
                branch.scope == BranchScope::Local && branch.branch_name == new_current_branch;
        }
    }

    pub fn can_delete_selected(&self) -> Result<(), String> {
        let Some(branch) = self.selected_branch() else {
            return Err("No branch selected".to_string());
        };

        if branch.is_current {
            return Err("Cannot delete the current branch".to_string());
        }

        if branch.is_protected {
            return Err(format!(
                "Cannot delete protected branch '{}'",
                branch.display_name
            ));
        }

        Ok(())
    }

    fn matches_active_view(&self, branch: &BranchInfo) -> bool {
        matches!(
            (self.active_view, branch.scope),
            (BranchView::Local, BranchScope::Local) | (BranchView::Remote, BranchScope::Remote)
        )
    }

    fn set_selected_index(&mut self, index: usize) {
        match self.active_view {
            BranchView::Local => self.local_selected_index = index,
            BranchView::Remote => self.remote_selected_index = index,
        }
    }

    fn clamp_selected_index(&mut self) {
        let count = self.visible_count();
        let next = if count == 0 {
            0
        } else {
            self.selected_index().min(count - 1)
        };
        self.set_selected_index(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(
        key: &str,
        display_name: &str,
        branch_name: &str,
        scope: BranchScope,
        is_current: bool,
        is_protected: bool,
        work_item_id: Option<u32>,
    ) -> BranchInfo {
        BranchInfo {
            key: key.to_string(),
            display_name: display_name.to_string(),
            branch_name: branch_name.to_string(),
            remote_name: (scope == BranchScope::Remote).then(|| "origin".to_string()),
            scope,
            work_item_id,
            is_current,
            is_protected,
            is_stale: false,
        }
    }

    fn create_test_branches() -> Vec<BranchInfo> {
        vec![
            branch(
                "refs/heads/main",
                "main",
                "main",
                BranchScope::Local,
                true,
                true,
                None,
            ),
            branch(
                "refs/heads/feature/123",
                "feature/123",
                "feature/123",
                BranchScope::Local,
                false,
                false,
                Some(123),
            ),
            branch(
                "refs/remotes/origin/feature/456",
                "origin/feature/456",
                "feature/456",
                BranchScope::Remote,
                false,
                false,
                Some(456),
            ),
        ]
    }

    #[test]
    fn test_navigation_wraps() {
        let branches = create_test_branches();
        let mut app = App::new(branches, vec!["main".to_string(), "master".to_string()]);

        assert_eq!(app.selected_index(), 0);

        app.previous();
        assert_eq!(app.selected_index(), 1);

        app.next();
        assert_eq!(app.selected_index(), 0);
    }

    #[test]
    fn test_navigation_movement() {
        let branches = create_test_branches();
        let mut app = App::new(branches, vec![]);

        app.next();
        assert_eq!(app.selected_index(), 1);

        app.previous();
        assert_eq!(app.selected_index(), 0);
    }

    #[test]
    fn test_scroll_bounds() {
        let branches = create_test_branches();
        let mut app = App::new(branches, vec![]);
        app.content_height = 50;
        app.visible_height = 20;

        app.scroll_down(10);
        assert_eq!(app.scroll_offset, 10);

        app.scroll_down(100);
        assert_eq!(app.scroll_offset, 30);

        app.scroll_up(10);
        assert_eq!(app.scroll_offset, 20);

        app.scroll_up(100);
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn test_reset_scroll_on_nav() {
        let branches = create_test_branches();
        let mut app = App::new(branches, vec![]);
        app.content_height = 50;
        app.visible_height = 20;

        app.scroll_down(10);
        app.next();

        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn test_visible_branches_filters_protected_in_active_view() {
        let branches = create_test_branches();
        let mut app = App::new(branches, vec![]);

        assert_eq!(app.visible_count(), 2);

        app.branches[0].is_current = false;
        assert_eq!(app.visible_count(), 1);

        app.toggle_show_protected();
        assert_eq!(app.visible_count(), 2);
    }

    #[test]
    fn test_toggle_view_keeps_separate_selection() {
        let branches = vec![
            branch(
                "refs/heads/main",
                "main",
                "main",
                BranchScope::Local,
                false,
                true,
                None,
            ),
            branch(
                "refs/heads/feature/1",
                "feature/1",
                "feature/1",
                BranchScope::Local,
                true,
                false,
                Some(1),
            ),
            branch(
                "refs/heads/feature/4",
                "feature/4",
                "feature/4",
                BranchScope::Local,
                false,
                false,
                Some(4),
            ),
            branch(
                "refs/remotes/origin/feature/2",
                "origin/feature/2",
                "feature/2",
                BranchScope::Remote,
                false,
                false,
                Some(2),
            ),
            branch(
                "refs/remotes/origin/feature/3",
                "origin/feature/3",
                "feature/3",
                BranchScope::Remote,
                false,
                false,
                Some(3),
            ),
        ];
        let mut app = App::new(branches, vec![]);

        app.local_selected_index = 1;
        app.toggle_view();
        assert_eq!(app.active_view, BranchView::Remote);
        assert_eq!(app.selected_index(), 0);

        app.next();
        assert_eq!(app.selected_index(), 1);

        app.toggle_view();
        assert_eq!(app.active_view, BranchView::Local);
        assert_eq!(app.selected_index(), 1);
    }

    #[test]
    fn test_remove_branch_clamps_to_visible_count() {
        let branches = vec![
            branch(
                "refs/heads/main",
                "main",
                "main",
                BranchScope::Local,
                false,
                true,
                None,
            ),
            branch(
                "refs/heads/feature/1",
                "feature/1",
                "feature/1",
                BranchScope::Local,
                true,
                false,
                Some(1),
            ),
            branch(
                "refs/heads/feature/2",
                "feature/2",
                "feature/2",
                BranchScope::Local,
                false,
                false,
                Some(2),
            ),
        ];
        let mut app = App::new(branches, vec![]);

        app.local_selected_index = 1;
        app.remove_branch("refs/heads/feature/2");

        assert_eq!(app.visible_count(), 1);
        assert_eq!(app.selected_index(), 0);
    }

    #[test]
    fn test_set_remote_freshness_marks_missing_remote_branches_stale() {
        let mut app = App::new(create_test_branches(), vec![]);
        let live = HashSet::from(["feature/other".to_string()]);

        app.set_remote_freshness(live);

        let remote_branch = app
            .branches
            .iter()
            .find(|branch| branch.scope == BranchScope::Remote)
            .expect("remote branch exists");
        assert!(remote_branch.is_stale);
    }
}
