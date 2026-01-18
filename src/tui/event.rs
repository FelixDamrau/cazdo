use std::collections::HashSet;
use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc;

use super::app::{App, AppMode, WorkItemStatus};
use super::ui;
use crate::azure_devops::{AzureDevOpsClient, WorkItem};
use crate::config::Config;
use crate::git::GitRepo;

/// Message sent from background fetch tasks to the main loop
enum FetchResult {
    Success { id: u32, work_item: WorkItem },
    Error { id: u32, error: String },
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
    // Get terminal size for scroll calculations
    let visible_height = terminal.size()?.height.saturating_sub(4);

    // Track which work items are currently being fetched to avoid duplicate requests
    let mut pending_fetches: HashSet<u32> = HashSet::new();

    loop {
        // Clear expired status messages
        app.clear_expired_status();

        // Process any completed fetch results (non-blocking)
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

        // Check if we need to fetch work item for currently selected branch
        if let Some(branch) = app.selected_branch()
            && let Some(wi_id) = branch.work_item_id {
                let status = app.get_work_item_status(wi_id);
                if matches!(status, WorkItemStatus::NotFetched) && !pending_fetches.contains(&wi_id)
                {
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
                        let _ = tx.send(result);
                    });
                }
            }

        // Fetch branch status for currently selected branch if needed (synchronous - git is fast)
        if let Some(branch) = app.selected_branch() {
            let branch_name = branch.name.clone();
            if app.needs_branch_status(&branch_name)
                && let Ok(status) = git_repo.get_branch_status(&branch_name) {
                    app.set_branch_status(branch_name, status);
                }
        }

        // Draw UI
        terminal.draw(|frame| ui::render(frame, app))?;

        // Handle input with timeout
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    match app.mode {
                        AppMode::Normal => match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => app.quit(),
                            KeyCode::Down | KeyCode::Char('j') => {
                                if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                                    app.scroll_down(3, visible_height);
                                } else {
                                    app.next();
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                                    app.scroll_up(3);
                                } else {
                                    app.previous();
                                }
                            }
                            KeyCode::PageDown => {
                                app.scroll_down(visible_height / 2, visible_height)
                            }
                            KeyCode::PageUp => app.scroll_up(visible_height / 2),
                            KeyCode::Char('d')
                                if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                            {
                                app.scroll_down(visible_height / 2, visible_height);
                            }
                            KeyCode::Char('u')
                                if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                            {
                                app.scroll_up(visible_height / 2);
                            }
                            // Delete confirmation
                            KeyCode::Char('d') => {
                                if let Err(e) = app.can_delete_selected() {
                                    app.set_status_message(e, true, 3);
                                } else {
                                    app.enter_delete_mode();
                                }
                            }
                            // Immediate delete (Force/Shift)
                            KeyCode::Char('D') => {
                                if let Err(e) = app.can_delete_selected() {
                                    app.set_status_message(e, true, 3);
                                } else if let Some(branch) = app.selected_branch() {
                                    let name = branch.name.clone();
                                    match git_repo.delete_branch(&name) {
                                        Ok(sha) => {
                                            app.record_deleted_branch(name.clone(), sha.clone());
                                            app.remove_branch(&name);
                                            app.set_status_message(
                                                format!("Deleted {} (was {})", name, &sha[..7]),
                                                false,
                                                4,
                                            );
                                        }
                                        Err(e) => {
                                            app.set_status_message(e.to_string(), true, 5);
                                        }
                                    }
                                }
                            }
                            KeyCode::Char('o') | KeyCode::Enter => {
                                open_current_work_item(app);
                            }
                            KeyCode::Char('r') => {
                                // Refresh: reload current work item
                                if let Some(branch) = app.selected_branch()
                                    && let Some(wi_id) = branch.work_item_id {
                                        pending_fetches.remove(&wi_id);
                                        app.reset_work_item(wi_id);
                                    }
                            }
                            KeyCode::Char('c')
                                if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                            {
                                app.quit()
                            }
                            _ => {}
                        },
                        AppMode::ConfirmDelete(ref branch_name) => {
                            let branch_name = branch_name.clone();
                            match key.code {
                                KeyCode::Char('y') | KeyCode::Enter => {
                                    match git_repo.delete_branch(&branch_name) {
                                        Ok(sha) => {
                                            app.record_deleted_branch(
                                                branch_name.clone(),
                                                sha.clone(),
                                            );
                                            app.remove_branch(&branch_name);
                                            app.set_status_message(
                                                format!(
                                                    "Deleted {} (was {})",
                                                    branch_name,
                                                    &sha[..7]
                                                ),
                                                false,
                                                4,
                                            );
                                        }
                                        Err(e) => {
                                            app.set_status_message(e.to_string(), true, 5);
                                        }
                                    }
                                    app.cancel_mode();
                                }
                                KeyCode::Char('n') | KeyCode::Esc | KeyCode::Char('q') => {
                                    app.cancel_mode();
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Event::Mouse(mouse_event) => {
                    use crossterm::event::MouseEventKind;
                    if app.is_normal_mode() {
                        match mouse_event.kind {
                            MouseEventKind::ScrollDown => app.scroll_down(3, visible_height),
                            MouseEventKind::ScrollUp => app.scroll_up(3),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

/// Open the currently selected work item in the default browser
fn open_current_work_item(app: &App) {
    if let Some(branch) = app.selected_branch()
        && let Some(wi_id) = branch.work_item_id
            && let WorkItemStatus::Loaded(wi) = app.get_work_item_status(wi_id)
                && let Some(ref url) = wi.url {
                    let _ = open_url(url);
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
