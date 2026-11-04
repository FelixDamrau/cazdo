use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::mpsc;

use super::app::{App, Msg, WorkItemStatus};
use super::theme::timing;
use crate::azure_devops::{AzureDevOpsClient, WorkItem};
use crate::git::{GitRepo, WorktreeInfo, list_origin_remote_heads_in_dir};

const REMOTE_FRESHNESS_TIMEOUT: Duration = Duration::from_secs(10);

enum FetchResult {
    WorkItem {
        id: u32,
        request: u64,
        result: Result<WorkItem, String>,
    },
    RemoteFreshness(Result<HashSet<String>, String>),
    Worktrees(Result<Vec<WorktreeInfo>, String>),
}

enum WorktreeRefresh {
    Idle,
    Running,
    RunAgain,
}

/// Owns background requests through completion. Callers request refreshes and
/// poll once per event-loop turn; channel and retry ordering stay here.
pub(super) struct BackgroundLoads {
    client: AzureDevOpsClient,
    repo_dir: Result<PathBuf, String>,
    tx: mpsc::UnboundedSender<FetchResult>,
    rx: mpsc::UnboundedReceiver<FetchResult>,
    pending_work_items: HashMap<u32, u64>,
    next_request: u64,
    worktree_refresh: WorktreeRefresh,
}

impl BackgroundLoads {
    pub(super) fn new(client: AzureDevOpsClient, repo_dir: Result<PathBuf>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            client,
            repo_dir: repo_dir.map_err(|error| error.to_string()),
            tx,
            rx,
            pending_work_items: HashMap::new(),
            next_request: 0,
            worktree_refresh: WorktreeRefresh::Idle,
        }
    }

    /// Apply completions before deciding which selected data still needs loading.
    pub(super) fn poll(&mut self, app: &mut App) {
        let mut refresh_again = false;
        while let Ok(result) = self.rx.try_recv() {
            match result {
                FetchResult::WorkItem {
                    id,
                    request,
                    result,
                } => {
                    // A refresh supersedes even a result already queued for delivery.
                    if self.pending_work_items.get(&id) != Some(&request) {
                        continue;
                    }
                    self.pending_work_items.remove(&id);
                    match result {
                        Ok(work_item) => app.set_work_item_loaded(id, work_item),
                        Err(error) => app.set_work_item_error(id, error),
                    }
                }
                FetchResult::RemoteFreshness(result) => match result {
                    Ok(live_branches) => app.set_remote_freshness(live_branches),
                    Err(error) => {
                        app.set_remote_freshness_error(error);
                        app.update(Msg::SetBackgroundError(
                            "Could not verify origin branches".to_string(),
                        ));
                    }
                },
                FetchResult::Worktrees(result) => {
                    refresh_again = matches!(self.worktree_refresh, WorktreeRefresh::RunAgain);
                    self.worktree_refresh = WorktreeRefresh::Idle;
                    app.update(match result {
                        Ok(worktrees) => Msg::SetWorktrees(worktrees),
                        Err(error) => Msg::SetWorktreeError(error),
                    });
                }
            }
        }

        // Start the coalesced request after draining, including synchronous errors.
        if refresh_again {
            self.refresh_worktrees();
        }
        if let Some(id) = app.selected_work_item_id()
            && matches!(app.get_work_item_status(id), WorkItemStatus::NotFetched)
            && !self.pending_work_items.contains_key(&id)
        {
            self.start_work_item(app, id);
        }
        self.check_remote_freshness(app);
    }

    /// Invalidate any older fetch and reload when this work item is selected.
    pub(super) fn refresh_work_item(&mut self, app: &mut App, id: u32) {
        self.pending_work_items.remove(&id);
        app.reset_work_item(id);
    }

    /// Requests during an inventory coalesce into one subsequent inventory.
    pub(super) fn refresh_worktrees(&mut self) {
        match self.worktree_refresh {
            WorktreeRefresh::Running | WorktreeRefresh::RunAgain => {
                self.worktree_refresh = WorktreeRefresh::RunAgain;
                return;
            }
            WorktreeRefresh::Idle => self.worktree_refresh = WorktreeRefresh::Running,
        }

        let repo_dir = match &self.repo_dir {
            Ok(repo_dir) => repo_dir.clone(),
            Err(error) => {
                let _ = self.tx.send(FetchResult::Worktrees(Err(error.clone())));
                return;
            }
        };
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result =
                tokio::task::spawn_blocking(move || GitRepo::list_worktrees_at(&repo_dir)).await;
            let result = match result {
                Ok(result) => result.map_err(|error| error.to_string()),
                Err(error) => Err(format!("Worktree inventory task failed: {error}")),
            };
            let _ = tx.send(FetchResult::Worktrees(result));
        });
    }

    fn start_work_item(&mut self, app: &mut App, id: u32) {
        let request = self.next_request;
        self.next_request += 1;
        self.pending_work_items.insert(id, request);
        app.set_work_item_loading(id);

        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = client
                .get_work_item(id)
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(FetchResult::WorkItem {
                id,
                request,
                result,
            });
        });
    }

    fn check_remote_freshness(&self, app: &mut App) {
        if !app.should_check_remote_freshness() {
            return;
        }

        let repo_dir = match &self.repo_dir {
            Ok(repo_dir) => repo_dir.clone(),
            Err(error) => {
                app.set_remote_freshness_error(error.clone());
                return;
            }
        };
        app.set_remote_freshness_checking();
        let tx = self.tx.clone();

        tokio::spawn(async move {
            let _ = tx.send(FetchResult::RemoteFreshness(
                fetch_remote_freshness(repo_dir).await,
            ));
        });
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

async fn fetch_remote_freshness(repo_dir: PathBuf) -> Result<HashSet<String>, String> {
    let task = tokio::task::spawn_blocking(move || list_origin_remote_heads_in_dir(&repo_dir));

    let join_result = match tokio::time::timeout(REMOTE_FRESHNESS_TIMEOUT, task).await {
        Ok(join_result) => join_result,
        Err(_) => {
            return Err("Network timeout checking origin branches".to_string());
        }
    };

    let branch_result = match join_result {
        Ok(branch_result) => branch_result,
        Err(_) => {
            return Err("Task panicked while checking origin branches".to_string());
        }
    };

    branch_result.map_err(|error| error.to_string())
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
mod tests;
