use std::io;

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use super::app::{App, DetailsMetrics, Msg};
use super::ui;
use super::{
    actions::{
        execute_checkout_branch, execute_delete_branch, execute_prune_branch,
        execute_prune_worktree, execute_remove_worktree, open_current_work_item,
    },
    background::{BackgroundLoads, fetch_branch_status_if_needed},
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

    let result = run_loop(&mut terminal, &mut app, client, &git_repo).await;

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
    git_repo: &GitRepo,
) -> Result<()> {
    let mut background = BackgroundLoads::new(client, git_repo.repo_dir());

    loop {
        app.clear_expired_status();
        background.poll(app);
        fetch_branch_status_if_needed(app, git_repo);

        let mut metrics = DetailsMetrics::default();
        terminal.draw(|frame| metrics = ui::render(frame, app))?;
        app.update(Msg::SetDetailsMetrics(metrics));

        if let Some(action) = handle_input(app)? {
            execute_command(app, git_repo, &mut background, action);
        }

        if app.should_quit() {
            return Ok(());
        }
    }
}

fn execute_command(
    app: &mut App,
    git_repo: &GitRepo,
    background: &mut BackgroundLoads,
    action: Command,
) {
    match action {
        Command::Delete(branch) => execute_delete_branch(app, git_repo, &branch),
        Command::Prune(branch) => execute_prune_branch(app, git_repo, &branch),
        Command::PruneWorktree(worktree) => {
            execute_prune_worktree(app, git_repo, &worktree);
            background.refresh_worktrees();
        }
        Command::RemoveWorktree(worktree) => {
            execute_remove_worktree(app, git_repo, &worktree);
            background.refresh_worktrees();
        }
        Command::Refresh(wi_id) => background.refresh_work_item(app, wi_id),
        Command::RefreshWorktrees => background.refresh_worktrees(),
        Command::OpenWorkItem => open_current_work_item(app),
        Command::Checkout(branch) => execute_checkout_branch(app, git_repo, &branch),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{
        FixtureGitRepo, WorktreeCleanliness, WorktreeIdentity, WorktreeInfo, WorktreeState,
        WorktreeSubmodules,
    };
    use crate::tui::app::AppMode;

    fn verify_removal_refresh(result: std::result::Result<(), String>) {
        let failed = result.is_err();
        let git_repo = GitRepo::fixture(FixtureGitRepo::new().with_remove_worktree_result(result));
        let dir = tempfile::TempDir::new().unwrap();
        let fixture = dir.path().join("work-items.json");
        std::fs::write(&fixture, "[]").unwrap();
        let client = AzureDevOpsClient::new_fixture(fixture).unwrap();
        let mut background = BackgroundLoads::new(client, git_repo.repo_dir());
        let mut app = App::new(vec![], vec![]);
        let worktree = WorktreeInfo {
            identity: WorktreeIdentity::Linked {
                name: "feature".to_string(),
            },
            path: dir.path().join("feature"),
            branch: Some("feature".to_string()),
            detached_short_sha: None,
            is_main: false,
            is_current: false,
            cleanliness: WorktreeCleanliness::Clean,
            lock_reason: None,
            state: WorktreeState::Valid,
            prunable: false,
            submodules: WorktreeSubmodules::None,
        };

        // The removal must request another inventory even while one is pending.
        background.refresh_worktrees();
        execute_command(
            &mut app,
            &git_repo,
            &mut background,
            Command::RemoveWorktree(worktree),
        );
        assert_eq!(matches!(app.mode(), AppMode::ErrorPopup(_)), failed);

        for _ in 0..2 {
            app.update(Msg::ClearStatus);
            background.poll(&mut app);
            assert_eq!(
                app.get_status_message()
                    .expect("inventory should have run")
                    .text,
                "fixture git repo: repo_dir unsupported"
            );
        }
        app.update(Msg::ClearStatus);
        background.poll(&mut app);
        assert!(app.get_status_message().is_none());
    }

    #[test]
    fn refreshes_inventory_after_successful_worktree_removal() {
        verify_removal_refresh(Ok(()));
    }

    #[test]
    fn refreshes_inventory_after_failed_worktree_removal() {
        verify_removal_refresh(Err("removal failed after filesystem change".to_string()));
    }
}
