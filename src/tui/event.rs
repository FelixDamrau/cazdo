use std::collections::HashSet;
use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc;

use super::app::{App, DetailsMetrics, Msg};
use super::ui;
use super::{
    actions::{
        execute_checkout_branch, execute_delete_branch, execute_prune_branch,
        execute_prune_worktree, open_current_work_item,
    },
    background::{
        FetchResult, fetch_branch_status_if_needed, process_fetch_results,
        trigger_remote_freshness_check, trigger_work_item_fetch, trigger_worktree_refresh,
        trigger_worktree_removal,
    },
    input::{Command, handle_input, is_quit_key},
};
use crate::azure_devops::{AzureDevOpsClient, work_item_client};
use crate::error::format_error_chain;
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

    if !app.deleted_branches().is_empty() {
        println!("\nDeleted branches this session:");
        for db in app.deleted_branches() {
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
    let mut worktree_refresh_pending = false;
    let mut worktree_refresh_requested = false;
    let mut worktree_removal_pending = false;

    loop {
        app.clear_expired_status();
        let removal_finished = process_fetch_results(
            &mut rx,
            app,
            &mut pending_fetches,
            &mut worktree_refresh_pending,
            &mut worktree_refresh_requested,
        );
        if removal_finished {
            worktree_removal_pending = false;
            discard_buffered_input(app)?;
        }
        if worktree_refresh_requested && !worktree_refresh_pending {
            trigger_worktree_refresh(
                git_repo,
                &tx,
                &mut worktree_refresh_pending,
                &mut worktree_refresh_requested,
            );
        }
        trigger_work_item_fetch(app, &client, &tx, &mut pending_fetches);
        trigger_remote_freshness_check(app, git_repo, &tx);
        fetch_branch_status_if_needed(app, git_repo);

        let mut metrics = DetailsMetrics::default();
        terminal.draw(|frame| metrics = ui::render(frame, app))?;
        app.update(Msg::SetDetailsMetrics(metrics));

        if let Some(action) = handle_input(app)? {
            match action {
                Command::Delete(branch) => execute_delete_branch(app, git_repo, &branch),
                Command::Prune(branch) => execute_prune_branch(app, git_repo, &branch),
                Command::PruneWorktree(worktree) => {
                    execute_prune_worktree(app, git_repo, &worktree);
                    trigger_worktree_refresh(
                        git_repo,
                        &tx,
                        &mut worktree_refresh_pending,
                        &mut worktree_refresh_requested,
                    );
                }
                Command::RemoveWorktree(worktree) => {
                    if worktree_removal_pending {
                        continue;
                    }

                    app.update(Msg::EnterWorktreeRemovalMode {
                        worktree: worktree.clone(),
                    });

                    match git_repo.repo_dir() {
                        Ok(repo_dir) => trigger_worktree_removal(
                            repo_dir,
                            worktree,
                            &tx,
                            &mut worktree_removal_pending,
                        ),
                        Err(error) => {
                            worktree_removal_pending = true;
                            let _ = tx.send(FetchResult::WorktreeRemovalError {
                                error: format_error_chain(&error),
                            });
                        }
                    }
                }
                Command::Refresh(wi_id) => {
                    pending_fetches.remove(&wi_id);
                    app.reset_work_item(wi_id);
                }
                Command::RefreshWorktrees => trigger_worktree_refresh(
                    git_repo,
                    &tx,
                    &mut worktree_refresh_pending,
                    &mut worktree_refresh_requested,
                ),
                Command::OpenWorkItem => open_current_work_item(app),
                Command::Checkout(branch) => execute_checkout_branch(app, git_repo, &branch),
            }
        }

        if app.should_quit() && !worktree_removal_pending {
            return Ok(());
        }
    }
}

fn discard_buffered_input(app: &mut App) -> Result<()> {
    while event::poll(Duration::ZERO)? {
        handle_buffered_input_event(app, event::read()?);
    }
    Ok(())
}

fn handle_buffered_input_event(app: &mut App, input: Event) {
    if let Event::Key(key) = input
        && key.kind == KeyEventKind::Press
        && is_quit_key(&key)
    {
        app.update(Msg::Quit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

    #[test]
    fn buffered_quit_is_preserved_while_other_input_is_discarded() {
        let mut app = App::new(vec![], vec![]);

        handle_buffered_input_event(
            &mut app,
            Event::Key(crossterm::event::KeyEvent::from(KeyCode::Char('d'))),
        );
        assert!(!app.should_quit());

        handle_buffered_input_event(
            &mut app,
            Event::Key(crossterm::event::KeyEvent::from(KeyCode::Char('q'))),
        );
        assert!(app.should_quit());
    }
}
