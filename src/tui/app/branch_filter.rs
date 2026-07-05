/// A draft and restore anchor can only exist while the editor is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchFilter {
    Inactive {
        query: String,
    },
    Editing {
        /// Applied filter, retained so `cancel` can revert to it.
        query: String,
        draft: String,
        restore_anchor: Option<String>,
    },
}

impl Default for BranchFilter {
    fn default() -> Self {
        BranchFilter::Inactive {
            query: String::new(),
        }
    }
}

impl BranchFilter {
    pub fn effective_query(&self) -> &str {
        match self {
            BranchFilter::Inactive { query } => query,
            BranchFilter::Editing { draft, .. } => draft,
        }
    }

    pub fn applied_query(&self) -> &str {
        match self {
            BranchFilter::Inactive { query } | BranchFilter::Editing { query, .. } => query,
        }
    }

    pub fn draft(&self) -> &str {
        match self {
            BranchFilter::Editing { draft, .. } => draft,
            BranchFilter::Inactive { .. } => "",
        }
    }

    pub fn is_editing(&self) -> bool {
        matches!(self, BranchFilter::Editing { .. })
    }

    pub fn has_active_filter(&self) -> bool {
        !self.applied_query().trim().is_empty()
    }

    pub fn enter(&mut self, restore_anchor: Option<String>) {
        *self = match std::mem::take(self) {
            BranchFilter::Inactive { query } => BranchFilter::Editing {
                draft: query.clone(),
                query,
                restore_anchor,
            },
            editing => editing,
        };
    }

    pub fn set_draft(&mut self, draft: String) {
        if let BranchFilter::Editing { draft: slot, .. } = self {
            *slot = draft;
        }
    }

    pub fn apply(&mut self) {
        *self = match std::mem::take(self) {
            BranchFilter::Editing { draft, .. } => BranchFilter::Inactive { query: draft },
            inactive => inactive,
        };
    }

    /// Returns the restore anchor so the caller can restore selection.
    pub fn cancel(&mut self) -> Option<String> {
        match std::mem::take(self) {
            BranchFilter::Editing {
                query,
                restore_anchor,
                ..
            } => {
                *self = BranchFilter::Inactive { query };
                restore_anchor
            }
            inactive => {
                *self = inactive;
                None
            }
        }
    }

    pub fn clear(&mut self) {
        *self = BranchFilter::Inactive {
            query: String::new(),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editing(query: &str, draft: &str, anchor: Option<&str>) -> BranchFilter {
        BranchFilter::Editing {
            query: query.to_string(),
            draft: draft.to_string(),
            restore_anchor: anchor.map(str::to_string),
        }
    }

    fn inactive(query: &str) -> BranchFilter {
        BranchFilter::Inactive {
            query: query.to_string(),
        }
    }

    #[test]
    fn enter_seeds_draft_from_query_and_captures_anchor() {
        let mut filter = inactive("feat");
        filter.enter(Some("refs/heads/x".to_string()));
        assert_eq!(filter, editing("feat", "feat", Some("refs/heads/x")));
    }

    #[test]
    fn enter_while_already_editing_is_a_noop() {
        let mut filter = editing("feat", "feature", Some("k"));
        let before = filter.clone();
        filter.enter(Some("other".to_string()));
        assert_eq!(filter, before);
    }

    #[test]
    fn set_draft_changes_only_the_draft() {
        let mut filter = editing("feat", "feat", None);
        filter.set_draft("feature".to_string());
        assert_eq!(filter.effective_query(), "feature");
        assert_eq!(filter.applied_query(), "feat");
    }

    #[test]
    fn set_draft_when_inactive_is_a_noop() {
        let mut filter = inactive("feat");
        filter.set_draft("ignored".to_string());
        assert_eq!(filter, inactive("feat"));
    }

    #[test]
    fn apply_commits_the_draft_and_closes_the_editor() {
        let mut filter = editing("old", "new", Some("k"));
        filter.apply();
        assert_eq!(filter, inactive("new"));
        assert!(!filter.is_editing());
    }

    #[test]
    fn cancel_reverts_to_committed_query_and_returns_the_anchor() {
        let mut filter = editing("feat", "feature typed", Some("k"));
        assert_eq!(filter.cancel(), Some("k".to_string()));
        assert_eq!(filter, inactive("feat"));
    }

    #[test]
    fn cancel_when_inactive_is_a_noop_returning_none() {
        let mut filter = inactive("feat");
        assert_eq!(filter.cancel(), None);
        assert_eq!(filter, inactive("feat"));
    }

    #[test]
    fn clear_empties_the_applied_query() {
        let mut filter = inactive("feat");
        filter.clear();
        assert_eq!(filter, inactive(""));
        assert!(!filter.has_active_filter());
    }

    #[test]
    fn effective_query_is_draft_while_editing_else_applied() {
        assert_eq!(inactive("a").effective_query(), "a");
        let filter = editing("a", "ab", None);
        assert_eq!(filter.effective_query(), "ab");
        assert_eq!(filter.applied_query(), "a");
    }

    #[test]
    fn has_active_filter_ignores_whitespace_only_query() {
        assert!(!inactive("   ").has_active_filter());
        assert!(inactive("x").has_active_filter());
    }
}
