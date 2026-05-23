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

    pub fn show_error_popup(&mut self, message: String) {
        self.update(Msg::ShowErrorPopup(message));
    }

    pub fn cancel_mode(&mut self) {
        self.update(Msg::EnterNormalMode);
    }

    pub fn is_normal_mode(&self) -> bool {
        matches!(self.mode, AppMode::Normal)
    }

    pub fn set_status_message(&mut self, text: String, is_error: bool, duration_secs: u64) {
        self.update(Msg::SetStatus(StatusMessage {
            text,
            is_error,
            expires_at: Instant::now() + std::time::Duration::from_secs(duration_secs),
        }));
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
