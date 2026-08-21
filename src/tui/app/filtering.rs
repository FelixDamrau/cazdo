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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::testing::{branch, create_test_branches};

    // --- matching ------------------------------------------------------------

    #[test]
    fn test_visible_branches_applies_case_insensitive_token_filter() {
        let branches = vec![
            branch(
                "refs/heads/feature/123-login",
                "feature/123-login",
                "feature/123-login",
                BranchScope::Local,
                false,
                false,
                Some(123),
            ),
            branch(
                "refs/heads/bugfix/login",
                "bugfix/login",
                "bugfix/login",
                BranchScope::Local,
                false,
                false,
                None,
            ),
            branch(
                "refs/heads/feature/reports",
                "feature/reports",
                "feature/reports",
                BranchScope::Local,
                false,
                false,
                None,
            ),
        ];
        let mut app = App::new(branches, vec![]);

        app.apply_branch_filter("FEATURE login".to_string());

        let visible = app.visible_branches();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].branch_name, "feature/123-login");
    }

    #[test]
    fn test_branch_filter_ignores_extra_whitespace() {
        let branches = vec![
            branch(
                "refs/heads/feature/123-login",
                "feature/123-login",
                "feature/123-login",
                BranchScope::Local,
                false,
                false,
                Some(123),
            ),
            branch(
                "refs/heads/feature/reports",
                "feature/reports",
                "feature/reports",
                BranchScope::Local,
                false,
                false,
                None,
            ),
        ];
        let mut app = App::new(branches, vec![]);

        app.apply_branch_filter("  feature   login  ".to_string());

        let visible = app.visible_branches();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].branch_name, "feature/123-login");
    }

    // --- selection across filter changes -------------------------------------

    #[test]
    fn test_apply_branch_filter_preserves_selected_branch_when_still_visible() {
        let branches = vec![
            branch(
                "refs/heads/feature/alpha-login",
                "feature/alpha-login",
                "feature/alpha-login",
                BranchScope::Local,
                false,
                false,
                None,
            ),
            branch(
                "refs/heads/feature/beta-login",
                "feature/beta-login",
                "feature/beta-login",
                BranchScope::Local,
                false,
                false,
                None,
            ),
            branch(
                "refs/heads/chore/docs",
                "chore/docs",
                "chore/docs",
                BranchScope::Local,
                false,
                false,
                None,
            ),
        ];
        let mut app = App::new(branches, vec![]);
        app.local_selected_index = 1;

        app.apply_branch_filter("login".to_string());

        assert_eq!(app.selected_index(), 1);
        assert_eq!(
            app.selected_branch().unwrap().branch_name,
            "feature/beta-login"
        );
    }

    #[test]
    fn test_apply_branch_filter_clamps_selection_when_selected_branch_hidden() {
        let branches = vec![
            branch(
                "refs/heads/feature/alpha-login",
                "feature/alpha-login",
                "feature/alpha-login",
                BranchScope::Local,
                false,
                false,
                None,
            ),
            branch(
                "refs/heads/chore/docs",
                "chore/docs",
                "chore/docs",
                BranchScope::Local,
                false,
                false,
                None,
            ),
            branch(
                "refs/heads/feature/beta-login",
                "feature/beta-login",
                "feature/beta-login",
                BranchScope::Local,
                false,
                false,
                None,
            ),
        ];
        let mut app = App::new(branches, vec![]);
        app.local_selected_index = 1;

        app.apply_branch_filter("login".to_string());

        assert_eq!(app.selected_index(), 0);
        assert_eq!(
            app.selected_branch().unwrap().branch_name,
            "feature/alpha-login"
        );
    }

    #[test]
    fn test_apply_filter_input_preserves_selected_branch_from_filtered_preview() {
        let branches = vec![
            branch(
                "refs/heads/a",
                "a",
                "a",
                BranchScope::Local,
                false,
                false,
                None,
            ),
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
        ];
        let mut app = App::new(branches, vec![]);
        app.local_selected_index = 2;

        app.enter_filter_input();
        app.update_filter_input("feature".to_string());
        app.apply_filter_input();

        assert_eq!(app.selected_index(), 1);
        assert_eq!(app.selected_branch().unwrap().branch_name, "feature/2");
    }

    // --- filter input lifecycle ----------------------------------------------

    #[test]
    fn test_update_start_filter_enters_filter_input_with_active_filter() {
        let mut app = App::new(
            vec![branch(
                "refs/heads/feature/login",
                "feature/login",
                "feature/login",
                BranchScope::Local,
                false,
                false,
                None,
            )],
            vec![],
        );
        app.apply_branch_filter("feature".to_string());

        app.update(Msg::StartFilter);

        assert!(app.is_editing_filter());
        assert_eq!(app.filter_input(), "feature");
    }

    #[test]
    fn test_update_set_filter_input_keeps_applied_filter_separate() {
        let mut app = App::new(
            vec![
                branch(
                    "refs/heads/feature/login",
                    "feature/login",
                    "feature/login",
                    BranchScope::Local,
                    false,
                    false,
                    None,
                ),
                branch(
                    "refs/heads/chore/docs",
                    "chore/docs",
                    "chore/docs",
                    BranchScope::Local,
                    false,
                    false,
                    None,
                ),
            ],
            vec![],
        );
        app.apply_branch_filter("feature".to_string());
        app.update(Msg::StartFilter);

        app.update(Msg::SetFilterInput("docs".to_string()));

        assert_eq!(app.branch_filter(), "feature");
        assert_eq!(app.filter_input(), "docs");
        assert_eq!(app.visible_branches()[0].branch_name, "chore/docs");
    }

    #[test]
    fn test_update_apply_filter_applies_draft_and_exits_filter_input() {
        let mut app = App::new(vec![create_test_branches()[1].clone()], vec![]);
        app.update(Msg::StartFilter);
        app.update(Msg::SetFilterInput("feature login".to_string()));

        app.update(Msg::ApplyFilter);

        assert!(!app.is_editing_filter());
        assert_eq!(app.branch_filter(), "feature login");
        assert_eq!(app.effective_branch_filter(), "feature login");
    }

    #[test]
    fn test_update_cancel_filter_discards_draft_without_changing_applied_filter() {
        let mut app = App::new(vec![create_test_branches()[1].clone()], vec![]);
        app.apply_branch_filter("feature".to_string());
        app.update(Msg::StartFilter);
        app.update(Msg::SetFilterInput("docs".to_string()));

        app.update(Msg::CancelFilter);

        assert!(!app.is_editing_filter());
        assert_eq!(app.branch_filter(), "feature");
        assert_eq!(app.effective_branch_filter(), "feature");
    }

    #[test]
    fn test_update_clear_filter_clears_applied_filter_and_resets_scroll() {
        let mut app = App::new(vec![create_test_branches()[1].clone()], vec![]);
        app.apply_branch_filter("feature".to_string());
        app.scroll_offset = 3;

        app.update(Msg::ClearFilter);

        assert!(app.branch_filter().is_empty());
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn test_toggle_view_keeps_shared_branch_filter() {
        let branches = vec![
            branch(
                "refs/heads/feature/login",
                "feature/login",
                "feature/login",
                BranchScope::Local,
                false,
                false,
                None,
            ),
            branch(
                "refs/heads/feature/reports",
                "feature/reports",
                "feature/reports",
                BranchScope::Local,
                false,
                false,
                None,
            ),
            branch(
                "refs/remotes/origin/feature/login",
                "origin/feature/login",
                "feature/login",
                BranchScope::Remote,
                false,
                false,
                None,
            ),
            branch(
                "refs/remotes/origin/feature/reports",
                "origin/feature/reports",
                "feature/reports",
                BranchScope::Remote,
                false,
                false,
                None,
            ),
        ];
        let mut app = App::new(branches, vec![]);

        app.apply_branch_filter("login".to_string());
        assert_eq!(app.visible_count(), 1);

        app.toggle_view();

        let visible = app.visible_branches();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].branch_name, "feature/login");
        assert_eq!(app.branch_filter(), "login");
    }
}
