use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, MouseEvent, MouseEventKind};

use super::app::{App, AppMode, BranchInfo, Msg};
use super::theme::{scroll, timing};

pub(super) enum Command {
    Delete(BranchInfo),
    Prune(BranchInfo),
    Refresh(u32),
    OpenWorkItem,
    Checkout(BranchInfo),
}

pub(super) fn handle_input(app: &mut App) -> Result<Option<Command>> {
    if !event::poll(timing::POLL_INTERVAL)? {
        return Ok(None);
    }

    match event::read()? {
        Event::Key(key) if key.kind == KeyEventKind::Press => Ok(handle_key_event(app, key)),
        Event::Mouse(mouse_event) => {
            handle_mouse_event(app, mouse_event);
            Ok(None)
        }
        _ => Ok(None),
    }
}

fn handle_key_event(app: &mut App, key: KeyEvent) -> Option<Command> {
    match app.mode() {
        AppMode::Normal => handle_normal_mode_key(app, key),
        AppMode::FilterInput => handle_filter_input_key(app, key),
        AppMode::ConfirmDelete { branch_key } => {
            let branch_key = branch_key.clone();
            handle_confirm_delete_key(app, key, &branch_key)
        }
        AppMode::ErrorPopup(_) => {
            handle_error_popup_key(app, key);
            None
        }
    }
}

fn handle_normal_mode_key(app: &mut App, key: KeyEvent) -> Option<Command> {
    match key.code {
        KeyCode::Esc => {
            if app.has_active_filter() {
                app.update(Msg::ClearFilter);
            } else {
                app.update(Msg::Quit);
            }
            None
        }
        KeyCode::Char('q') => {
            app.update(Msg::Quit);
            None
        }
        KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
            app.update(Msg::Quit);
            None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                app.scroll_down(scroll::LINE_SCROLL_AMOUNT);
            } else {
                app.update(Msg::NextBranch);
            }
            None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                app.scroll_up(scroll::LINE_SCROLL_AMOUNT);
            } else {
                app.update(Msg::PreviousBranch);
            }
            None
        }
        KeyCode::PageDown => {
            app.scroll_down(app.visible_height() / scroll::PAGE_SCROLL_DIVISOR);
            None
        }
        KeyCode::PageUp => {
            app.scroll_up(app.visible_height() / scroll::PAGE_SCROLL_DIVISOR);
            None
        }
        KeyCode::Char('d') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
            app.scroll_down(app.visible_height() / scroll::PAGE_SCROLL_DIVISOR);
            None
        }
        KeyCode::Char('u') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
            app.scroll_up(app.visible_height() / scroll::PAGE_SCROLL_DIVISOR);
            None
        }
        KeyCode::Char('d') => {
            if let Err(error) = app.can_delete_selected() {
                app.set_status_message(error, true, timing::STATUS_DURATION_SECS);
            } else {
                app.enter_confirm_mode();
            }
            None
        }
        KeyCode::Char('D') => {
            if let Err(error) = app.can_delete_selected() {
                app.set_status_message(error, true, timing::STATUS_DURATION_SECS);
                None
            } else if app.selected_branch().is_some_and(|branch| branch.is_stale) {
                app.selected_branch().cloned().map(Command::Prune)
            } else {
                app.selected_branch().cloned().map(Command::Delete)
            }
        }
        KeyCode::Char('o') => Some(Command::OpenWorkItem),
        KeyCode::Enter => app.selected_branch().cloned().map(Command::Checkout),
        KeyCode::Char('t') => {
            app.update(Msg::ToggleView);
            None
        }
        KeyCode::Char('/') => {
            app.update(Msg::StartFilter);
            None
        }
        KeyCode::Char('r') => app.selected_work_item_id().map(Command::Refresh),
        KeyCode::Char('p') => {
            app.update(Msg::ToggleShowProtected);
            None
        }
        _ => None,
    }
}

fn handle_filter_input_key(app: &mut App, key: KeyEvent) -> Option<Command> {
    match key.code {
        KeyCode::Enter => {
            app.update(Msg::ApplyFilter);
            None
        }
        KeyCode::Esc => {
            app.update(Msg::CancelFilter);
            None
        }
        KeyCode::Backspace => {
            let mut filter_input = app.filter_input().to_string();
            filter_input.pop();
            app.update(Msg::SetFilterInput(filter_input));
            None
        }
        KeyCode::Char('u') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
            app.update(Msg::SetFilterInput(String::new()));
            None
        }
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(event::KeyModifiers::CONTROL | event::KeyModifiers::ALT) =>
        {
            let mut filter_input = app.filter_input().to_string();
            filter_input.push(c);
            app.update(Msg::SetFilterInput(filter_input));
            None
        }
        _ => None,
    }
}

fn handle_confirm_delete_key(app: &mut App, key: KeyEvent, branch_key: &str) -> Option<Command> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => {
            let branch = app.branch_by_key(branch_key)?.clone();
            let action = if branch.is_stale {
                Command::Prune(branch)
            } else {
                Command::Delete(branch)
            };
            app.cancel_mode();
            Some(action)
        }
        KeyCode::Char('n') | KeyCode::Esc | KeyCode::Char('q') => {
            app.cancel_mode();
            None
        }
        _ => None,
    }
}

fn handle_error_popup_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => app.cancel_mode(),
        _ => {}
    }
}

fn handle_mouse_event(app: &mut App, mouse_event: MouseEvent) {
    if !app.is_normal_mode() {
        return;
    }

    match mouse_event.kind {
        MouseEventKind::ScrollDown => app.scroll_down(scroll::LINE_SCROLL_AMOUNT),
        MouseEventKind::ScrollUp => app.scroll_up(scroll::LINE_SCROLL_AMOUNT),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::BranchScope;
    use crate::tui::app::{App, AppMode, BranchInfo, BranchView};

    #[test]
    fn test_confirm_delete_derives_prune_from_current_branch_state() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        app.update(Msg::ToggleView);
        app.enter_confirm_mode();
        app.mark_branch_stale("refs/remotes/origin/feature/1");

        let action = handle_key_event(&mut app, KeyEvent::from(KeyCode::Enter));

        match action {
            Some(Command::Prune(branch)) => {
                assert_eq!(branch.key, "refs/remotes/origin/feature/1")
            }
            _ => panic!("expected prune action after branch became stale"),
        }
    }

    #[test]
    fn test_slash_enters_filter_input_with_prefilled_filter() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        app.apply_branch_filter("feature old".to_string());

        let action = handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('/')));

        assert!(action.is_none());
        assert!(matches!(app.mode(), AppMode::FilterInput));
        assert_eq!(app.filter_input(), "feature old");
    }

    #[test]
    fn test_filter_input_enter_applies_filter() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        app.enter_filter_input();
        app.update_filter_input("feature login".to_string());

        let action = handle_key_event(&mut app, KeyEvent::from(KeyCode::Enter));

        assert!(action.is_none());
        assert!(matches!(app.mode(), AppMode::Normal));
        assert_eq!(app.branch_filter(), "feature login");
    }

    #[test]
    fn test_filter_input_escape_discards_draft() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        app.apply_branch_filter("feature old".to_string());
        app.enter_filter_input();
        app.update_filter_input("feature new".to_string());

        let action = handle_key_event(&mut app, KeyEvent::from(KeyCode::Esc));

        assert!(action.is_none());
        assert!(matches!(app.mode(), AppMode::Normal));
        assert_eq!(app.branch_filter(), "feature old");
        assert_eq!(app.filter_input(), "feature old");
    }

    #[test]
    fn test_filter_input_escape_restores_pre_edit_selection() {
        let mut app = App::new(
            vec![
                BranchInfo {
                    key: "refs/heads/feature/alpha-login".to_string(),
                    display_name: "feature/alpha-login".to_string(),
                    branch_name: "feature/alpha-login".to_string(),
                    remote_name: None,
                    scope: BranchScope::Local,
                    work_item_id: None,
                    is_current: false,
                    is_protected: false,
                    is_stale: false,
                },
                BranchInfo {
                    key: "refs/heads/feature/beta-login".to_string(),
                    display_name: "feature/beta-login".to_string(),
                    branch_name: "feature/beta-login".to_string(),
                    remote_name: None,
                    scope: BranchScope::Local,
                    work_item_id: None,
                    is_current: false,
                    is_protected: false,
                    is_stale: false,
                },
                BranchInfo {
                    key: "refs/heads/chore/docs".to_string(),
                    display_name: "chore/docs".to_string(),
                    branch_name: "chore/docs".to_string(),
                    remote_name: None,
                    scope: BranchScope::Local,
                    work_item_id: None,
                    is_current: false,
                    is_protected: false,
                    is_stale: false,
                },
            ],
            vec![],
        );
        app.set_selected_index_for_test(2);
        app.enter_filter_input();
        app.update_filter_input("login".to_string());

        let action = handle_key_event(&mut app, KeyEvent::from(KeyCode::Esc));

        assert!(action.is_none());
        assert!(matches!(app.mode(), AppMode::Normal));
        assert_eq!(app.selected_branch().unwrap().branch_name, "chore/docs");
    }

    #[test]
    fn test_escape_clears_active_filter_before_quit() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        app.apply_branch_filter("feature".to_string());

        let action = handle_key_event(&mut app, KeyEvent::from(KeyCode::Esc));

        assert!(action.is_none());
        assert!(app.branch_filter().is_empty());
        assert!(!app.should_quit());
    }

    #[test]
    fn test_filter_input_ignores_normal_shortcuts() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        app.enter_filter_input();

        let action = handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('t')));

        assert!(action.is_none());
        assert!(matches!(app.mode(), AppMode::FilterInput));
        assert_eq!(app.active_view(), BranchView::Local);
        assert_eq!(app.filter_input(), "t");
    }

    #[test]
    fn test_delete_shortcut_sets_status_when_branch_cannot_be_deleted() {
        let mut app = App::new(
            vec![BranchInfo {
                key: "refs/heads/main".to_string(),
                display_name: "main".to_string(),
                branch_name: "main".to_string(),
                remote_name: None,
                scope: BranchScope::Local,
                work_item_id: None,
                is_current: true,
                is_protected: true,
                is_stale: false,
            }],
            vec![],
        );

        let action = handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('d')));

        assert!(action.is_none());
        let status = app
            .get_status_message()
            .expect("delete failure should set a status");
        assert!(status.is_error);
        assert_eq!(status.text, "Cannot delete the current branch");
    }

    #[test]
    fn test_immediate_delete_shortcut_prunes_stale_branch() {
        let mut app = App::new(vec![remote_branch(true)], vec![]);
        app.update(Msg::ToggleView);

        let action = handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('D')));

        match action {
            Some(Command::Prune(branch)) => {
                assert_eq!(branch.key, "refs/remotes/origin/feature/1")
            }
            _ => panic!("expected stale branch to trigger prune action"),
        }
    }

    fn remote_branch(is_stale: bool) -> BranchInfo {
        BranchInfo {
            key: "refs/remotes/origin/feature/1".to_string(),
            display_name: "origin/feature/1".to_string(),
            branch_name: "feature/1".to_string(),
            remote_name: Some("origin".to_string()),
            scope: BranchScope::Remote,
            work_item_id: None,
            is_current: false,
            is_protected: false,
            is_stale,
        }
    }
}
