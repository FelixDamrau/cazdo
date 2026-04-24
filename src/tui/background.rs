use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::mpsc;

use super::app::{App, WorkItemStatus};
use super::theme::timing;
use crate::azure_devops::{AzureDevOpsClient, WorkItem};
use crate::git::{GitRepo, list_origin_remote_heads_in_dir};

const REMOTE_FRESHNESS_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) enum FetchResult {
    Success { id: u32, work_item: WorkItem },
    Error { id: u32, error: String },
    RemoteFreshnessSuccess { live_branches: HashSet<String> },
    RemoteFreshnessError { error: String },
}

pub(super) fn process_fetch_results(
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
            app.set_remote_freshness_error(error.to_string());
            return;
        }
    };

    tokio::spawn(async move {
        let _ = tx.send(fetch_remote_freshness(repo_dir).await);
    });
}

pub(super) fn trigger_work_item_fetch(
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
                    Err(error) => FetchResult::Error {
                        id: wi_id,
                        error: error.to_string(),
                    },
                };
                let _ = tx.send(result);
            });
        }
    }
}

pub(super) fn fetch_branch_status_if_needed(app: &mut App, git_repo: &GitRepo) {
    if let Some(branch) = app.selected_branch() {
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
            error: error.to_string(),
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
            let error_text = error.to_string();
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
    use crate::azure_devops::{WorkItem, WorkItemState, WorkItemType};
    use crate::git::BranchScope;
    use crate::tui::app::BranchInfo;

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

    #[test]
    fn test_process_fetch_results_loads_work_item_and_clears_pending_fetch() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut pending_fetches = HashSet::from([42]);

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

        process_fetch_results(&mut rx, &mut app, &mut pending_fetches);

        assert!(pending_fetches.is_empty());
        match app.get_work_item_status(42) {
            WorkItemStatus::Loaded(work_item) => assert_eq!(work_item.title, "Loaded item"),
            _ => panic!("expected loaded work item"),
        }
    }

    #[test]
    fn test_process_fetch_results_sets_remote_freshness_error_and_status() {
        let mut app = App::new(vec![remote_branch(false)], vec![]);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut pending_fetches = HashSet::new();

        tx.send(FetchResult::RemoteFreshnessError {
            error: "origin unreachable".to_string(),
        })
        .expect("send should succeed");

        process_fetch_results(&mut rx, &mut app, &mut pending_fetches);

        assert_eq!(app.remote_freshness_error(), Some("origin unreachable"));
        let status = app
            .get_status_message()
            .expect("remote freshness error should surface in footer");
        assert!(status.is_error);
        assert_eq!(status.text, "Could not verify origin branches");
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
