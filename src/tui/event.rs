use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use super::app::{App, WorkItemStatus};
use super::ui;
use crate::azure_devops::AzureDevOpsClient;
use crate::config::Config;

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

    // Main loop
    let result = run_loop(&mut terminal, &mut app, &client).await;

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
    client: &AzureDevOpsClient,
) -> Result<()> {
    // Get terminal size for scroll calculations
    let visible_height = terminal.size()?.height.saturating_sub(4); // Account for borders/footer

    loop {
        // Fetch work item for currently selected branch if needed
        if let Some(branch) = app.selected_branch() {
            if let Some(wi_id) = branch.work_item_id {
                if matches!(app.get_work_item_status(wi_id), WorkItemStatus::NotFetched) {
                    app.set_work_item_loading(wi_id);

                    match client.get_work_item(wi_id).await {
                        Ok(wi) => app.set_work_item_loaded(wi_id, wi),
                        Err(e) => app.set_work_item_error(wi_id, e.to_string()),
                    }
                }
            }
        }

        // Draw UI
        terminal.draw(|frame| ui::render(frame, app))?;

        // Handle input with timeout (to allow async operations)
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => app.quit(),
                        KeyCode::Down | KeyCode::Char('j') => {
                            if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                                // Shift+j = scroll down
                                app.scroll_down(3, visible_height);
                            } else {
                                app.next();
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                                // Shift+k = scroll up
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
                            // Ctrl+d = half page down (vim style)
                            app.scroll_down(visible_height / 2, visible_height);
                        }
                        KeyCode::Char('u')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            // Ctrl+u = half page up (vim style)
                            app.scroll_up(visible_height / 2);
                        }
                        KeyCode::Char('o') | KeyCode::Enter => {
                            open_current_work_item(app);
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
                    // Use open crate or fallback to platform-specific commands
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
