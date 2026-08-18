mod worktrees;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, MouseEvent, MouseEventKind};

use super::app::{App, AppMode, BranchInfo, Msg};
use super::theme::{scroll, timing};
use worktrees::handle_worktree_mode_key;

pub(super) enum Command {
    Delete(BranchInfo),
    Prune(BranchInfo),
    PruneWorktree(crate::git::WorktreeInfo),
    RemoveWorktree(crate::git::WorktreeInfo),
    Refresh(u32),
    RefreshWorktrees,
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
    if app.is_editing_filter() {
        return handle_filter_input_key(app, key);
    }

    match app.mode().clone() {
        AppMode::Normal => {
            if app.is_worktree_view() {
                handle_worktree_mode_key(app, key)
            } else {
                handle_normal_mode_key(app, key)
            }
        }
        AppMode::ConfirmDelete { branch_key } => {
            let branch_key = branch_key.clone();
            handle_confirm_delete_key(app, key, &branch_key)
        }
        AppMode::ConfirmWorktreePrune { worktree } => {
            handle_confirm_worktree_prune_key(app, key, &worktree)
        }
        AppMode::ConfirmRemoveWorktree { .. } => handle_confirm_remove_worktree_key(app, key),
        AppMode::RemovingWorktree { .. } => handle_pending_worktree_removal_key(app, key),
        AppMode::ErrorPopup(_) => {
            handle_error_popup_key(app, key);
            None
        }
    }
}

pub(super) fn is_quit_key(key: &KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => true,
        KeyCode::Char('c') => key.modifiers.contains(event::KeyModifiers::CONTROL),
        _ => false,
    }
}

fn handle_pending_worktree_removal_key(app: &mut App, key: KeyEvent) -> Option<Command> {
    if is_quit_key(&key) {
        app.update(Msg::Quit);
    }
    None
}

fn handle_normal_mode_key(app: &mut App, key: KeyEvent) -> Option<Command> {
    if is_quit_key(&key) {
        if key.code == KeyCode::Esc && app.has_active_filter() {
            app.update(Msg::ClearFilter);
        } else {
            app.update(Msg::Quit);
        }
        return None;
    }

    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                app.update(Msg::ScrollDown(scroll::LINE_SCROLL_AMOUNT));
            } else {
                app.update(Msg::NextBranch);
            }
            None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                app.update(Msg::ScrollUp(scroll::LINE_SCROLL_AMOUNT));
            } else {
                app.update(Msg::PreviousBranch);
            }
            None
        }
        KeyCode::PageDown => {
            app.update(Msg::ScrollDown(
                app.visible_height() / scroll::PAGE_SCROLL_DIVISOR,
            ));
            None
        }
        KeyCode::PageUp => {
            app.update(Msg::ScrollUp(
                app.visible_height() / scroll::PAGE_SCROLL_DIVISOR,
            ));
            None
        }
        KeyCode::Char('d') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
            app.update(Msg::ScrollDown(
                app.visible_height() / scroll::PAGE_SCROLL_DIVISOR,
            ));
            None
        }
        KeyCode::Char('u') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
            app.update(Msg::ScrollUp(
                app.visible_height() / scroll::PAGE_SCROLL_DIVISOR,
            ));
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
        KeyCode::Char('w') => {
            app.update(Msg::ToggleWorktreeView);
            Some(Command::RefreshWorktrees)
        }
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

fn handle_confirm_worktree_prune_key(
    app: &mut App,
    key: KeyEvent,
    worktree: &crate::git::WorktreeInfo,
) -> Option<Command> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => {
            let worktree = worktree.clone();
            app.cancel_mode();
            Some(Command::PruneWorktree(worktree))
        }
        KeyCode::Char('n') | KeyCode::Esc | KeyCode::Char('q') => {
            app.cancel_mode();
            None
        }
        _ => None,
    }
}

fn handle_confirm_remove_worktree_key(app: &mut App, key: KeyEvent) -> Option<Command> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => {
            let worktree = app.confirmed_remove_worktree();
            app.cancel_mode();
            worktree.map(Command::RemoveWorktree)
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
    if !app.is_normal_mode() || app.is_editing_filter() {
        return;
    }

    match mouse_event.kind {
        MouseEventKind::ScrollDown => app.update(Msg::ScrollDown(scroll::LINE_SCROLL_AMOUNT)),
        MouseEventKind::ScrollUp => app.update(Msg::ScrollUp(scroll::LINE_SCROLL_AMOUNT)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::git::{
        BranchScope, WorktreeCleanliness, WorktreeDirtyReason, WorktreeIdentity, WorktreeInfo,
        WorktreeState, WorktreeSubmodules,
    };
    use crate::tui::app::{App, BranchInfo, BranchView};

    #[test]
    fn test_confirm_delete_derives_prune_from_current_branch_state() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        app.update(Msg::ToggleView);
        app.enter_confirm_mode();
        app.set_remote_freshness(HashSet::new());

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
        assert!(app.is_editing_filter());
        assert_eq!(app.filter_input(), "feature old");
    }

    #[test]
    fn test_filter_input_enter_applies_filter() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        app.enter_filter_input();
        app.update_filter_input("feature login".to_string());

        let action = handle_key_event(&mut app, KeyEvent::from(KeyCode::Enter));

        assert!(action.is_none());
        assert!(!app.is_editing_filter());
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
        assert!(!app.is_editing_filter());
        assert_eq!(app.branch_filter(), "feature old");
        assert_eq!(app.effective_branch_filter(), "feature old");
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
        assert!(!app.is_editing_filter());
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
        assert!(app.is_editing_filter());
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
    #[test]
    fn test_worktree_shortcuts_toggle_navigate_and_refresh() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        app.update(Msg::SetWorktrees(vec![
            worktree(WorktreeIdentity::Main, true),
            worktree(
                WorktreeIdentity::Linked {
                    name: "linked".to_string(),
                },
                false,
            ),
        ]));

        assert!(matches!(
            handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('w'))),
            Some(Command::RefreshWorktrees)
        ));
        assert!(app.is_worktree_view());

        handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.selected_worktree().unwrap().name(), "linked");
        handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(app.selected_worktree().unwrap().name(), "main");
        handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('j')));

        assert!(handle_key_event(&mut app, KeyEvent::from(KeyCode::Enter)).is_none());
        assert!(app.is_normal_mode());
        assert!(matches!(
            handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('r'))),
            Some(Command::RefreshWorktrees)
        ));
        assert!(handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('w'))).is_none());
        assert!(!app.is_worktree_view());
    }

    #[test]
    fn test_worktree_d_confirms_only_missing_prunable_metadata_and_routes_action() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        let mut entry = worktree(
            WorktreeIdentity::Linked {
                name: "linked missing".to_string(),
            },
            false,
        );
        entry.path = "/tmp/missing linked path".into();
        entry.branch = Some("feature/preserved".to_string());
        entry.state = WorktreeState::Missing;
        entry.prunable = true;
        app.update(Msg::SetWorktrees(vec![
            worktree(WorktreeIdentity::Main, true),
            entry.clone(),
        ]));
        app.update(Msg::ToggleWorktreeView);
        handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('j')));

        assert!(handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('d'))).is_none());
        let AppMode::ConfirmWorktreePrune {
            worktree: confirmation,
        } = app.mode()
        else {
            panic!("missing worktree should enter confirmation mode");
        };
        assert_eq!(confirmation.path, entry.path);
        assert_eq!(confirmation.ref_display(), "feature/preserved");

        match handle_key_event(&mut app, KeyEvent::from(KeyCode::Enter)) {
            Some(Command::PruneWorktree(worktree)) => {
                assert_eq!(worktree.identity, entry.identity);
            }
            _ => panic!("confirmation should route metadata prune action"),
        }
        assert!(app.is_normal_mode());
    }

    #[test]
    fn test_worktree_d_rejects_valid_and_unknown_entries() {
        for state in [
            WorktreeState::Valid,
            WorktreeState::Unknown("probe failed".into()),
        ] {
            let mut app = App::new(vec![remote_branch(false)], vec![]);
            let mut entry = worktree(
                WorktreeIdentity::Linked {
                    name: "linked".to_string(),
                },
                false,
            );
            entry.state = state;
            entry.prunable = true;
            app.update(Msg::SetWorktrees(vec![entry]));
            app.update(Msg::ToggleWorktreeView);

            assert!(handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('d'))).is_none());
            assert!(matches!(app.mode(), AppMode::Normal));
            assert!(
                app.get_status_message()
                    .expect("rejection should set status")
                    .is_error
            );
        }
    }

    #[test]
    fn test_worktree_d_rejects_malformed_linked_identity() {
        for name in ["", "   "] {
            let mut app = App::new(vec![remote_branch(false)], vec![]);
            let mut entry = worktree(
                WorktreeIdentity::Linked {
                    name: name.to_string(),
                },
                false,
            );
            entry.state = WorktreeState::Missing;
            entry.prunable = true;
            app.update(Msg::SetWorktrees(vec![entry]));
            app.update(Msg::ToggleWorktreeView);

            handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('d')));

            assert!(matches!(app.mode(), AppMode::Normal));
            let status = app
                .get_status_message()
                .expect("rejection should set status");
            assert!(status.is_error);
            assert!(status.text.contains("valid linked worktree"));
        }
    }

    #[test]
    fn test_worktree_d_rejects_main_worktree_with_status() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        app.update(Msg::SetWorktrees(vec![worktree(
            WorktreeIdentity::Main,
            true,
        )]));
        app.update(Msg::ToggleWorktreeView);

        assert!(handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('d'))).is_none());
        assert!(app.is_normal_mode());

        let status = app
            .get_status_message()
            .expect("main worktree rejection should set a status");
        assert!(status.is_error);
        assert!(status.text.contains("main worktree is protected"));
    }

    #[test]
    fn test_worktree_d_rejects_dirty_valid_linked_worktree_with_status() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        let mut entry = worktree(
            WorktreeIdentity::Linked {
                name: "dirty linked".to_string(),
            },
            false,
        );
        entry.cleanliness = WorktreeCleanliness::Dirty(vec![WorktreeDirtyReason::Worktree]);
        app.update(Msg::SetWorktrees(vec![entry]));
        app.update(Msg::ToggleWorktreeView);

        assert!(handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('d'))).is_none());
        assert!(app.is_normal_mode());

        let status = app
            .get_status_message()
            .expect("dirty worktree rejection should set a status");
        assert!(status.is_error);
        assert!(status.text.contains("uncommitted changes"));
    }

    #[test]
    fn test_worktree_remove_shortcut_uses_separate_confirmation_action() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        app.update(Msg::SetWorktrees(vec![
            worktree(WorktreeIdentity::Main, true),
            worktree(
                WorktreeIdentity::Linked {
                    name: "linked".to_string(),
                },
                false,
            ),
        ]));
        app.update(Msg::ToggleWorktreeView);
        handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('j')));

        assert!(handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('d'))).is_none());
        let confirmed_worktree = match app.mode() {
            AppMode::ConfirmRemoveWorktree { worktree } => worktree,
            _ => panic!("d should enter worktree confirmation"),
        };
        assert_eq!(
            confirmed_worktree.path,
            std::path::PathBuf::from("/tmp/fixture")
        );
        assert_eq!(confirmed_worktree.ref_display(), "main");
        let mut replacement = worktree(
            WorktreeIdentity::Linked {
                name: "linked".to_string(),
            },
            false,
        );
        replacement.path = "/tmp/replaced".into();
        app.update(Msg::SetWorktrees(vec![
            worktree(WorktreeIdentity::Main, true),
            replacement,
        ]));

        let action = handle_key_event(&mut app, KeyEvent::from(KeyCode::Enter));
        match action {
            Some(Command::RemoveWorktree(entry)) => {
                assert_eq!(entry.name(), "linked");
                assert_eq!(entry.path, std::path::PathBuf::from("/tmp/fixture"));
            }
            _ => panic!("expected worktree removal action"),
        }
        assert!(app.is_normal_mode());
    }

    #[test]
    fn test_worktree_removal_pending_consumes_mutations_and_defers_quit() {
        let target = worktree(
            WorktreeIdentity::Linked {
                name: "linked".to_string(),
            },
            false,
        );
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        app.update(Msg::SetWorktrees(vec![target.clone()]));
        app.update(Msg::ToggleWorktreeView);
        app.update(Msg::EnterWorktreeRemovalMode { worktree: target });

        for key in [
            KeyCode::Char('d'),
            KeyCode::Char('r'),
            KeyCode::Char('j'),
            KeyCode::Down,
        ] {
            assert!(handle_key_event(&mut app, KeyEvent::from(key)).is_none());
        }
        assert!(matches!(app.mode(), AppMode::RemovingWorktree { .. }));
        assert!(!app.should_quit());

        assert!(handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('q'))).is_none());
        assert!(app.should_quit());
        assert!(matches!(app.mode(), AppMode::RemovingWorktree { .. }));
    }

    fn worktree(identity: WorktreeIdentity, is_current: bool) -> WorktreeInfo {
        let is_main = identity.is_main();
        WorktreeInfo {
            identity,
            path: "/tmp/fixture".into(),
            branch: Some("main".to_string()),
            detached_short_sha: None,
            is_main,
            is_current,
            cleanliness: WorktreeCleanliness::Clean,
            lock_reason: None,
            state: WorktreeState::Valid,
            prunable: false,
            submodules: WorktreeSubmodules::None,
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
