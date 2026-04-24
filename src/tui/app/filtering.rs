use super::*;

impl App {
    pub fn is_filter_input_mode(&self) -> bool {
        matches!(self.mode, AppMode::FilterInput)
    }

    pub fn has_active_filter(&self) -> bool {
        !self.branch_filter.trim().is_empty()
    }

    pub fn effective_branch_filter(&self) -> &str {
        if self.is_filter_input_mode() {
            &self.filter_input
        } else {
            &self.branch_filter
        }
    }

    pub fn enter_filter_input(&mut self) {
        self.filter_input_selected_key = self.selected_branch().map(|branch| branch.key.clone());
        self.filter_input = self.branch_filter.clone();
        self.mode = AppMode::FilterInput;
    }

    pub fn update_filter_input(&mut self, filter_input: String) {
        self.update_filter_query(filter_input, false);
    }

    pub fn apply_branch_filter(&mut self, filter: String) {
        self.update_filter_query(filter, true);
    }

    pub fn apply_filter_input(&mut self) {
        let filter = self.filter_input.clone();
        self.filter_input_selected_key = None;
        self.apply_branch_filter(filter);
        self.mode = AppMode::Normal;
    }

    pub fn cancel_filter_input(&mut self) {
        self.filter_input = self.branch_filter.clone();
        self.mode = AppMode::Normal;
        self.scroll_offset = 0;

        if !self
            .filter_input_selected_key
            .take()
            .as_deref()
            .is_some_and(|key| self.select_visible_branch_by_key(key))
        {
            self.clamp_selected_index();
        }
    }

    pub fn clear_branch_filter(&mut self) {
        self.apply_branch_filter(String::new());
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

    fn update_filter_query(&mut self, filter: String, applied: bool) {
        let selected_key = self.selected_branch().map(|branch| branch.key.clone());

        if applied {
            self.branch_filter = filter;
        } else {
            self.filter_input = filter;
        }

        self.scroll_offset = 0;

        if !selected_key
            .as_deref()
            .is_some_and(|key| self.select_visible_branch_by_key(key))
        {
            if self.visible_count() > 0 {
                self.set_selected_index(0);
            } else {
                self.clamp_selected_index();
            }
        }
    }
}
