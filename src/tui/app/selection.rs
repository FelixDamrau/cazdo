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
        if self.worktree_view {
            self.next_worktree();
            return;
        }
        let count = self.visible_count();
        if count > 0 {
            let next = (self.selected_index() + 1) % count;
            self.set_selected_index(next);
            self.scroll_offset = 0;
        }
    }

    pub(super) fn previous(&mut self) {
        if self.worktree_view {
            self.previous_worktree();
            return;
        }
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
        self.select_by_key_or(selected_key.as_deref(), OnMiss::Clamp);
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

    pub(super) fn toggle_worktree_view(&mut self) {
        self.worktree_view = !self.worktree_view;
        self.scroll_offset = 0;
        if self.worktree_view {
            self.worktree_selected_index = self
                .worktree_selected_index
                .min(self.worktrees.len().saturating_sub(1));
        } else {
            self.clamp_selected_index();
        }
    }

    fn next_worktree(&mut self) {
        if !self.worktrees.is_empty() {
            self.worktree_selected_index =
                (self.worktree_selected_index + 1) % self.worktrees.len();
            self.scroll_offset = 0;
        }
    }

    fn previous_worktree(&mut self) {
        if !self.worktrees.is_empty() {
            self.worktree_selected_index = if self.worktree_selected_index == 0 {
                self.worktrees.len() - 1
            } else {
                self.worktree_selected_index - 1
            };
            self.scroll_offset = 0;
        }
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

    pub(super) fn select_by_key_or(&mut self, target: Option<&str>, on_miss: OnMiss) {
        let keys: Vec<&str> = self
            .visible_branches()
            .iter()
            .map(|branch| branch.key.as_str())
            .collect();
        let idx = resolve_selection(target, &keys, self.selected_index(), on_miss);
        self.set_selected_index(idx);
    }
}

/// Fallback when the target key is no longer visible.
pub(super) enum OnMiss {
    First,
    Clamp,
}

fn resolve_selection(
    target: Option<&str>,
    visible_keys: &[&str],
    current: usize,
    on_miss: OnMiss,
) -> usize {
    if let Some(key) = target
        && let Some(idx) = visible_keys.iter().position(|k| *k == key)
    {
        return idx;
    }
    match on_miss {
        OnMiss::First => 0,
        OnMiss::Clamp => current.min(visible_keys.len().saturating_sub(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::testing::{branch, create_test_branches};

    // --- OnMiss policy -------------------------------------------------------

    #[test]
    fn resolves_to_the_targets_position_when_visible() {
        let keys = ["a", "b", "c"];
        assert_eq!(resolve_selection(Some("b"), &keys, 0, OnMiss::First), 1);
        assert_eq!(resolve_selection(Some("c"), &keys, 0, OnMiss::Clamp), 2);
    }

    #[test]
    fn misses_fall_back_to_first_under_first_policy() {
        let keys = ["a", "b", "c"];
        assert_eq!(resolve_selection(Some("x"), &keys, 2, OnMiss::First), 0);
        assert_eq!(resolve_selection(None, &keys, 2, OnMiss::First), 0);
    }

    #[test]
    fn misses_clamp_the_current_index_under_clamp_policy() {
        let keys = ["a", "b"];
        assert_eq!(resolve_selection(Some("x"), &keys, 3, OnMiss::Clamp), 1);
        assert_eq!(resolve_selection(None, &keys, 3, OnMiss::Clamp), 1);
        assert_eq!(resolve_selection(None, &keys, 0, OnMiss::Clamp), 0);
    }

    #[test]
    fn empty_visible_list_resolves_to_zero_for_both_policies() {
        let keys: [&str; 0] = [];
        assert_eq!(resolve_selection(Some("a"), &keys, 5, OnMiss::First), 0);
        assert_eq!(resolve_selection(Some("a"), &keys, 5, OnMiss::Clamp), 0);
        assert_eq!(resolve_selection(None, &keys, 5, OnMiss::Clamp), 0);
    }

    // --- visible_branches ----------------------------------------------------

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
    fn test_visible_branches_hides_protected_remotes_by_default() {
        let branches = vec![
            branch(
                "refs/remotes/origin/main",
                "origin/main",
                "main",
                BranchScope::Remote,
                false,
                true,
                None,
            ),
            branch(
                "refs/remotes/origin/feature/1",
                "origin/feature/1",
                "feature/1",
                BranchScope::Remote,
                false,
                false,
                Some(1),
            ),
        ];
        let mut app = App::new(branches, vec![]);
        app.active_view = BranchView::Remote;

        // protected remote is hidden by default since it's not current
        assert_eq!(app.visible_count(), 1);

        app.toggle_show_protected();
        assert_eq!(app.visible_count(), 2);
    }

    #[test]
    fn test_has_hidden_branches_in_active_view_tracks_filtered_protected_branches() {
        let branches = vec![branch(
            "refs/remotes/origin/main",
            "origin/main",
            "main",
            BranchScope::Remote,
            false,
            true,
            None,
        )];
        let mut app = App::new(branches, vec![]);
        app.active_view = BranchView::Remote;

        assert!(app.has_hidden_branches_in_active_view());

        app.toggle_show_protected();
        assert!(!app.has_hidden_branches_in_active_view());
    }

    // --- navigation ----------------------------------------------------------

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
    fn selected_branch_remains_available_in_worktree_view() {
        let branch = BranchInfo {
            key: "refs/heads/feature/117".to_string(),
            display_name: "feature/117".to_string(),
            branch_name: "feature/117".to_string(),
            remote_name: None,
            scope: BranchScope::Local,
            work_item_id: Some(117),
            is_current: false,
            is_protected: false,
            is_stale: false,
        };
        let mut app = App::new(vec![branch], vec![]);

        app.update(Msg::ToggleWorktreeView);

        assert!(app.is_worktree_view());
        assert_eq!(
            app.selected_branch().map(|branch| branch.key.as_str()),
            Some("refs/heads/feature/117")
        );
    }

    // --- selection after removal ---------------------------------------------

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
    fn test_remove_branch_keeps_selection_on_previous_visible_branch() {
        let mut app = App::new(
            vec![
                branch(
                    "refs/heads/feature/1",
                    "feature/1",
                    "feature/1",
                    BranchScope::Local,
                    false,
                    false,
                    None,
                ),
                branch(
                    "refs/heads/feature/2",
                    "feature/2",
                    "feature/2",
                    BranchScope::Local,
                    false,
                    false,
                    None,
                ),
            ],
            vec![],
        );
        app.set_selected_index_for_test(1);

        app.remove_branch("refs/heads/feature/2");

        assert_eq!(app.selected_index(), 0);
        assert_eq!(
            app.selected_branch().expect("remaining branch").branch_name,
            "feature/1"
        );
    }
}
