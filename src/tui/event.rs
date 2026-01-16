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

use super::app::{App, WorkItemStatus};
use super::ui;
use crate::azure_devops::{AzureDevOpsClient, WorkItem};
use crate::config::Config;

/// Message sent from background fetch tasks to the main loop
enum FetchResult {
    Success { id: u32, work_item: WorkItem },
    Error { id: u32, error: String },
}

pub async fn run_app(mut app: App) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Load config and create client
    let config = Config::load()?;
    let client = AzureDevOpsClient::new(&config)?;

    // Create channel for background fetch results
    let (tx, rx) = mpsc::unbounded_channel::<FetchResult>();

    // Main loop
    let result = run_loop(&mut terminal, &mut app, client, tx, rx).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    client: AzureDevOpsClient,
    tx: mpsc::UnboundedSender<FetchResult>,
    mut rx: mpsc::UnboundedReceiver<FetchResult>,
) -> Result<()> {
    // Get terminal size for scroll calculations
    let visible_height = terminal.size()?.height.saturating_sub(4);

    // Track which work items are currently being fetched to avoid duplicate requests
    let mut pending_fetches: HashSet<u32> = HashSet::new();

    loop {
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
        if let Some(branch) = app.selected_branch() {
            if let Some(wi_id) = branch.work_item_id {
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
        }

        // Draw UI
        terminal.draw(|frame| ui::render(frame, app))?;

        // Handle input with timeout
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    match key.code {
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
                        KeyCode::PageDown => app.scroll_down(visible_height / 2, visible_height),
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
                        KeyCode::Char('o') | KeyCode::Enter => {
                            open_current_work_item(app);
                        }
                        KeyCode::Char('r') => {
                            // Refresh: reload current work item
                            if let Some(branch) = app.selected_branch() {
                                if let Some(wi_id) = branch.work_item_id {
                                    pending_fetches.remove(&wi_id);
                                    app.reset_work_item(wi_id);
                                }
                            }
                        }
                        KeyCode::Char('c')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            app.quit()
                        }
                        _ => {}
                    }
                }
                Event::Mouse(mouse_event) => {
                    use crossterm::event::MouseEventKind;
                    match mouse_event.kind {
                        MouseEventKind::ScrollDown => app.scroll_down(3, visible_height),
                        MouseEventKind::ScrollUp => app.scroll_up(3),
                        _ => {}
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
    if let Some(branch) = app.selected_branch() {
        if let Some(wi_id) = branch.work_item_id {
            if let WorkItemStatus::Loaded(wi) = app.get_work_item_status(wi_id) {
                if let Some(ref url) = wi.url {
                    let _ = open_url(url);
                }
            }
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
