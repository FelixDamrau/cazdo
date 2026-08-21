use super::*;

impl App {
    pub fn should_check_remote_freshness(&self) -> bool {
        self.active_view == BranchView::Remote
            && matches!(self.remote_freshness, RemoteFreshness::NotChecked)
    }

    pub fn set_remote_freshness_checking(&mut self) {
        self.update(Msg::SetRemoteFreshness(RemoteFreshness::Checking));
    }

    pub fn set_remote_freshness(&mut self, live_branches: HashSet<String>) {
        self.update(Msg::SetRemoteFreshnessChecked(live_branches));
    }

    pub(super) fn apply_remote_freshness_checked(&mut self, live_branches: HashSet<String>) {
        for branch in &mut self.branches {
            if branch.scope == BranchScope::Remote {
                branch.is_stale = !live_branches.contains(&branch.branch_name);
            }
        }
        self.remote_freshness = RemoteFreshness::Checked;
    }

    pub fn set_remote_freshness_error(&mut self, error: String) {
        self.update(Msg::SetRemoteFreshness(RemoteFreshness::Error(error)));
    }

    pub fn remote_freshness_is_checking(&self) -> bool {
        matches!(self.remote_freshness, RemoteFreshness::Checking)
    }

    pub fn remote_freshness_error(&self) -> Option<&str> {
        match &self.remote_freshness {
            RemoteFreshness::Error(error) => Some(error.as_str()),
            _ => None,
        }
    }

    pub fn get_work_item_status(&self, id: u32) -> &WorkItemStatus {
        self.work_items
            .get(&id)
            .unwrap_or(&WorkItemStatus::NotFetched)
    }

    pub fn set_work_item_loading(&mut self, id: u32) {
        self.update(Msg::SetWorkItemLoading(id));
    }

    pub(super) fn apply_work_item_loading(&mut self, id: u32) {
        self.work_items.insert(id, WorkItemStatus::Loading);
    }

    pub fn set_work_item_loaded(&mut self, id: u32, work_item: WorkItem) {
        self.update(Msg::SetWorkItemLoaded { id, work_item });
    }

    pub(super) fn apply_work_item_loaded(&mut self, id: u32, work_item: WorkItem) {
        self.work_items
            .insert(id, WorkItemStatus::Loaded(work_item));
    }

    pub fn set_work_item_error(&mut self, id: u32, error: String) {
        self.update(Msg::SetWorkItemError { id, error });
    }

    pub(super) fn apply_work_item_error(&mut self, id: u32, error: String) {
        self.work_items.insert(id, WorkItemStatus::Error(error));
    }

    pub fn reset_work_item(&mut self, id: u32) {
        self.work_items.remove(&id);
    }

    pub fn current_branch_has_work_item(&self) -> bool {
        self.selected_branch()
            .and_then(|branch| branch.work_item_id)
            .is_some()
    }

    pub fn get_branch_status(&self, key: &str) -> Option<&BranchStatus> {
        self.branch_statuses
            .get(key)
            .and_then(|status| status.as_ref().ok())
    }

    pub fn get_branch_status_error(&self, key: &str) -> Option<&str> {
        self.branch_statuses
            .get(key)
            .and_then(|status| status.as_ref().err())
            .map(String::as_str)
    }

    pub fn set_branch_status(&mut self, key: String, status: BranchStatus) {
        self.update(Msg::SetBranchStatus { key, status });
    }

    pub(super) fn apply_branch_status(&mut self, key: String, status: BranchStatus) {
        self.branch_statuses.insert(key, Ok(status));
    }

    pub fn set_branch_status_error(&mut self, key: String, error: String) {
        self.update(Msg::SetBranchStatusError { key, error });
    }

    pub(super) fn apply_branch_status_error(&mut self, key: String, error: String) {
        self.branch_statuses.insert(key, Err(error));
    }

    pub fn needs_branch_status(&self, key: &str) -> bool {
        !matches!(self.branch_statuses.get(key), Some(Ok(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::RemoteStatus;
    use crate::tui::app::testing::{branch, create_test_branches};

    // --- remote freshness ----------------------------------------------------

    #[test]
    fn test_remote_freshness_is_checking() {
        let mut app = App::new(vec![], vec![]);

        assert!(!app.remote_freshness_is_checking());

        app.set_remote_freshness_checking();
        assert!(app.remote_freshness_is_checking());

        app.set_remote_freshness_error("timeout".to_string());
        assert!(!app.remote_freshness_is_checking());
    }

    #[test]
    fn test_remote_freshness_error() {
        let mut app = App::new(vec![], vec![]);

        assert_eq!(app.remote_freshness_error(), None);

        app.set_remote_freshness_error("Network timeout".to_string());
        assert_eq!(app.remote_freshness_error(), Some("Network timeout"));

        app.set_remote_freshness(HashSet::new());
        assert_eq!(app.remote_freshness_error(), None);
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

    #[test]
    fn test_set_remote_freshness_keeps_live_branches_fresh() {
        let mut app = App::new(create_test_branches(), vec![]);
        // The remote branch in create_test_branches is feature/456
        let live = HashSet::from(["feature/456".to_string()]);

        app.set_remote_freshness(live);

        let remote_branch = app
            .branches
            .iter()
            .find(|branch| branch.scope == BranchScope::Remote)
            .expect("remote branch exists");
        assert!(!remote_branch.is_stale);
    }

    #[test]
    fn test_remote_freshness_retries_after_reentering_remote_view() {
        let branches = vec![branch(
            "refs/remotes/origin/feature/1",
            "origin/feature/1",
            "feature/1",
            BranchScope::Remote,
            false,
            false,
            Some(1),
        )];
        let mut app = App::new(branches, vec![]);

        app.toggle_view();
        assert!(app.should_check_remote_freshness());

        app.set_remote_freshness_error("timeout".to_string());
        assert!(!app.should_check_remote_freshness());

        app.toggle_view();
        app.toggle_view();

        assert!(app.should_check_remote_freshness());
    }

    #[test]
    fn test_remote_freshness_does_not_reset_after_successful_reentry() {
        let branches = vec![branch(
            "refs/remotes/origin/feature/1",
            "origin/feature/1",
            "feature/1",
            BranchScope::Remote,
            false,
            false,
            Some(1),
        )];
        let mut app = App::new(branches, vec![]);

        app.toggle_view();
        app.set_remote_freshness(HashSet::from(["feature/1".to_string()]));

        app.toggle_view();
        app.toggle_view();

        assert!(!app.should_check_remote_freshness());
    }

    // --- branch status retries -----------------------------------------------

    #[test]
    fn test_needs_branch_status_retries_after_cached_error() {
        let mut app = App::new(vec![], vec![]);

        assert!(app.needs_branch_status("refs/heads/feature/1"));

        app.set_branch_status_error(
            "refs/heads/feature/1".to_string(),
            "temporary failure".to_string(),
        );

        assert!(app.needs_branch_status("refs/heads/feature/1"));
    }

    #[test]
    fn test_needs_branch_status_stops_retrying_after_success() {
        let mut app = App::new(vec![], vec![]);
        app.set_branch_status(
            "refs/heads/feature/1".to_string(),
            BranchStatus {
                remote_status: RemoteStatus::UpToDate,
                last_commit_author: None,
                last_commit_time: None,
            },
        );

        assert!(!app.needs_branch_status("refs/heads/feature/1"));
    }
}
