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

use super::app::{App, AppMode, WorkItemStatus};
use super::theme::{scroll, timing};
use super::ui;
use crate::azure_devops::{AzureDevOpsClient, WorkItem};
use crate::config::Config;
use crate::git::GitRepo;

/// Message sent from background fetch tasks to the main loop
enum FetchResult {
    Success { id: u32, work_item: WorkItem },
    Error { id: u32, error: String },
}

/// Actions that can be triggered by user input
enum Action {
    /// Request to delete a branch by name
    Delete(String),
    /// Request to refresh a work item by ID
    Refresh(u32),
    /// Open the current work item in browser
    OpenWorkItem,
    /// Checkout the selected branch
    Checkout(String),
}

pub async fn run_app(mut app: App, git_repo: GitRepo) -> Result<()> {
    // Load config and create client BEFORE terminal setup
    // This ensures errors (like missing CAZDO_PAT) display cleanly
    let config = Config::load()?;
    let client = AzureDevOpsClient::new(&config)?;

    // Setup terminal (only after config validation succeeds)
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create channel for background fetch results
    let (tx, rx) = mpsc::unbounded_channel::<FetchResult>();

    // Main loop
    let result = run_loop(&mut terminal, &mut app, client, tx, rx, &git_repo).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Print summary of deleted branches
    if !app.deleted_branches.is_empty() {
        println!("\nDeleted branches this session:");
        for db in &app.deleted_branches {
            println!(
                "  • {} (was {}) - restore: git checkout -b {} {}",
                db.name,
                &db.commit_sha[..7],
                db.name,
                db.commit_sha
            );
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
    // Track which work items are currently being fetched to avoid duplicate requests
    let mut pending_fetches: HashSet<u32> = HashSet::new();

    loop {
        // Clear expired status messages
        app.clear_expired_status();

        // Process any completed fetch results
        process_fetch_results(&mut rx, app, &mut pending_fetches);

        // Trigger work item fetch if needed
        trigger_work_item_fetch(app, &client, &tx, &mut pending_fetches);

        // Fetch branch status if needed (synchronous - git is fast)
        fetch_branch_status_if_needed(app, git_repo);

        // Draw UI
        terminal.draw(|frame| ui::render(frame, app))?;

        // Handle input and process any resulting actions
        if let Some(action) = handle_input(app)? {
            match action {
                Action::Delete(name) => execute_delete_branch(app, git_repo, &name),
                Action::Refresh(wi_id) => {
                    pending_fetches.remove(&wi_id);
                    app.reset_work_item(wi_id);
                }
                Action::OpenWorkItem => open_current_work_item(app),
                Action::Checkout(name) => execute_checkout_branch(app, git_repo, &name),
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

/// Process completed work item fetch results from the background channel
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
        }
    }
}

/// Trigger a work item fetch if the current branch has an unfetched work item
fn trigger_work_item_fetch(
    app: &mut App,
    client: &AzureDevOpsClient,
    tx: &mpsc::UnboundedSender<FetchResult>,
    pending_fetches: &mut HashSet<u32>,
) {
    if let Some(wi_id) = app.selected_work_item_id() {
        let status = app.get_work_item_status(wi_id);
        if matches!(status, WorkItemStatus::NotFetched) && !pending_fetches.contains(&wi_id) {
            // Mark as loading and spawn background fetch
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
                // Ignore send error - receiver dropped means app is shutting down
                let _ = tx.send(result);
            });
        }
    }
}

/// Fetch branch status if needed (synchronous - git is fast)
fn fetch_branch_status_if_needed(app: &mut App, git_repo: &GitRepo) {
    if let Some(branch) = app.selected_branch() {
        let branch_name = branch.name.clone();
        if app.needs_branch_status(&branch_name)
            && let Ok(status) = git_repo.get_branch_status(&branch_name)
        {
            app.set_branch_status(branch_name, status);
        }
    }
}

/// Handle input events and return an action if one should be performed
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

/// Handle keyboard events based on current app mode
fn handle_key_event(app: &mut App, key: KeyEvent) -> Option<Action> {
    match &app.mode {
        AppMode::Normal => handle_normal_mode_key(app, key),
        AppMode::ConfirmDelete(branch_name) => {
            let branch_name = branch_name.clone();
            handle_confirm_delete_key(app, key, &branch_name)
        }
        AppMode::ErrorPopup(_) => {
            handle_error_popup_key(app, key);
            None
        }
    }
}

/// Handle keyboard events in normal mode
fn handle_normal_mode_key(app: &mut App, key: KeyEvent) -> Option<Action> {
    match key.code {
        // Quit
        KeyCode::Char('q') | KeyCode::Esc => {
            app.quit();
            None
        }
        KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
            app.quit();
            None
        }

        // Navigation
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

        // Page scrolling
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

        // Delete with confirmation
        KeyCode::Char('d') => {
            if let Err(e) = app.can_delete_selected() {
                app.set_status_message(e, true, timing::STATUS_DURATION_SECS);
            } else {
                app.enter_delete_mode();
            }
            None
        }

        // Immediate delete (Force/Shift)
        KeyCode::Char('D') => {
            if let Err(e) = app.can_delete_selected() {
                app.set_status_message(e, true, timing::STATUS_DURATION_SECS);
                None
            } else {
                app.selected_branch()
                    .map(|b| Action::Delete(b.name.clone()))
            }
        }

        // Open work item
        KeyCode::Char('o') => Some(Action::OpenWorkItem),

        // Checkout branch
        KeyCode::Enter => app
            .selected_branch()
            .map(|b| Action::Checkout(b.name.clone())),

        // Refresh work item
        KeyCode::Char('r') => app.selected_work_item_id().map(Action::Refresh),

        // Toggle protected branches
        KeyCode::Char('p') => {
            app.toggle_show_protected();
            None
        }

        _ => None,
    }
}

/// Handle keyboard events in delete confirmation mode
fn handle_confirm_delete_key(app: &mut App, key: KeyEvent, branch_name: &str) -> Option<Action> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => {
            let action = Action::Delete(branch_name.to_string());
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

/// Handle keyboard events in error popup mode
fn handle_error_popup_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
            app.cancel_mode();
        }
        _ => {}
    }
}

/// Handle mouse events
fn handle_mouse_event(app: &mut App, mouse_event: MouseEvent) {
    if !app.is_normal_mode() {
        return;
    }

    match mouse_event.kind {
        MouseEventKind::ScrollDown => {
            app.scroll_down(scroll::LINE_SCROLL_AMOUNT);
        }
        MouseEventKind::ScrollUp => {
            app.scroll_up(scroll::LINE_SCROLL_AMOUNT);
        }
        _ => {}
    }
}

/// Open the currently selected work item in the default browser
fn open_current_work_item(app: &App) {
    if let Some(wi_id) = app.selected_work_item_id()
        && let WorkItemStatus::Loaded(wi) = app.get_work_item_status(wi_id)
        && let Some(ref url) = wi.url
    {
        let _ = open_url(url);
    }
}

/// Execute branch deletion and update app state with result
fn execute_delete_branch(app: &mut App, git_repo: &GitRepo, branch_name: &str) {
    match git_repo.delete_branch(branch_name, &app.protected_patterns) {
        Ok(sha) => {
            app.record_deleted_branch(branch_name.to_string(), sha.clone());
            app.remove_branch(branch_name);
            app.set_status_message(
                format!("Deleted {} (was {})", branch_name, &sha[..7]),
                false,
                timing::STATUS_DURATION_SECS,
            );
        }
        Err(e) => {
            app.set_status_message(e.to_string(), true, timing::STATUS_DURATION_SECS);
        }
    }
}

/// Execute branch checkout and update app state with result
fn execute_checkout_branch(app: &mut App, git_repo: &GitRepo, branch_name: &str) {
    match git_repo.checkout_branch(branch_name) {
        Ok(()) => {
            app.update_current_branch(branch_name);
            app.set_status_message(
                format!("Switched to branch '{}'", branch_name),
                false,
                timing::STATUS_DURATION_SECS,
            );
        }
        Err(e) => {
            app.show_error_popup(e.to_string());
        }
    }
}

/// Open a URL in the default browser
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
