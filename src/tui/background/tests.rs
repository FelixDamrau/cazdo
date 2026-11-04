use super::*;
use crate::git::{BranchScope, FixtureGitRepo};
use crate::tui::app::BranchInfo;
use tempfile::TempDir;

fn fixture_client() -> AzureDevOpsClient {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("work-items.json");
    std::fs::write(
        &path,
        r#"[{"id":42,"fields":{"System.Title":"Loaded item","System.WorkItemType":"Task","System.State":"Active"}}]"#,
    )
    .unwrap();
    AzureDevOpsClient::new_fixture(path).unwrap()
}

fn local_branch(id: u32) -> BranchInfo {
    BranchInfo {
        key: format!("refs/heads/feature/{id}"),
        display_name: format!("feature/{id}"),
        branch_name: format!("feature/{id}"),
        remote_name: None,
        scope: BranchScope::Local,
        work_item_id: Some(id),
        is_current: false,
        is_protected: false,
        is_stale: false,
    }
}

fn remote_branch(name: &str) -> BranchInfo {
    BranchInfo {
        key: format!("refs/remotes/origin/{name}"),
        display_name: format!("origin/{name}"),
        branch_name: name.to_string(),
        remote_name: Some("origin".to_string()),
        scope: BranchScope::Remote,
        work_item_id: None,
        is_current: false,
        is_protected: false,
        is_stale: false,
    }
}

fn init_repo(path: &std::path::Path) {
    let repo = git2::Repository::init(path).unwrap();
    repo.set_head("refs/heads/feature/1").unwrap();
    let signature = git2::Signature::now("Test User", "test@example.com").unwrap();
    let tree_id = repo.treebuilder(None).unwrap().write().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, "init", &tree, &[])
        .unwrap();
}

async fn poll_until(background: &mut BackgroundLoads, app: &mut App, ready: impl Fn(&App) -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            background.poll(app);
            if ready(app) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("background request should complete");
}

fn loaded(app: &App) -> bool {
    matches!(app.get_work_item_status(42), WorkItemStatus::Loaded(_))
}

#[tokio::test]
async fn loads_selected_work_item_and_preserves_cached_success() {
    let mut app = App::new(vec![local_branch(42)], vec![]);
    let mut background = BackgroundLoads::new(fixture_client(), Err(anyhow::anyhow!("unused")));

    background.poll(&mut app);
    assert!(matches!(
        app.get_work_item_status(42),
        WorkItemStatus::Loading
    ));
    poll_until(&mut background, &mut app, loaded).await;

    background.poll(&mut app);
    match app.get_work_item_status(42) {
        WorkItemStatus::Loaded(item) => assert_eq!(item.title, "Loaded item"),
        _ => panic!("cached item should stay loaded"),
    }

    background.refresh_work_item(&mut app, 42);
    assert!(matches!(
        app.get_work_item_status(42),
        WorkItemStatus::NotFetched
    ));
    background.poll(&mut app);
    assert!(matches!(
        app.get_work_item_status(42),
        WorkItemStatus::Loading
    ));
    poll_until(&mut background, &mut app, loaded).await;
}

#[tokio::test]
async fn suppresses_duplicate_inflight_work_items_even_if_display_status_is_reset() {
    let mut app = App::new(vec![local_branch(42)], vec![]);
    let mut background = BackgroundLoads::new(fixture_client(), Err(anyhow::anyhow!("unused")));

    background.poll(&mut app);
    app.reset_work_item(42);
    background.poll(&mut app);
    // No second request marks the item loading. The first still completes.
    assert!(matches!(
        app.get_work_item_status(42),
        WorkItemStatus::NotFetched
    ));
    poll_until(&mut background, &mut app, loaded).await;
}

#[tokio::test]
async fn explicit_refresh_supersedes_a_completion_waiting_to_be_applied() {
    let mut app = App::new(vec![local_branch(42)], vec![]);
    let mut background = BackgroundLoads::new(fixture_client(), Err(anyhow::anyhow!("unused")));

    background.poll(&mut app);
    // The fixture task runs to completion without I/O on this current-thread runtime.
    tokio::task::yield_now().await;
    background.refresh_work_item(&mut app, 42);
    background.poll(&mut app);
    assert!(matches!(
        app.get_work_item_status(42),
        WorkItemStatus::Loading
    ));
    poll_until(&mut background, &mut app, loaded).await;
}

#[tokio::test]
async fn work_item_errors_wait_for_explicit_refresh_before_retrying() {
    let mut app = App::new(vec![local_branch(99)], vec![]);
    let mut background = BackgroundLoads::new(fixture_client(), Err(anyhow::anyhow!("unused")));
    let failed = |app: &App| matches!(app.get_work_item_status(99), WorkItemStatus::Error(_));

    poll_until(&mut background, &mut app, failed).await;
    background.poll(&mut app);
    match app.get_work_item_status(99) {
        WorkItemStatus::Error(error) => assert!(error.contains("#99 not found")),
        _ => panic!("error should stay cached"),
    }

    background.refresh_work_item(&mut app, 99);
    background.poll(&mut app);
    assert!(matches!(
        app.get_work_item_status(99),
        WorkItemStatus::Loading
    ));
    poll_until(&mut background, &mut app, failed).await;
}

#[test]
fn coalesces_requests_into_exactly_one_followup_after_inventory_setup_errors() {
    let git_repo = GitRepo::fixture(FixtureGitRepo::new());
    let mut background = BackgroundLoads::new(fixture_client(), git_repo.repo_dir());
    let mut app = App::new(vec![], vec![]);

    background.refresh_worktrees();
    background.refresh_worktrees();
    background.refresh_worktrees();

    for _ in 0..2 {
        app.update(Msg::ClearStatus);
        background.poll(&mut app);
        let status = app
            .get_status_message()
            .expect("inventory error should be applied");
        assert!(status.is_error);
        assert_eq!(status.text, "fixture git repo: repo_dir unsupported");
    }

    app.update(Msg::ClearStatus);
    background.poll(&mut app);
    assert!(app.get_status_message().is_none());
    background.refresh_worktrees();
    background.poll(&mut app);
    assert!(app.get_status_message().is_some());
}

#[tokio::test]
async fn retains_a_coalesced_inventory_after_success_and_allows_later_refreshes() {
    let dir = TempDir::new().unwrap();
    init_repo(dir.path());
    let mut background = BackgroundLoads::new(fixture_client(), Ok(dir.path().to_path_buf()));
    let mut app = App::new(vec![], vec![]);

    background.refresh_worktrees();
    background.refresh_worktrees();
    background.refresh_worktrees();
    poll_until(&mut background, &mut app, |app| !app.worktrees().is_empty()).await;
    let first_inventory = app.worktrees().to_vec();

    // The followup has been scheduled, but cannot execute until this task yields.
    app.update(Msg::SetWorktrees(vec![]));
    poll_until(&mut background, &mut app, |app| !app.worktrees().is_empty()).await;
    assert_eq!(app.worktrees(), first_inventory);

    app.update(Msg::SetWorktrees(vec![]));
    background.refresh_worktrees();
    poll_until(&mut background, &mut app, |app| !app.worktrees().is_empty()).await;
    assert_eq!(app.worktrees(), first_inventory);
}

#[tokio::test]
async fn retains_a_coalesced_inventory_after_task_error_and_recovers() {
    let dir = TempDir::new().unwrap();
    let mut background = BackgroundLoads::new(fixture_client(), Ok(dir.path().to_path_buf()));
    let mut app = App::new(vec![], vec![]);

    background.refresh_worktrees();
    background.refresh_worktrees();
    poll_until(&mut background, &mut app, |app| {
        app.get_status_message().is_some()
    })
    .await;
    let error = app.get_status_message().unwrap();
    assert!(error.is_error);
    assert!(error.text.contains("Failed to discover repository"));

    // Repair the local dependency before the queued followup gets to run.
    init_repo(dir.path());
    poll_until(&mut background, &mut app, |app| !app.worktrees().is_empty()).await;
    assert!(app.worktrees()[0].is_main);
}

#[tokio::test]
async fn origin_errors_show_in_footer_and_retry_when_remote_view_is_reentered() {
    let dir = TempDir::new().unwrap();
    let local = dir.path().join("local");
    let origin = dir.path().join("origin");
    init_repo(&local);
    init_repo(&origin);
    let mut background = BackgroundLoads::new(fixture_client(), Ok(local.clone()));
    let mut app = App::new(
        vec![remote_branch("feature/1"), remote_branch("gone")],
        vec![],
    );

    background.poll(&mut app);
    assert!(!app.remote_freshness_is_checking());
    assert!(app.remote_freshness_error().is_none());
    app.update(Msg::ToggleView);
    background.poll(&mut app);
    assert!(app.remote_freshness_is_checking());
    poll_until(&mut background, &mut app, |app| {
        app.remote_freshness_error().is_some()
    })
    .await;
    let status = app.get_status_message().unwrap();
    assert!(status.is_error);
    assert_eq!(status.text, "Could not verify origin branches");

    background.poll(&mut app);
    assert!(!app.remote_freshness_is_checking());
    git2::Repository::open(&local)
        .unwrap()
        .remote("origin", origin.to_str().unwrap())
        .unwrap();
    app.update(Msg::ToggleView);
    app.update(Msg::ToggleView);
    background.poll(&mut app);
    assert!(app.remote_freshness_is_checking());
    poll_until(&mut background, &mut app, |app| {
        !app.remote_freshness_is_checking()
    })
    .await;
    assert!(app.remote_freshness_error().is_none());
    let branches = app.visible_branches();
    assert!(!branches[0].is_stale);
    assert!(branches[1].is_stale);

    app.update(Msg::ToggleView);
    app.update(Msg::ToggleView);
    background.poll(&mut app);
    assert!(!app.remote_freshness_is_checking());
    assert!(!app.should_check_remote_freshness());
}

#[test]
fn origin_setup_errors_keep_the_existing_footer_and_can_retry_on_reentry() {
    let git_repo = GitRepo::fixture(FixtureGitRepo::new());
    let mut background = BackgroundLoads::new(fixture_client(), git_repo.repo_dir());
    let mut app = App::new(vec![remote_branch("feature/1")], vec![]);
    app.set_status_message(
        "Keep footer".to_string(),
        false,
        timing::STATUS_DURATION_SECS,
    );
    app.update(Msg::ToggleView);

    background.poll(&mut app);
    assert_eq!(
        app.remote_freshness_error(),
        Some("fixture git repo: repo_dir unsupported")
    );
    assert_eq!(app.get_status_message().unwrap().text, "Keep footer");
    app.update(Msg::ToggleView);
    app.update(Msg::ToggleView);
    assert!(app.should_check_remote_freshness());
    background.poll(&mut app);
    assert!(app.remote_freshness_error().is_some());
}

#[test]
fn branch_status_error_preserves_existing_footer_until_error_changes() {
    let mut app = App::new(vec![remote_branch("feature/1")], vec![]);
    let key = "refs/remotes/origin/feature/1";
    let display = "origin/feature/1";

    apply_branch_status_result(
        &mut app,
        key,
        display,
        Err(anyhow::anyhow!("git lookup failed")),
    );
    assert_eq!(app.get_branch_status_error(key), Some("git lookup failed"));
    let status = app.get_status_message().unwrap();
    assert!(status.is_error);
    assert!(status.text.contains(display));

    app.set_status_message(
        "Deleted branch".to_string(),
        false,
        timing::STATUS_DURATION_SECS,
    );
    apply_branch_status_result(
        &mut app,
        key,
        display,
        Err(anyhow::anyhow!("git lookup failed")),
    );
    assert_eq!(app.get_status_message().unwrap().text, "Deleted branch");

    apply_branch_status_result(&mut app, key, display, Err(anyhow::anyhow!("repo locked")));
    assert!(app.get_status_message().unwrap().is_error);
    assert!(
        app.get_status_message()
            .unwrap()
            .text
            .contains("repo locked")
    );
}
