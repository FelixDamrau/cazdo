use super::*;

impl App {
    pub fn is_editing_filter(&self) -> bool {
        self.filter.is_editing()
    }

    pub fn has_active_filter(&self) -> bool {
        self.filter.has_active_filter()
    }

    pub fn effective_branch_filter(&self) -> &str {
        self.filter.effective_query()
    }

    pub fn filter_input(&self) -> &str {
        self.filter.draft()
    }

    #[cfg(test)]
    pub fn branch_filter(&self) -> &str {
        self.filter.applied_query()
    }

    pub fn enter_filter_input(&mut self) {
        let anchor = self.selected_branch().map(|branch| branch.key.clone());
        self.filter.enter(anchor);
    }

    pub fn update_filter_input(&mut self, filter_input: String) {
        let selected_key = self.selected_branch().map(|branch| branch.key.clone());
        self.filter.set_draft(filter_input);
        self.reselect_or_first(selected_key);
    }

    #[cfg(test)]
    pub fn apply_branch_filter(&mut self, filter: String) {
        let selected_key = self.selected_branch().map(|branch| branch.key.clone());
        self.filter = BranchFilter::Inactive { query: filter };
        self.reselect_or_first(selected_key);
    }

    pub fn apply_filter_input(&mut self) {
        let selected_key = self.selected_branch().map(|branch| branch.key.clone());
        self.filter.apply();
        self.reselect_or_first(selected_key);
    }

    pub fn cancel_filter_input(&mut self) {
        let restore_anchor = self.filter.cancel();
        self.scroll_offset = 0;
        self.select_by_key_or(restore_anchor.as_deref(), OnMiss::Clamp);
    }

    pub fn clear_branch_filter(&mut self) {
        let selected_key = self.selected_branch().map(|branch| branch.key.clone());
        self.filter.clear();
        self.reselect_or_first(selected_key);
    }

    pub(super) fn branch_matches_filter(&self, branch: &BranchInfo, filter: &str) -> bool {
        let filter = filter.trim();
        if filter.is_empty() {
            return true;
        }

        let branch_name = branch.display_name.to_ascii_lowercase();
        filter
            .split_whitespace()
            .map(|token| token.to_ascii_lowercase())
            .all(|token| branch_name.contains(&token))
    }

    /// After a filter change: keep the selected branch, else fall back to first
    /// (cancel clamps instead).
    fn reselect_or_first(&mut self, selected_key: Option<String>) {
        self.scroll_offset = 0;
        self.select_by_key_or(selected_key.as_deref(), OnMiss::First);
    }
}
