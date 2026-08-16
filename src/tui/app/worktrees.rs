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
    pub fn can_remove_selected_worktree(&self) -> Result<(), String> {
        let Some(worktree) = self.selected_worktree() else {
            return Err("No worktree selected".to_string());
        };
        if worktree.is_main {
            return Err(format!(
                "Cannot remove worktree '{}': the main worktree is protected",
                worktree.path.display()
            ));
        }
        if worktree.is_current {
            return Err(format!(
                "Cannot remove worktree '{}': the current worktree is protected",
                worktree.path.display()
            ));
        }
        if worktree
            .linked_name()
            .is_none_or(|name| name.trim().is_empty())
        {
            return Err(format!(
                "Cannot remove worktree '{}': linked identity is malformed",
                worktree.path.display()
            ));
        }
        if !worktree.state.is_valid() {
            return Err(format!(
                "Cannot remove worktree '{}': state is {}; refresh worktree inventory first",
                worktree.path.display(),
                worktree.state.label()
            ));
        }
        if worktree.prunable {
            return Err(format!(
                "Cannot remove worktree '{}': it is prunable or missing; refresh worktree inventory first",
                worktree.path.display()
            ));
        }
        if let Some(reason) = &worktree.lock_reason {
            return Err(format!(
                "Cannot remove worktree '{}': it is locked ({reason})",
                worktree.path.display()
            ));
        }
        match &worktree.cleanliness {
            crate::git::WorktreeCleanliness::Clean => {}
            crate::git::WorktreeCleanliness::Dirty(reasons) => {
                let reasons = reasons
                    .iter()
                    .map(|reason| reason.label())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(format!(
                    "Cannot remove worktree '{}': it has uncommitted changes ({reasons})",
                    worktree.path.display()
                ));
            }
            crate::git::WorktreeCleanliness::Unknown(error) => {
                return Err(format!(
                    "Cannot remove worktree '{}': status is unknown ({error})",
                    worktree.path.display()
                ));
            }
        }
        match &worktree.submodules {
            crate::git::WorktreeSubmodules::None => {}
            crate::git::WorktreeSubmodules::Present => {
                return Err(format!(
                    "Cannot remove worktree '{}': it contains submodules",
                    worktree.path.display()
                ));
            }
            crate::git::WorktreeSubmodules::Unknown(error) => {
                return Err(format!(
                    "Cannot remove worktree '{}': submodule status is unknown ({error})",
                    worktree.path.display()
                ));
            }
        }
        Ok(())
    }

    pub(super) fn apply_enter_remove_worktree_confirm_mode(&mut self) {
        if let Some(worktree) = self.selected_worktree().cloned() {
            let ref_display = worktree.ref_display();
            self.mode = super::AppMode::ConfirmRemoveWorktree {
                worktree: Box::new(worktree),
                ref_display,
            };
        }
    }

    pub fn confirmed_remove_worktree(&self) -> Option<WorktreeInfo> {
        let super::AppMode::ConfirmRemoveWorktree { worktree, .. } = &self.mode else {
            return None;
        };
        Some(worktree.as_ref().clone())
    }

    pub fn remove_worktree_confirmation_details(&self) -> Option<(&std::path::Path, &str)> {
        let super::AppMode::ConfirmRemoveWorktree {
            worktree,
            ref_display,
        } = &self.mode
        else {
            return None;
        };
        Some((&worktree.path, ref_display))
    }

    pub fn worktree_selected_index(&self) -> usize {
        self.worktree_selected_index
    }

    pub(super) fn set_worktrees(&mut self, worktrees: Vec<WorktreeInfo>) {
        let selected_identity = self.selected_worktree().map(|entry| entry.identity.clone());
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
