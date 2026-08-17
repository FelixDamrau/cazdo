use super::*;

impl App {
    pub fn enter_confirm_mode(&mut self) {
        self.update(Msg::EnterDeleteConfirmMode);
    }

    pub(super) fn apply_enter_confirm_mode(&mut self) {
        if let Some(branch) = self.selected_branch() {
            self.mode = AppMode::ConfirmDelete {
                branch_key: branch.key.clone(),
            };
        }
    }

    pub(super) fn apply_request_worktree_prune(&mut self) {
        let Some(worktree) = self.selected_worktree().cloned() else {
            self.apply_worktree_error("No worktree selected".to_string());
            return;
        };
        if let Err(error) = Self::worktree_prune_error(&worktree) {
            self.apply_worktree_error(error);
            return;
        }
        self.mode = AppMode::ConfirmWorktreePrune { worktree };
    }

    pub fn enter_remove_worktree_confirm_mode(&mut self) {
        self.update(Msg::EnterRemoveWorktreeConfirmMode);
    }

    pub fn show_error_popup(&mut self, message: String) {
        self.update(Msg::ShowErrorPopup(message));
    }

    pub fn cancel_mode(&mut self) {
        self.update(Msg::EnterNormalMode);
    }

    pub fn is_normal_mode(&self) -> bool {
        matches!(self.mode, AppMode::Normal)
    }

    pub fn mode(&self) -> &AppMode {
        &self.mode
    }

    pub fn set_status_message(&mut self, text: String, is_error: bool, duration_secs: u64) {
        self.update(Msg::SetStatus(StatusMessage {
            text,
            is_error,
            expires_at: Instant::now() + std::time::Duration::from_secs(duration_secs),
        }));
    }

    pub(super) fn apply_background_error(&mut self, error: String) {
        self.status_message = Some(StatusMessage {
            text: error,
            is_error: true,
            expires_at: Instant::now()
                + std::time::Duration::from_secs(crate::tui::theme::timing::STATUS_DURATION_SECS),
        });
    }

    pub fn get_status_message(&self) -> Option<&StatusMessage> {
        self.status_message
            .as_ref()
            .filter(|message| message.expires_at > Instant::now())
    }

    pub fn clear_expired_status(&mut self) {
        if let Some(ref message) = self.status_message
            && message.expires_at <= Instant::now()
        {
            self.update(Msg::ClearStatus);
        }
    }
}
