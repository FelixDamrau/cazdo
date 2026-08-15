use super::{App, AppMode};
use crate::git::{WorktreeInfo, validate_worktree_prune};
use crate::tui::theme::timing;

impl App {
    pub fn is_worktree_view(&self) -> bool {
        self.worktree_view
    }

    pub fn worktrees(&self) -> &[WorktreeInfo] {
        &self.worktrees
    }

    pub fn selected_worktree(&self) -> Option<&WorktreeInfo> {
        self.worktrees.get(self.worktree_selected_index)
    }

    pub fn confirm_worktree_prune(&self) -> Option<&WorktreeInfo> {
        match &self.mode {
            AppMode::ConfirmWorktreePrune { worktree } => Some(worktree),
            _ => None,
        }
    }

    pub(super) fn worktree_prune_error(entry: &WorktreeInfo) -> Option<String> {
        validate_worktree_prune(entry).err()
    }

    pub fn worktree_selected_index(&self) -> usize {
        self.worktree_selected_index
    }

    pub(super) fn set_worktrees(&mut self, mut worktrees: Vec<WorktreeInfo>) {
        let selected_identity = self.selected_worktree().map(|entry| entry.identity.clone());
        worktrees.sort_by(|left, right| {
            left.is_main
                .cmp(&right.is_main)
                .reverse()
                .then_with(|| left.name().cmp(right.name()))
        });
        self.worktrees = worktrees;
        self.worktree_selected_index = selected_identity
            .and_then(|identity| {
                self.worktrees
                    .iter()
                    .position(|entry| entry.identity == identity)
            })
            .unwrap_or(
                self.worktree_selected_index
                    .min(self.worktrees.len().saturating_sub(1)),
            );
    }

    pub(super) fn apply_worktree_error(&mut self, error: String) {
        self.set_status_message(error, true, timing::STATUS_DURATION_SECS);
    }
}
