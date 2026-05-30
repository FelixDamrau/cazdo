use super::*;

impl App {
    pub fn selected_branch(&self) -> Option<&BranchInfo> {
        let visible = self.visible_branches();
        visible.get(self.selected_index()).copied()
    }

    pub fn visible_branches(&self) -> Vec<&BranchInfo> {
        self.branches
            .iter()
            .filter(|branch| self.matches_active_view(branch))
            .filter(|branch| self.show_protected || branch.is_current || !branch.is_protected)
            .filter(|branch| self.branch_matches_filter(branch, self.effective_branch_filter()))
            .collect()
    }

    pub fn visible_count(&self) -> usize {
        self.visible_branches().len()
    }

    pub fn has_hidden_branches_in_active_view(&self) -> bool {
        self.branches
            .iter()
            .filter(|branch| self.matches_active_view(branch))
            .any(|branch| !self.show_protected && !branch.is_current && branch.is_protected)
    }

    pub fn selected_index(&self) -> usize {
        match self.active_view {
            BranchView::Local => self.local_selected_index,
            BranchView::Remote => self.remote_selected_index,
        }
    }

    pub fn active_view(&self) -> BranchView {
        self.active_view
    }

    /// Switch to the local view and select the branch with the given name,
    /// resetting scroll. Falls back to a clamped local selection when the
    /// branch is not currently visible.
    pub fn focus_local_branch(&mut self, branch_name: &str) {
        self.active_view = BranchView::Local;
        self.scroll_offset = 0;

        if let Some(idx) = self
            .visible_branches()
            .iter()
            .position(|branch| branch.branch_name == branch_name)
        {
            self.local_selected_index = idx;
        } else {
            self.local_selected_index = self
                .visible_branches()
                .len()
                .checked_sub(1)
                .map_or(0, |idx| self.local_selected_index.min(idx));
        }
    }

    /// Test-only setter for the active view's selection index, bypassing the
    /// clamping that navigation methods apply, so tests can exercise clamping.
    #[cfg(test)]
    pub fn set_selected_index_for_test(&mut self, index: usize) {
        self.set_selected_index(index);
    }

    pub(super) fn next(&mut self) {
        let count = self.visible_count();
        if count > 0 {
            let next = (self.selected_index() + 1) % count;
            self.set_selected_index(next);
            self.scroll_offset = 0;
        }
    }

    pub(super) fn previous(&mut self) {
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

    pub(super) fn toggle_show_protected(&mut self) {
        let selected_key = self.selected_branch().map(|branch| branch.key.clone());
        self.show_protected = !self.show_protected;

        if selected_key
            .as_deref()
            .is_some_and(|key| self.select_visible_branch_by_key(key))
        {
            return;
        }

        self.clamp_selected_index();
    }

    pub(super) fn toggle_view(&mut self) {
        self.active_view = self.active_view.toggle();
        if self.active_view == BranchView::Remote
            && matches!(self.remote_freshness, RemoteFreshness::Error(_))
        {
            self.remote_freshness = RemoteFreshness::NotChecked;
        }
        self.scroll_offset = 0;
        self.clamp_selected_index();
    }

    fn matches_active_view(&self, branch: &BranchInfo) -> bool {
        matches!(
            (self.active_view, branch.scope),
            (BranchView::Local, BranchScope::Local) | (BranchView::Remote, BranchScope::Remote)
        )
    }

    pub(super) fn set_selected_index(&mut self, index: usize) {
        match self.active_view {
            BranchView::Local => self.local_selected_index = index,
            BranchView::Remote => self.remote_selected_index = index,
        }
    }

    pub(super) fn clamp_selected_index(&mut self) {
        let count = self.visible_count();
        let next = if count == 0 {
            0
        } else {
            self.selected_index().min(count - 1)
        };
        self.set_selected_index(next);
    }

    pub(super) fn select_visible_branch_by_key(&mut self, key: &str) -> bool {
        if let Some(new_idx) = self
            .visible_branches()
            .iter()
            .position(|branch| branch.key == key)
        {
            self.set_selected_index(new_idx);
            true
        } else {
            false
        }
    }
}
