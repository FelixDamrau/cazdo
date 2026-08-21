use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::mpsc;

use super::app::{App, Msg, WorkItemStatus};
use super::theme::timing;
use crate::azure_devops::{AzureDevOpsClient, WorkItem};
use crate::error::format_error_chain;
use crate::git::{GitRepo, WorktreeInfo, list_origin_remote_heads_in_dir};

const REMOTE_FRESHNESS_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) enum FetchResult {
    Success { id: u32, work_item: WorkItem },
    Error { id: u32, error: String },
    RemoteFreshnessSuccess { live_branches: HashSet<String> },
    RemoteFreshnessError { error: String },
    WorktreeInventorySuccess { worktrees: Vec<WorktreeInfo> },
    WorktreeInventoryError { error: String },
    WorktreeRemovalSuccess { worktree: WorktreeInfo },
    WorktreeRemovalError { error: String },
    WorktreeRemovalTaskError { error: String },
}

pub(super) fn process_fetch_results(
    rx: &mut mpsc::UnboundedReceiver<FetchResult>,
    app: &mut App,
    pending_fetches: &mut HashSet<u32>,
    worktree_refresh_pending: &mut bool,
    worktree_refresh_requested: &mut bool,
) -> bool {
    let mut worktree_removal_finished = false;

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
                app.update(Msg::SetBackgroundError(
                    "Could not verify origin branches".to_string(),
                ));
            }
            FetchResult::WorktreeInventorySuccess { worktrees } => {
                *worktree_refresh_pending = false;
                app.update(Msg::SetWorktrees(worktrees));
            }
            FetchResult::WorktreeInventoryError { error } => {
                *worktree_refresh_pending = false;
                app.update(Msg::SetWorktreeError(error));
            }
            FetchResult::WorktreeRemovalSuccess { worktree } => {
                worktree_removal_finished = true;
                *worktree_refresh_requested = true;
                app.cancel_mode();
                app.set_status_message(
                    format!("Removed worktree '{}'", worktree.path.display()),
                    false,
                    timing::STATUS_DURATION_SECS,
                );
            }
            FetchResult::WorktreeRemovalError { error }
            | FetchResult::WorktreeRemovalTaskError { error } => {
                worktree_removal_finished = true;
                *worktree_refresh_requested = true;
                app.cancel_mode();
                app.show_error_popup(error);
            }
        }
    }

    worktree_removal_finished
}

pub(super) fn trigger_remote_freshness_check(
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
            app.set_remote_freshness_error(format_error_chain(&error));
            return;
        }
    };

    tokio::spawn(async move {
        let _ = tx.send(fetch_remote_freshness(repo_dir).await);
    });
}

pub(super) fn trigger_worktree_refresh(
    git_repo: &GitRepo,
    tx: &mpsc::UnboundedSender<FetchResult>,
    worktree_refresh_pending: &mut bool,
    worktree_refresh_requested: &mut bool,
) {
    if *worktree_refresh_pending {
        *worktree_refresh_requested = true;
        return;
    }

    *worktree_refresh_requested = false;

    let repo_dir = match git_repo.repo_dir() {
        Ok(repo_dir) => repo_dir,
        Err(error) => {
            let _ = tx.send(FetchResult::WorktreeInventoryError {
                error: format_error_chain(&error),
            });
            return;
        }
    };

    *worktree_refresh_pending = true;
    let tx = tx.clone();

    tokio::spawn(async move {
        let result =
            tokio::task::spawn_blocking(move || GitRepo::list_worktrees_at(&repo_dir)).await;
        let result = match result {
            Ok(Ok(worktrees)) => FetchResult::WorktreeInventorySuccess { worktrees },
            Ok(Err(error)) => FetchResult::WorktreeInventoryError {
                error: format_error_chain(&error),
            },
            Err(error) => FetchResult::WorktreeInventoryError {
                error: format!(
                    "Worktree inventory task failed: {}",
                    format_error_chain(&error)
                ),
            },
        };
        let _ = tx.send(result);
    });
}

pub(super) fn trigger_worktree_removal(
    repo_dir: PathBuf,
    worktree: WorktreeInfo,
    tx: &mpsc::UnboundedSender<FetchResult>,
    removal_pending: &mut bool,
) {
    if *removal_pending {
        return;
    }

    *removal_pending = true;
    let tx = tx.clone();
    let target = worktree.clone();

    tokio::spawn(async move {
        let result =
            tokio::task::spawn_blocking(move || GitRepo::remove_worktree_at(repo_dir, target))
                .await;
        let result = match result {
            Ok(Ok(())) => FetchResult::WorktreeRemovalSuccess { worktree },
            Ok(Err(error)) => FetchResult::WorktreeRemovalError {
                error: format_error_chain(&error),
            },
            Err(error) => FetchResult::WorktreeRemovalTaskError {
                error: format!(
                    "Worktree removal task failed: {}",
                    format_error_chain(&error)
                ),
            },
        };
        let _ = tx.send(result);
    });
}

pub(super) fn trigger_work_item_fetch(
    app: &mut App,
    client: &AzureDevOpsClient,
    tx: &mpsc::UnboundedSender<FetchResult>,
    pending_fetches: &mut HashSet<u32>,
) {
    if !app.is_worktree_view()
        && let Some(wi_id) = app.selected_work_item_id()
    {
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
                    Err(error) => FetchResult::Error {
                        id: wi_id,
                        error: format_error_chain(&error),
                    },
                };
                let _ = tx.send(result);
            });
        }
    }
}

pub(super) fn fetch_branch_status_if_needed(app: &mut App, git_repo: &GitRepo) {
    if !app.is_worktree_view()
        && let Some(branch) = app.selected_branch()
    {
        let branch_key = branch.key.clone();
        let branch_display_name = branch.display_name.clone();

        if app.needs_branch_status(&branch_key) {
            let result = git_repo.get_branch_status(
                branch.scope,
                &branch.branch_name,
                branch.remote_name.as_deref(),
            );
            apply_branch_status_result(app, &branch_key, &branch_display_name, result);
        }
    }
}

async fn fetch_remote_freshness(repo_dir: PathBuf) -> FetchResult {
    let task = tokio::task::spawn_blocking(move || list_origin_remote_heads_in_dir(&repo_dir));

    let join_result = match tokio::time::timeout(REMOTE_FRESHNESS_TIMEOUT, task).await {
        Ok(join_result) => join_result,
        Err(_) => {
            return FetchResult::RemoteFreshnessError {
                error: "Network timeout checking origin branches".to_string(),
            };
        }
    };

    let branch_result = match join_result {
        Ok(branch_result) => branch_result,
        Err(_) => {
            return FetchResult::RemoteFreshnessError {
                error: "Task panicked while checking origin branches".to_string(),
            };
        }
    };

    match branch_result {
        Ok(live_branches) => FetchResult::RemoteFreshnessSuccess { live_branches },
        Err(error) => FetchResult::RemoteFreshnessError {
            error: format_error_chain(&error),
        },
    }
}

fn apply_branch_status_result(
    app: &mut App,
    branch_key: &str,
    branch_display_name: &str,
    result: Result<crate::git::BranchStatus>,
) {
    match result {
        Ok(status) => app.set_branch_status(branch_key.to_string(), status),
        Err(error) => {
            let error_text = format_error_chain(&error);
            let should_show_status = app.get_branch_status_error(branch_key) != Some(&error_text);

            app.set_branch_status_error(branch_key.to_string(), error_text.clone());

            if should_show_status {
                app.set_status_message(
                    format!(
                        "Could not load branch info for '{}': {}",
                        branch_display_name, error_text
                    ),
                    true,
                    timing::STATUS_DURATION_SECS,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azure_devops::{WorkItemState, WorkItemType};
    use crate::git::{
        BranchScope, FixtureGitRepo, GitRepo, WorktreeCleanliness, WorktreeIdentity, WorktreeInfo,
        WorktreeState, WorktreeSubmodules,
    };
    use crate::tui::app::BranchInfo;

    // --- fixtures ------------------------------------------------------------

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

    fn removal_target() -> WorktreeInfo {
        WorktreeInfo {
            identity: WorktreeIdentity::Linked {
                name: "feature/test".to_string(),
            },
            path: "/repo/feature-test".into(),
            branch: Some("feature/test".to_string()),
            detached_short_sha: None,
            is_main: false,
            is_current: false,
            cleanliness: WorktreeCleanliness::Clean,
            lock_reason: None,
            state: WorktreeState::Valid,
            prunable: false,
            submodules: WorktreeSubmodules::None,
        }
    }

    // --- process_fetch_results -----------------------------------------------

    #[test]
    fn test_process_fetch_results_loads_work_item_and_clears_pending_fetch() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut pending_fetches = HashSet::from([42]);
        let mut worktree_refresh_pending = false;
        let mut worktree_refresh_requested = false;

        tx.send(FetchResult::Success {
            id: 42,
            work_item: WorkItem {
                id: 42,
                title: "Loaded item".to_string(),
                work_item_type: WorkItemType::Task,
                state: WorkItemState::Active,
                assigned_to: None,
                url: None,
                tags: vec![],
                rich_text_fields: vec![],
            },
        })
        .expect("send should succeed");

        process_fetch_results(
            &mut rx,
            &mut app,
            &mut pending_fetches,
            &mut worktree_refresh_pending,
            &mut worktree_refresh_requested,
        );

        assert!(pending_fetches.is_empty());
        match app.get_work_item_status(42) {
            WorkItemStatus::Loaded(work_item) => assert_eq!(work_item.title, "Loaded item"),
            _ => panic!("expected loaded work item"),
        }
    }

    #[test]
    fn test_process_fetch_results_preserves_nested_async_error_text() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut pending_fetches = HashSet::from([42]);
        let mut worktree_refresh_pending = false;
        let mut worktree_refresh_requested = false;

        tx.send(FetchResult::Error {
            id: 42,
            error: "request failed: transport failed: connection refused".to_string(),
        })
        .expect("send should succeed");

        process_fetch_results(
            &mut rx,
            &mut app,
            &mut pending_fetches,
            &mut worktree_refresh_pending,
            &mut worktree_refresh_requested,
        );

        match app.get_work_item_status(42) {
            WorkItemStatus::Error(error) => assert_eq!(
                error,
                "request failed: transport failed: connection refused"
            ),
            status => panic!("expected nested work item error, got {status:?}"),
        }
        assert!(pending_fetches.is_empty());
    }

    #[test]
    fn test_process_fetch_results_sets_remote_freshness_error_and_status() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut pending_fetches = HashSet::new();
        let mut worktree_refresh_pending = false;
        let mut worktree_refresh_requested = false;

        tx.send(FetchResult::RemoteFreshnessError {
            error: "origin unreachable".to_string(),
        })
        .expect("send should succeed");

        process_fetch_results(
            &mut rx,
            &mut app,
            &mut pending_fetches,
            &mut worktree_refresh_pending,
            &mut worktree_refresh_requested,
        );

        assert_eq!(app.remote_freshness_error(), Some("origin unreachable"));
        let status = app
            .get_status_message()
            .expect("remote freshness error should surface in footer");
        assert!(status.is_error);
        assert_eq!(status.text, "Could not verify origin branches");
    }

    #[test]
    fn test_process_fetch_results_applies_worktree_inventory_and_clears_pending() {
        let mut app = App::new(vec![], vec![]);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut pending_fetches = HashSet::new();
        let mut worktree_refresh_pending = true;
        let mut worktree_refresh_requested = false;
        let replacement = WorktreeInfo {
            identity: WorktreeIdentity::Linked {
                name: "feature/test".to_string(),
            },
            path: "/repo/feature-test".into(),
            branch: Some("feature/test".to_string()),
            detached_short_sha: None,
            is_main: false,
            is_current: false,
            cleanliness: WorktreeCleanliness::Clean,
            lock_reason: None,
            state: WorktreeState::Valid,
            prunable: false,
            submodules: WorktreeSubmodules::None,
        };

        tx.send(FetchResult::WorktreeInventorySuccess {
            worktrees: vec![replacement.clone()],
        })
        .expect("send should succeed");

        process_fetch_results(
            &mut rx,
            &mut app,
            &mut pending_fetches,
            &mut worktree_refresh_pending,
            &mut worktree_refresh_requested,
        );

        assert!(!worktree_refresh_pending);
        assert_eq!(app.worktrees(), &[replacement]);
    }

    #[test]
    fn test_process_fetch_results_reports_worktree_inventory_error() {
        let mut app = App::new(vec![], vec![]);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut pending_fetches = HashSet::new();
        let mut worktree_refresh_pending = true;
        let mut worktree_refresh_requested = false;

        tx.send(FetchResult::WorktreeInventoryError {
            error: "metadata unreadable".to_string(),
        })
        .expect("send should succeed");

        process_fetch_results(
            &mut rx,
            &mut app,
            &mut pending_fetches,
            &mut worktree_refresh_pending,
            &mut worktree_refresh_requested,
        );

        assert!(!worktree_refresh_pending);
        let status = app
            .get_status_message()
            .expect("worktree inventory error should be visible");
        assert!(status.is_error);
        assert_eq!(status.text, "metadata unreadable");
    }

    #[test]
    fn test_process_fetch_results_maps_removal_success_and_clears_pending_mode() {
        let target = removal_target();
        let mut app = App::new(vec![], vec![]);
        app.update(Msg::EnterWorktreeRemovalMode {
            worktree: target.clone(),
        });
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut pending_fetches = HashSet::new();
        let mut worktree_refresh_pending = false;
        let mut worktree_refresh_requested = false;

        tx.send(FetchResult::WorktreeRemovalSuccess { worktree: target })
            .expect("send should succeed");

        assert!(process_fetch_results(
            &mut rx,
            &mut app,
            &mut pending_fetches,
            &mut worktree_refresh_pending,
            &mut worktree_refresh_requested,
        ));
        assert!(!app.is_worktree_removal_pending());
        assert!(worktree_refresh_requested);
        let status = app
            .get_status_message()
            .expect("success status should be visible");
        assert_eq!(status.text, "Removed worktree '/repo/feature-test'");
        assert!(!status.is_error);
    }

    #[test]
    fn test_process_fetch_results_maps_removal_operation_and_task_errors() {
        for task_error in [false, true] {
            let target = removal_target();
            let mut app = App::new(vec![], vec![]);
            app.update(Msg::EnterWorktreeRemovalMode {
                worktree: target.clone(),
            });
            let (tx, mut rx) = mpsc::unbounded_channel();
            let mut pending_fetches = HashSet::new();
            let mut worktree_refresh_pending = false;
            let mut worktree_refresh_requested = false;
            let error = if task_error {
                FetchResult::WorktreeRemovalTaskError {
                    error: "Worktree removal task failed: join failed".to_string(),
                }
            } else {
                FetchResult::WorktreeRemovalError {
                    error: "worktree is locked".to_string(),
                }
            };
            tx.send(error).expect("send should succeed");

            assert!(process_fetch_results(
                &mut rx,
                &mut app,
                &mut pending_fetches,
                &mut worktree_refresh_pending,
                &mut worktree_refresh_requested,
            ));
            assert!(!app.is_worktree_removal_pending());
            assert!(worktree_refresh_requested);
            assert!(matches!(
                app.mode(),
                crate::tui::app::AppMode::ErrorPopup(message)
                    if message == if task_error {
                        "Worktree removal task failed: join failed"
                    } else {
                        "worktree is locked"
                    }
            ));
        }
    }

    // --- trigger_worktree_refresh --------------------------------------------

    #[test]
    fn test_refresh_request_is_retained_while_inventory_is_pending() {
        let git_repo = GitRepo::fixture(FixtureGitRepo::new());
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut worktree_refresh_pending = true;
        let mut worktree_refresh_requested = false;

        trigger_worktree_refresh(
            &git_repo,
            &tx,
            &mut worktree_refresh_pending,
            &mut worktree_refresh_requested,
        );

        assert!(worktree_refresh_pending);
        assert!(worktree_refresh_requested);
        assert!(rx.try_recv().is_err());
    }

    // --- trigger_worktree_removal --------------------------------------------

    #[test]
    fn test_removal_trigger_rejects_concurrent_operation() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut removal_pending = true;

        trigger_worktree_removal("/repo".into(), removal_target(), &tx, &mut removal_pending);

        assert!(removal_pending);
        assert!(rx.try_recv().is_err());
    }

    // --- fetch scheduling ----------------------------------------------------

    #[test]
    fn test_background_branch_and_work_item_fetches_skip_worktree_view() {
        let branch = BranchInfo {
            key: "refs/heads/feature/117".to_string(),
            display_name: "feature/117".to_string(),
            branch_name: "feature/117".to_string(),
            remote_name: None,
            scope: BranchScope::Local,
            work_item_id: Some(101),
            is_current: false,
            is_protected: false,
            is_stale: false,
        };
        let mut app = App::new(vec![branch], vec![]);
        app.update(Msg::ToggleWorktreeView);

        let client = AzureDevOpsClient::new_fixture(format!(
            "{}/docs/tapes/demo-work-items.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("demo work item fixture should load");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut pending_fetches = HashSet::new();

        trigger_work_item_fetch(&mut app, &client, &tx, &mut pending_fetches);

        assert!(pending_fetches.is_empty());
        assert!(rx.try_recv().is_err());
        assert!(matches!(
            app.get_work_item_status(101),
            WorkItemStatus::NotFetched
        ));

        let git_repo = GitRepo::fixture(FixtureGitRepo::new());
        fetch_branch_status_if_needed(&mut app, &git_repo);

        assert!(
            app.get_branch_status_error("refs/heads/feature/117")
                .is_none()
        );
        assert!(app.get_status_message().is_none());
    }

    // --- apply_branch_status_result ------------------------------------------

    #[test]
    fn test_apply_branch_status_result_caches_error_and_sets_status_message() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);

        apply_branch_status_result(
            &mut app,
            "refs/remotes/origin/feature/1",
            "origin/feature/1",
            Err(anyhow::anyhow!("git lookup failed")),
        );

        assert_eq!(
            app.get_branch_status_error("refs/remotes/origin/feature/1"),
            Some("git lookup failed")
        );

        let status = app
            .get_status_message()
            .expect("status message should be set");
        assert!(status.is_error);
        assert!(status.text.contains("origin/feature/1"));
    }

    #[test]
    fn test_apply_branch_status_result_preserves_error_chain() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);

        let error = anyhow::anyhow!("remote ref unavailable").context("git status failed");
        apply_branch_status_result(
            &mut app,
            "refs/remotes/origin/feature/1",
            "origin/feature/1",
            Err(error),
        );

        assert_eq!(
            app.get_branch_status_error("refs/remotes/origin/feature/1"),
            Some("git status failed: remote ref unavailable")
        );
        let status = app
            .get_status_message()
            .expect("status message should be set");
        assert_eq!(
            status.text,
            "Could not load branch info for 'origin/feature/1': git status failed: remote ref unavailable"
        );
    }

    #[test]
    fn test_apply_branch_status_result_does_not_overwrite_status_for_same_error() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);

        apply_branch_status_result(
            &mut app,
            "refs/remotes/origin/feature/1",
            "origin/feature/1",
            Err(anyhow::anyhow!("git lookup failed")),
        );

        app.set_status_message(
            "Deleted branch".to_string(),
            false,
            timing::STATUS_DURATION_SECS,
        );

        apply_branch_status_result(
            &mut app,
            "refs/remotes/origin/feature/1",
            "origin/feature/1",
            Err(anyhow::anyhow!("git lookup failed")),
        );

        let status = app
            .get_status_message()
            .expect("status message should be preserved");
        assert!(!status.is_error);
        assert_eq!(status.text, "Deleted branch");
    }

    #[test]
    fn test_apply_branch_status_result_updates_status_when_error_changes() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);

        apply_branch_status_result(
            &mut app,
            "refs/remotes/origin/feature/1",
            "origin/feature/1",
            Err(anyhow::anyhow!("git lookup failed")),
        );

        app.set_status_message(
            "Deleted branch".to_string(),
            false,
            timing::STATUS_DURATION_SECS,
        );

        apply_branch_status_result(
            &mut app,
            "refs/remotes/origin/feature/1",
            "origin/feature/1",
            Err(anyhow::anyhow!("repo locked")),
        );

        let status = app
            .get_status_message()
            .expect("updated error message should be visible");
        assert!(status.is_error);
        assert!(status.text.contains("repo locked"));
    }
}
