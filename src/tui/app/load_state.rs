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

    pub fn set_work_item_loaded(&mut self, id: u32, work_item: WorkItem) {
        self.update(Msg::SetWorkItemLoaded { id, work_item });
    }

    pub fn set_work_item_error(&mut self, id: u32, error: String) {
        self.update(Msg::SetWorkItemError { id, error });
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

    pub fn set_branch_status_error(&mut self, key: String, error: String) {
        self.update(Msg::SetBranchStatusError { key, error });
    }

    pub fn needs_branch_status(&self, key: &str) -> bool {
        !matches!(self.branch_statuses.get(key), Some(Ok(_)))
    }
}
