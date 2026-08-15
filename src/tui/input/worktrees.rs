use crossterm::event::{self, KeyCode, KeyEvent};

use super::Command;
use crate::tui::app::{App, Msg};

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
        KeyCode::Char('w') => {
            app.update(Msg::ToggleWorktreeView);
            None
        }
        KeyCode::Char('r') => Some(Command::RefreshWorktrees),
        _ => None,
    }
}
