use crossterm::event::{self, KeyCode, KeyEvent};

use super::Command;
use crate::tui::app::{App, Msg};
use crate::tui::theme::timing;

pub(super) fn handle_worktree_mode_key(app: &mut App, key: KeyEvent) -> Option<Command> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.update(Msg::Quit);
            None
        }
        KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
            app.update(Msg::Quit);
            None
        }
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
