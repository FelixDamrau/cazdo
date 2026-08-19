use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Command, is_quit_key};
use crate::tui::app::{App, Msg};
use crate::tui::theme::timing;

pub(super) fn handle_worktree_mode_key(app: &mut App, key: KeyEvent) -> Option<Command> {
    if is_quit_key(&key) {
        app.update(Msg::Quit);
        return None;
    }
    if key.modifiers != KeyModifiers::NONE {
        return None;
    }

    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            app.update(Msg::NextBranch);
            None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.update(Msg::PreviousBranch);
            None
        }
        KeyCode::Char('d') => {
            let needs_metadata_prune = app
                .selected_worktree()
                .is_some_and(|entry| !entry.state.is_valid() || entry.prunable);
            if needs_metadata_prune {
                app.update(Msg::RequestWorktreePrune);
            } else if let Err(error) = app.can_remove_selected_worktree() {
                app.set_status_message(error, true, timing::STATUS_DURATION_SECS);
            } else {
                app.enter_remove_worktree_confirm_mode();
            }
            None
        }
        KeyCode::Char('w') => {
            app.update(Msg::ToggleWorktreeView);
            None
        }
        KeyCode::Char('r') => Some(Command::RefreshWorktrees),
        _ => None,
    }
}
