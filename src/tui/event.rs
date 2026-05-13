use std::collections::HashSet;
use std::io;

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc;

use super::app::App;
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
    input::{Command, handle_input},
};
use crate::azure_devops::{AzureDevOpsClient, work_item_client};
use crate::git::GitRepo;

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
                Command::Delete(branch) => execute_delete_branch(app, git_repo, &branch),
                Command::Prune(branch) => execute_prune_branch(app, git_repo, &branch),
                Command::Refresh(wi_id) => {
                    pending_fetches.remove(&wi_id);
                    app.reset_work_item(wi_id);
                }
                Command::OpenWorkItem => open_current_work_item(app),
                Command::Checkout(branch) => execute_checkout_branch(app, git_repo, &branch),
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
