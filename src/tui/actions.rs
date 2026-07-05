use anyhow::Result;

use super::app::{App, BranchInfo, BranchView, Msg, WorkItemStatus};
use super::theme::timing;
use crate::git::{BranchScope, DeleteResult, GitRepo, short_sha};

pub(super) fn open_current_work_item(app: &mut App) {
    open_current_work_item_with(app, open_url);
}

pub(super) fn execute_delete_branch(app: &mut App, git_repo: &GitRepo, branch: &BranchInfo) {
    match git_repo.delete_branch(
        branch.scope,
        &branch.branch_name,
        branch.remote_name.as_deref(),
        app.protected_patterns(),
    ) {
        Ok(DeleteResult::Local { commit_sha }) => {
            let restore_hint = format!("git checkout -b {} {}", branch.branch_name, commit_sha);
            app.update(Msg::BranchDeleted {
                key: branch.key.clone(),
                name: branch.display_name.clone(),
                restore_hint: Some(restore_hint),
            });
            app.set_status_message(
                format!(
                    "Deleted {} (was {})",
                    branch.display_name,
                    short_sha(&commit_sha)
                ),
                false,
                timing::STATUS_DURATION_SECS,
            );
        }
        Ok(DeleteResult::Remote) => {
            apply_remote_delete_result(
                app,
                branch,
                git_repo.prune_remote_tracking_branch(&branch.branch_name),
            );
        }
        Err(error) => app.set_status_message(error.to_string(), true, timing::STATUS_DURATION_SECS),
    }
}

pub(super) fn execute_prune_branch(app: &mut App, git_repo: &GitRepo, branch: &BranchInfo) {
    match git_repo.prune_remote_tracking_branch(&branch.branch_name) {
        Ok(()) => {
            app.update(Msg::BranchPruned {
                key: branch.key.clone(),
            });
            app.set_status_message(
                format!("Pruned stale tracking ref '{}'", branch.display_name),
                false,
                timing::STATUS_DURATION_SECS,
            );
        }
        Err(error) => app.set_status_message(error.to_string(), true, timing::STATUS_DURATION_SECS),
    }
}

pub(super) fn execute_checkout_branch(app: &mut App, git_repo: &GitRepo, branch: &BranchInfo) {
    if let Some(message) = stale_remote_checkout_error_message(branch) {
        app.set_status_message(message, true, timing::STATUS_DURATION_SECS);
        return;
    }

    match git_repo.checkout_branch(
        branch.scope,
        &branch.branch_name,
        branch.remote_name.as_deref(),
    ) {
        Ok(()) => {
            app.ensure_local_branch_exists(branch);
            app.update(Msg::SetCurrentBranch(branch.branch_name.clone()));
            if branch.scope == BranchScope::Remote || app.active_view() == BranchView::Local {
                app.update(Msg::SortBranches);
                app.focus_local_branch(&branch.branch_name);
            }
            app.set_status_message(
                format!("Switched to branch '{}'", branch.branch_name),
                false,
                timing::STATUS_DURATION_SECS,
            );
        }
        Err(error) => app.show_error_popup(error.to_string()),
    }
}

pub(super) fn stale_remote_checkout_error_message(branch: &BranchInfo) -> Option<String> {
    if branch.scope == BranchScope::Remote && branch.is_stale {
        Some(format!(
            "'{}' is stale (no longer on origin). Prune it first with 'd'.",
            branch.display_name
        ))
    } else {
        None
    }
}

fn open_current_work_item_with<F>(app: &mut App, open: F)
where
    F: FnOnce(&str) -> Result<()>,
{
    if let Some(wi_id) = app.selected_work_item_id()
        && let WorkItemStatus::Loaded(wi) = app.get_work_item_status(wi_id)
        && let Some(ref url) = wi.url
        && let Err(error) = open(url)
    {
        app.set_status_message(
            format!("Could not open work item in browser: {}", error),
            true,
            timing::STATUS_DURATION_SECS,
        );
    }
}

fn apply_remote_delete_result(app: &mut App, branch: &BranchInfo, prune_result: Result<()>) {
    let (message, is_error) = remote_delete_status_message(&branch.display_name, prune_result);

    if is_error {
        app.update(Msg::BranchDeletePruneFailed {
            key: branch.key.clone(),
            name: branch.display_name.clone(),
        });
    } else {
        app.update(Msg::BranchDeleted {
            key: branch.key.clone(),
            name: branch.display_name.clone(),
            restore_hint: None,
        });
    }

    app.set_status_message(message, is_error, timing::STATUS_DURATION_SECS);
}

fn remote_delete_status_message(display_name: &str, prune_result: Result<()>) -> (String, bool) {
    match prune_result {
        Ok(()) => (format!("Deleted remote branch '{}'", display_name), false),
        Err(error) => (
            format!(
                "Deleted remote branch '{}', but could not prune tracking ref: {}",
                display_name, error
            ),
            true,
        ),
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azure_devops::{WorkItem, WorkItemState, WorkItemType};
    use crate::tui::app::Msg;

    #[test]
    fn test_remote_delete_status_message_reports_prune_failure() {
        let (message, is_error) = remote_delete_status_message(
            "origin/feature/test",
            Err(anyhow::anyhow!("could not prune tracking ref")),
        );

        assert!(is_error);
        assert_eq!(
            message,
            "Deleted remote branch 'origin/feature/test', but could not prune tracking ref: could not prune tracking ref"
        );
    }

    #[test]
    fn test_remote_delete_status_message_reports_success() {
        let (message, is_error) = remote_delete_status_message("origin/feature/test", Ok(()));

        assert!(!is_error);
        assert_eq!(message, "Deleted remote branch 'origin/feature/test'");
    }

    #[test]
    fn test_remote_delete_with_prune_failure_keeps_branch_visible_and_marks_stale() {
        let branch = remote_branch(false);
        let mut app = App::new(vec![branch.clone()], vec![]);
        app.update(Msg::ToggleView);

        apply_remote_delete_result(
            &mut app,
            &branch,
            Err(anyhow::anyhow!("could not prune tracking ref")),
        );

        let branch = app
            .branch_by_key("refs/remotes/origin/feature/1")
            .expect("branch should remain visible");
        assert!(branch.is_stale);

        let status = app
            .get_status_message()
            .expect("status message should be set");
        assert!(status.is_error);
        assert!(status.text.contains("could not prune tracking ref"));
    }

    #[test]
    fn test_remote_delete_with_prune_failure_still_records_deleted_branch_summary() {
        let branch = remote_branch(false);
        let mut app = App::new(vec![branch.clone()], vec![]);
        app.update(Msg::ToggleView);

        apply_remote_delete_result(
            &mut app,
            &branch,
            Err(anyhow::anyhow!("could not prune tracking ref")),
        );

        assert_eq!(app.deleted_branches().len(), 1);
        assert_eq!(app.deleted_branches()[0].name, "origin/feature/1");
        assert_eq!(app.deleted_branches()[0].restore_hint, None);
    }

    #[test]
    fn test_focus_local_branch_after_checkout_clamps_when_branch_not_visible() {
        let remote_branch = remote_branch(false);
        let mut app = App::new(vec![], vec![]);
        app.set_selected_index_for_test(4);
        app.update(Msg::ToggleView);

        app.focus_local_branch(&remote_branch.branch_name);

        assert_eq!(app.active_view(), BranchView::Local);
        assert_eq!(app.scroll_offset(), 0);
        assert_eq!(app.selected_index(), 0);
    }

    #[test]
    fn test_focus_local_branch_after_checkout_selects_matching_local_branch() {
        let remote_branch = remote_branch(false);
        let local_branch = BranchInfo {
            key: "refs/heads/feature/1".to_string(),
            display_name: "feature/1".to_string(),
            branch_name: "feature/1".to_string(),
            remote_name: None,
            scope: BranchScope::Local,
            work_item_id: None,
            is_current: true,
            is_protected: false,
            is_stale: false,
        };
        let mut app = App::new(vec![local_branch], vec![]);
        app.update(Msg::ToggleView);

        app.focus_local_branch(&remote_branch.branch_name);

        assert_eq!(app.active_view(), BranchView::Local);
        assert_eq!(app.selected_index(), 0);
    }

    #[test]
    fn test_focus_local_branch_after_checkout_refocuses_sorted_current_branch() {
        let mut app = App::new(
            vec![
                BranchInfo {
                    key: "refs/heads/feature/1".to_string(),
                    display_name: "feature/1".to_string(),
                    branch_name: "feature/1".to_string(),
                    remote_name: None,
                    scope: BranchScope::Local,
                    work_item_id: None,
                    is_current: true,
                    is_protected: false,
                    is_stale: false,
                },
                BranchInfo {
                    key: "refs/heads/feature/4".to_string(),
                    display_name: "feature/4".to_string(),
                    branch_name: "feature/4".to_string(),
                    remote_name: None,
                    scope: BranchScope::Local,
                    work_item_id: None,
                    is_current: false,
                    is_protected: false,
                    is_stale: false,
                },
            ],
            vec![],
        );
        app.set_selected_index_for_test(1);

        app.update(Msg::SetCurrentBranch("feature/4".to_string()));
        app.update(Msg::SortBranches);
        app.focus_local_branch("feature/4");

        assert_eq!(app.active_view(), BranchView::Local);
        assert_eq!(app.scroll_offset(), 0);
        assert_eq!(app.selected_index(), 0);
        assert_eq!(app.visible_branches()[0].branch_name, "feature/4");
    }

    #[test]
    fn test_open_current_work_item_reports_browser_open_error() {
        let mut app = App::new(
            vec![BranchInfo {
                key: "refs/heads/feature/1".to_string(),
                display_name: "feature/1".to_string(),
                branch_name: "feature/1".to_string(),
                remote_name: None,
                scope: BranchScope::Local,
                work_item_id: Some(42),
                is_current: false,
                is_protected: false,
                is_stale: false,
            }],
            vec![],
        );
        app.set_work_item_loaded(
            42,
            WorkItem {
                id: 42,
                title: "Open me".to_string(),
                work_item_type: WorkItemType::Task,
                state: WorkItemState::Active,
                assigned_to: None,
                url: Some("https://example.test/items/42".to_string()),
                tags: vec![],
                rich_text_fields: vec![],
            },
        );

        open_current_work_item_with(&mut app, |_| Err(anyhow::anyhow!("xdg-open missing")));

        let status = app
            .get_status_message()
            .expect("status message should be set");
        assert!(status.is_error);
        assert_eq!(
            status.text,
            "Could not open work item in browser: xdg-open missing"
        );
    }

    #[test]
    fn test_stale_remote_checkout_error_message_reports_prune_hint() {
        let branch = remote_branch(true);

        let message = stale_remote_checkout_error_message(&branch).expect("stale remote message");

        assert_eq!(
            message,
            "'origin/feature/1' is stale (no longer on origin). Prune it first with 'd'."
        );
    }

    #[test]
    fn test_ensure_local_branch_exists_creates_checkout_target_branch() {
        let remote_branch = remote_branch(false);
        let mut app = App::new(vec![remote_branch.clone()], vec![]);

        app.ensure_local_branch_exists(&remote_branch);

        let local_branch = app
            .branch_by_key("refs/heads/feature/1")
            .expect("local branch should be created");
        assert_eq!(local_branch.scope, BranchScope::Local);
        assert_eq!(local_branch.display_name, "feature/1");
        assert_eq!(local_branch.remote_name, None);
        assert!(!local_branch.is_stale);
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
