use std::collections::HashSet;
use std::io;

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc;

use super::app::{App, AppMode, BranchInfo};
use super::theme::{scroll, timing};
use super::ui;
use super::{
    actions::{
        execute_checkout_branch, execute_delete_branch, execute_prune_branch,
        open_current_work_item,
    },
    background::{
        FetchResult, fetch_branch_status_if_needed, process_fetch_results,
        trigger_remote_freshness_check, trigger_work_item_fetch,
    },
};
use crate::azure_devops::{AzureDevOpsClient, work_item_client};
use crate::git::GitRepo;

enum Action {
    Delete(BranchInfo),
    Prune(BranchInfo),
    Refresh(u32),
    OpenWorkItem,
    Checkout(BranchInfo),
}

pub async fn run_app(mut app: App, git_repo: GitRepo) -> Result<()> {
    let client = work_item_client()?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, rx) = mpsc::unbounded_channel::<FetchResult>();
    let result = run_loop(&mut terminal, &mut app, client, tx, rx, &git_repo).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if !app.deleted_branches.is_empty() {
        println!("\nDeleted branches this session:");
        for db in &app.deleted_branches {
            match &db.restore_hint {
                Some(hint) => println!("  • {} - restore: {}", db.name, hint),
                None => println!("  • {}", db.name),
            }
        }
    }

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    client: AzureDevOpsClient,
    tx: mpsc::UnboundedSender<FetchResult>,
    mut rx: mpsc::UnboundedReceiver<FetchResult>,
    git_repo: &GitRepo,
) -> Result<()> {
    let mut pending_fetches: HashSet<u32> = HashSet::new();

    loop {
        app.clear_expired_status();
        process_fetch_results(&mut rx, app, &mut pending_fetches);
        trigger_work_item_fetch(app, &client, &tx, &mut pending_fetches);
        trigger_remote_freshness_check(app, git_repo, &tx);
        fetch_branch_status_if_needed(app, git_repo);

        terminal.draw(|frame| ui::render(frame, app))?;

        if let Some(action) = handle_input(app)? {
            match action {
                Action::Delete(branch) => execute_delete_branch(app, git_repo, &branch),
                Action::Prune(branch) => execute_prune_branch(app, git_repo, &branch),
                Action::Refresh(wi_id) => {
                    pending_fetches.remove(&wi_id);
                    app.reset_work_item(wi_id);
                }
                Action::OpenWorkItem => open_current_work_item(app),
                Action::Checkout(branch) => execute_checkout_branch(app, git_repo, &branch),
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn handle_input(app: &mut App) -> Result<Option<Action>> {
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

fn handle_key_event(app: &mut App, key: KeyEvent) -> Option<Action> {
    match &app.mode {
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

fn handle_normal_mode_key(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => {
            if app.has_active_filter() {
                app.clear_branch_filter();
            } else {
                app.quit();
            }
            None
        }
        KeyCode::Char('q') => {
            app.quit();
            None
        }
        KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
            app.quit();
            None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                app.scroll_down(scroll::LINE_SCROLL_AMOUNT);
            } else {
                app.next();
            }
            None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                app.scroll_up(scroll::LINE_SCROLL_AMOUNT);
            } else {
                app.previous();
            }
            None
        }
        KeyCode::PageDown => {
            app.scroll_down(app.visible_height / scroll::PAGE_SCROLL_DIVISOR);
            None
        }
        KeyCode::PageUp => {
            app.scroll_up(app.visible_height / scroll::PAGE_SCROLL_DIVISOR);
            None
        }
        KeyCode::Char('d') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
            app.scroll_down(app.visible_height / scroll::PAGE_SCROLL_DIVISOR);
            None
        }
        KeyCode::Char('u') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
            app.scroll_up(app.visible_height / scroll::PAGE_SCROLL_DIVISOR);
            None
        }
        KeyCode::Char('d') => {
            if let Err(e) = app.can_delete_selected() {
                app.set_status_message(e, true, timing::STATUS_DURATION_SECS);
            } else {
                app.enter_confirm_mode();
            }
            None
        }
        KeyCode::Char('D') => {
            if let Err(e) = app.can_delete_selected() {
                app.set_status_message(e, true, timing::STATUS_DURATION_SECS);
                None
            } else if app.selected_branch().is_some_and(|b| b.is_stale) {
                app.selected_branch().cloned().map(Action::Prune)
            } else {
                app.selected_branch().cloned().map(Action::Delete)
            }
        }
        KeyCode::Char('o') => Some(Action::OpenWorkItem),
        KeyCode::Enter => app.selected_branch().cloned().map(Action::Checkout),
        KeyCode::Char('t') => {
            app.toggle_view();
            None
        }
        KeyCode::Char('/') => {
            app.enter_filter_input();
            None
        }
        KeyCode::Char('r') => app.selected_work_item_id().map(Action::Refresh),
        KeyCode::Char('p') => {
            app.toggle_show_protected();
            None
        }
        _ => None,
    }
}

fn handle_filter_input_key(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Enter => {
            app.apply_filter_input();
            None
        }
        KeyCode::Esc => {
            app.cancel_filter_input();
            None
        }
        KeyCode::Backspace => {
            let mut filter_input = app.filter_input.clone();
            filter_input.pop();
            app.update_filter_input(filter_input);
            None
        }
        KeyCode::Char('u') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
            app.update_filter_input(String::new());
            None
        }
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(event::KeyModifiers::CONTROL | event::KeyModifiers::ALT) =>
        {
            let mut filter_input = app.filter_input.clone();
            filter_input.push(c);
            app.update_filter_input(filter_input);
            None
        }
        _ => None,
    }
}

fn handle_confirm_delete_key(app: &mut App, key: KeyEvent, branch_key: &str) -> Option<Action> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => {
            let branch = app.branch_by_key(branch_key)?.clone();
            let action = if branch.is_stale {
                Action::Prune(branch)
            } else {
                Action::Delete(branch)
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
        KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
            app.cancel_mode();
        }
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
        app.active_view = BranchView::Remote;
        app.enter_confirm_mode();
        app.branches[0].is_stale = true;

        let branch = match &app.mode {
            AppMode::ConfirmDelete { branch_key } => branch_key.clone(),
            _ => panic!("expected confirm mode"),
        };

        let action = handle_confirm_delete_key(&mut app, KeyEvent::from(KeyCode::Enter), &branch);

        match action {
            Some(Action::Prune(branch)) => assert_eq!(branch.key, "refs/remotes/origin/feature/1"),
            _ => panic!("expected prune action after branch became stale"),
        }
    }

    #[test]
    fn test_slash_enters_filter_input_with_prefilled_filter() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        app.branch_filter = "feature old".to_string();

        let action = handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('/')));

        assert!(action.is_none());
        assert!(matches!(app.mode, AppMode::FilterInput));
        assert_eq!(app.filter_input, "feature old");
    }

    #[test]
    fn test_filter_input_enter_applies_filter() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        app.enter_filter_input();
        app.update_filter_input("feature login".to_string());

        let action = handle_key_event(&mut app, KeyEvent::from(KeyCode::Enter));

        assert!(action.is_none());
        assert!(matches!(app.mode, AppMode::Normal));
        assert_eq!(app.branch_filter, "feature login");
    }

    #[test]
    fn test_filter_input_escape_discards_draft() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        app.branch_filter = "feature old".to_string();
        app.enter_filter_input();
        app.update_filter_input("feature new".to_string());

        let action = handle_key_event(&mut app, KeyEvent::from(KeyCode::Esc));

        assert!(action.is_none());
        assert!(matches!(app.mode, AppMode::Normal));
        assert_eq!(app.branch_filter, "feature old");
        assert_eq!(app.filter_input, "feature old");
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
        app.local_selected_index = 2;
        app.enter_filter_input();
        app.update_filter_input("login".to_string());

        let action = handle_key_event(&mut app, KeyEvent::from(KeyCode::Esc));

        assert!(action.is_none());
        assert!(matches!(app.mode, AppMode::Normal));
        assert_eq!(app.selected_branch().unwrap().branch_name, "chore/docs");
    }

    #[test]
    fn test_escape_clears_active_filter_before_quit() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        app.branch_filter = "feature".to_string();

        let action = handle_key_event(&mut app, KeyEvent::from(KeyCode::Esc));

        assert!(action.is_none());
        assert!(app.branch_filter.is_empty());
        assert!(!app.should_quit);
    }

    #[test]
    fn test_filter_input_ignores_normal_shortcuts() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        app.enter_filter_input();

        let action = handle_key_event(&mut app, KeyEvent::from(KeyCode::Char('t')));

        assert!(action.is_none());
        assert!(matches!(app.mode, AppMode::FilterInput));
        assert_eq!(app.active_view, BranchView::Local);
        assert_eq!(app.filter_input, "t");
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
