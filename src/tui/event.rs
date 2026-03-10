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

use super::app::{App, AppMode, BranchInfo, BranchView, WorkItemStatus};
use super::theme::{scroll, timing};
use super::ui;
use crate::azure_devops::{AzureDevOpsClient, WorkItem};
use crate::config::Config;
use crate::git::{BranchScope, DeleteResult, GitRepo, list_origin_remote_heads_in_dir, short_sha};

enum FetchResult {
    Success { id: u32, work_item: WorkItem },
    Error { id: u32, error: String },
    RemoteFreshnessSuccess { live_branches: HashSet<String> },
    RemoteFreshnessError { error: String },
}

enum Action {
    Delete(BranchInfo),
    Refresh(u32),
    OpenWorkItem,
    Checkout(BranchInfo),
}

pub async fn run_app(mut app: App, git_repo: GitRepo) -> Result<()> {
    let config = Config::load()?;
    let client = AzureDevOpsClient::new(&config)?;

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

fn process_fetch_results(
    rx: &mut mpsc::UnboundedReceiver<FetchResult>,
    app: &mut App,
    pending_fetches: &mut HashSet<u32>,
) {
    while let Ok(result) = rx.try_recv() {
        match result {
            FetchResult::Success { id, work_item } => {
                app.set_work_item_loaded(id, work_item);
                pending_fetches.remove(&id);
            }
            FetchResult::Error { id, error } => {
                app.set_work_item_error(id, error);
                pending_fetches.remove(&id);
            }
            FetchResult::RemoteFreshnessSuccess { live_branches } => {
                app.set_remote_freshness(live_branches);
            }
            FetchResult::RemoteFreshnessError { error } => {
                app.set_remote_freshness_error(error);
                app.set_status_message(
                    "Could not verify origin branches".to_string(),
                    true,
                    timing::STATUS_DURATION_SECS,
                );
            }
        }
    }
}

fn trigger_remote_freshness_check(
    app: &mut App,
    git_repo: &GitRepo,
    tx: &mpsc::UnboundedSender<FetchResult>,
) {
    if !app.should_check_remote_freshness() {
        return;
    }

    app.set_remote_freshness_checking();
    let tx = tx.clone();

    let repo_dir = match git_repo.repo_dir() {
        Ok(repo_dir) => repo_dir,
        Err(error) => {
            app.set_remote_freshness_error(error.to_string());
            return;
        }
    };

    tokio::task::spawn_blocking(move || {
        let message = match list_origin_remote_heads_in_dir(&repo_dir) {
            Ok(live_branches) => FetchResult::RemoteFreshnessSuccess { live_branches },
            Err(error) => FetchResult::RemoteFreshnessError {
                error: error.to_string(),
            },
        };
        let _ = tx.send(message);
    });
}

fn trigger_work_item_fetch(
    app: &mut App,
    client: &AzureDevOpsClient,
    tx: &mpsc::UnboundedSender<FetchResult>,
    pending_fetches: &mut HashSet<u32>,
) {
    if let Some(wi_id) = app.selected_work_item_id() {
        let status = app.get_work_item_status(wi_id);
        if matches!(status, WorkItemStatus::NotFetched) && !pending_fetches.contains(&wi_id) {
            app.set_work_item_loading(wi_id);
            pending_fetches.insert(wi_id);

            let client = client.clone();
            let tx = tx.clone();

            tokio::spawn(async move {
                let result = match client.get_work_item(wi_id).await {
                    Ok(work_item) => FetchResult::Success {
                        id: wi_id,
                        work_item,
                    },
                    Err(e) => FetchResult::Error {
                        id: wi_id,
                        error: e.to_string(),
                    },
                };
                let _ = tx.send(result);
            });
        }
    }
}

fn fetch_branch_status_if_needed(app: &mut App, git_repo: &GitRepo) {
    if let Some(branch) = app.selected_branch() {
        let branch_key = branch.key.clone();
        if app.needs_branch_status(&branch_key)
            && let Ok(status) = git_repo.get_branch_status(
                branch.scope,
                &branch.branch_name,
                branch.remote_name.as_deref(),
            )
        {
            app.set_branch_status(branch_key, status);
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
        AppMode::ConfirmDelete(branch) => {
            let branch = branch.clone();
            handle_confirm_delete_key(app, key, &branch)
        }
        AppMode::ErrorPopup(_) => {
            handle_error_popup_key(app, key);
            None
        }
    }
}

fn handle_normal_mode_key(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
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
                app.enter_delete_mode();
            }
            None
        }
        KeyCode::Char('D') => {
            if let Err(e) = app.can_delete_selected() {
                app.set_status_message(e, true, timing::STATUS_DURATION_SECS);
                None
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
        KeyCode::Char('r') => app.selected_work_item_id().map(Action::Refresh),
        KeyCode::Char('p') => {
            app.toggle_show_protected();
            None
        }
        _ => None,
    }
}

fn handle_confirm_delete_key(app: &mut App, key: KeyEvent, branch: &BranchInfo) -> Option<Action> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => {
            let action = Action::Delete(branch.clone());
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

fn open_current_work_item(app: &App) {
    if let Some(wi_id) = app.selected_work_item_id()
        && let WorkItemStatus::Loaded(wi) = app.get_work_item_status(wi_id)
        && let Some(ref url) = wi.url
    {
        let _ = open_url(url);
    }
}

fn execute_delete_branch(app: &mut App, git_repo: &GitRepo, branch: &BranchInfo) {
    match git_repo.delete_branch(
        branch.scope,
        &branch.branch_name,
        branch.remote_name.as_deref(),
        &app.protected_patterns,
    ) {
        Ok(DeleteResult::Local { commit_sha }) => {
            let restore_hint = format!("git checkout -b {} {}", branch.branch_name, commit_sha);
            app.record_deleted_branch(branch.display_name.clone(), Some(restore_hint));
            app.remove_branch(&branch.key);
            app.set_status_message(
                format!(
                    "Deleted {} (was {})",
                    branch.display_name,
                    short_sha(&commit_sha)
                ),
                false,
                timing::STATUS_DURATION_SECS,
            );
        }
        Ok(DeleteResult::Remote) => {
            app.record_deleted_branch(branch.display_name.clone(), None);
            app.remove_branch(&branch.key);
            app.set_status_message(
                format!("Deleted remote branch '{}'", branch.display_name),
                false,
                timing::STATUS_DURATION_SECS,
            );
        }
        Err(e) => app.set_status_message(e.to_string(), true, timing::STATUS_DURATION_SECS),
    }
}

fn execute_checkout_branch(app: &mut App, git_repo: &GitRepo, branch: &BranchInfo) {
    match git_repo.checkout_branch(
        branch.scope,
        &branch.branch_name,
        branch.remote_name.as_deref(),
    ) {
        Ok(()) => {
            app.update_current_branch(&branch.branch_name);
            if branch.scope == BranchScope::Remote {
                app.active_view = BranchView::Local;
            }
            app.set_status_message(
                format!("Switched to branch '{}'", branch.branch_name),
                false,
                timing::STATUS_DURATION_SECS,
            );
        }
        Err(e) => app.show_error_popup(e.to_string()),
    }
}

fn open_url(url: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    Ok(())
}
