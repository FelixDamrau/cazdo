//! Building the worktree list. `inventory` is the only entry point; it always
//! yields one entry per worktree, degrading to an `Unknown`/`Missing` entry
//! rather than failing, so a single broken worktree cannot blank the whole view.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use git2::{Repository, WorktreeLockStatus};

use super::paths::{
    linked_worktree_admin_path, linked_worktree_fallback_path, main_worktree_path,
    worktree_paths_equal,
};
use super::status::{cleanliness, submodules};
use super::{
    WorktreeCleanliness, WorktreeIdentity, WorktreeInfo, WorktreeState, WorktreeSubmodules,
};
use crate::error::format_error_chain;
use crate::git::branch::short_sha;

/// Build an inventory using only libgit2 repository/worktree/status APIs and
/// filesystem metadata. No Git command is spawned.
pub(crate) fn inventory(repo: &Repository) -> Result<Vec<WorktreeInfo>> {
    let current_path = repo.workdir().map(Path::to_path_buf);
    let main_path = main_worktree_path(repo).context("Failed to determine main worktree path")?;

    let mut entries = Vec::new();
    if let Some(main_path) = main_path {
        entries.push(inspect_entry(
            repo,
            WorktreeIdentity::Main,
            main_path,
            current_path.as_deref(),
            true,
        ));
    }

    let worktrees = repo
        .worktrees()
        .context("Failed to list linked worktrees")?;
    for name in worktrees.iter().flatten() {
        let identity = WorktreeIdentity::Linked {
            name: name.to_string(),
        };
        match repo.find_worktree(name) {
            Ok(worktree) => {
                let path = worktree.path().to_path_buf();
                let lock_reason = match worktree.is_locked() {
                    Ok(WorktreeLockStatus::Unlocked) => None,
                    Ok(WorktreeLockStatus::Locked(reason)) => {
                        Some(reason.unwrap_or_else(|| "locked".to_string()))
                    }
                    Err(error) => Some(format!(
                        "lock status unknown: {}",
                        format_error_chain(&error)
                    )),
                };
                let validated = worktree.validate();
                let prunable = worktree.is_prunable(None).unwrap_or(false);
                let state = if !path.exists() {
                    WorktreeState::Missing
                } else if let Err(error) = validated {
                    WorktreeState::Invalid(format_error_chain(&error))
                } else {
                    WorktreeState::Valid
                };

                entries.push(inspect_entry_with_state(
                    repo,
                    identity,
                    path,
                    current_path.as_deref(),
                    false,
                    lock_reason,
                    state,
                    prunable,
                ));
            }
            Err(error) => {
                let (branch, detached_short_sha) = metadata_head_identity(repo, name);
                entries.push(unknown_entry(
                    identity,
                    linked_worktree_fallback_path(repo, name),
                    current_path.as_deref(),
                    branch,
                    detached_short_sha,
                    format!(
                        "unable to open linked worktree: {}",
                        format_error_chain(&error)
                    ),
                ));
            }
        }
    }

    entries.sort_by(|left, right| {
        left.is_main
            .cmp(&right.is_main)
            .reverse()
            .then_with(|| left.name().cmp(right.name()))
    });
    Ok(entries)
}

// --- Per-entry inspection -------------------------------------------------

fn inspect_entry(
    inventory_repo: &Repository,
    identity: WorktreeIdentity,
    path: PathBuf,
    current_path: Option<&Path>,
    is_main: bool,
) -> WorktreeInfo {
    let state = if path.exists() {
        WorktreeState::Valid
    } else {
        WorktreeState::Missing
    };
    inspect_entry_with_state(
        inventory_repo,
        identity,
        path,
        current_path,
        is_main,
        None,
        state,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn inspect_entry_with_state(
    inventory_repo: &Repository,
    identity: WorktreeIdentity,
    path: PathBuf,
    current_path: Option<&Path>,
    is_main: bool,
    lock_reason: Option<String>,
    mut state: WorktreeState,
    prunable: bool,
) -> WorktreeInfo {
    let same_as_inventory =
        current_path.is_some_and(|current| worktree_paths_equal(current, &path));
    let (branch, detached_short_sha, cleanliness, submodules) = if same_as_inventory {
        let (branch, detached_short_sha) = head_identity(inventory_repo);
        let cleanliness = cleanliness(inventory_repo);
        let submodules = submodules(inventory_repo);
        (branch, detached_short_sha, cleanliness, submodules)
    } else {
        match Repository::open(&path) {
            Ok(repository) => {
                let (branch, detached_short_sha) = head_identity(&repository);
                let cleanliness = cleanliness(&repository);
                let submodules = submodules(&repository);
                (branch, detached_short_sha, cleanliness, submodules)
            }
            Err(error) => {
                let error = format_error_chain(&error);
                if path.exists() && state.is_valid() {
                    state = WorktreeState::Invalid(error.clone());
                }
                let cleanliness = WorktreeCleanliness::Unknown(format!(
                    "unable to open worktree repository: {error}"
                ));
                let submodules = WorktreeSubmodules::Unknown(error);
                if let Some(name) = identity.linked_name() {
                    let (branch, detached_short_sha) = metadata_head_identity(inventory_repo, name);
                    (branch, detached_short_sha, cleanliness, submodules)
                } else {
                    (None, None, cleanliness, submodules)
                }
            }
        }
    };

    WorktreeInfo {
        identity,
        path: path.clone(),
        branch,
        detached_short_sha,
        is_main,
        is_current: current_path.is_some_and(|current| worktree_paths_equal(current, &path)),
        cleanliness,
        lock_reason,
        state,
        prunable,
        submodules,
    }
}

fn unknown_entry(
    identity: WorktreeIdentity,
    path: PathBuf,
    current_path: Option<&Path>,
    branch: Option<String>,
    detached_short_sha: Option<String>,
    error: String,
) -> WorktreeInfo {
    WorktreeInfo {
        is_current: current_path.is_some_and(|current| worktree_paths_equal(current, &path)),
        is_main: false,
        identity,
        path,
        branch,
        detached_short_sha,
        cleanliness: WorktreeCleanliness::Unknown(error.clone()),
        lock_reason: None,
        submodules: WorktreeSubmodules::Unknown(error.clone()),
        state: WorktreeState::Unknown(error),
        prunable: false,
    }
}

// --- HEAD identity --------------------------------------------------------

fn head_identity(repo: &Repository) -> (Option<String>, Option<String>) {
    let Ok(head) = repo.head() else {
        return (None, None);
    };

    if head.is_branch() {
        (head.shorthand().map(str::to_string), None)
    } else {
        (
            None,
            head.target()
                .map(|oid| short_sha(&oid.to_string()).to_string()),
        )
    }
}

fn metadata_head_identity(repo: &Repository, name: &str) -> (Option<String>, Option<String>) {
    let Some(admin_path) = linked_worktree_admin_path(repo, name) else {
        return (None, None);
    };
    let head_path = admin_path.join("HEAD");
    let Ok(head) = fs::read_to_string(head_path) else {
        return (None, None);
    };
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref: refs/heads/") {
        (Some(reference.to_string()), None)
    } else {
        (
            None,
            (!head.is_empty()).then(|| short_sha(head).to_string()),
        )
    }
}

pub(super) fn valid_head_metadata(head: &str) -> bool {
    let head = head.trim();
    if let Some(branch) = head.strip_prefix("ref: refs/heads/") {
        !branch.is_empty() && !branch.contains(char::is_whitespace)
    } else {
        git2::Oid::from_str(head).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo::{GitBackend, LiveGitRepo};
    use crate::git::testing::{
        add_worktree_at, add_worktree_for_branch, init_bare_test_repo, init_test_repo,
        init_test_repo_with_external_git_dir, worktree_paths_match,
    };
    use crate::git::{GitRepo, WorktreeDirtyReason};
    use std::path::PathBuf;
    use tempfile::tempdir;

    // --- inventory -----------------------------------------------------------

    #[test]
    fn test_list_worktrees_synthesizes_main_and_preserves_linked_identity() {
        let (repo, repo_path, oid) = init_test_repo("worktree-inventory");
        let worktree_path = add_worktree_for_branch(&repo, &repo_path, oid, "feature/test");

        let inventory = repo
            .list_worktrees()
            .expect("worktree inventory should succeed");

        assert_eq!(inventory.len(), 2);
        let main = inventory
            .iter()
            .find(|entry| entry.is_main)
            .expect("main worktree should be synthesized");
        assert!(main.is_current);
        assert!(worktree_paths_match(&main.path, &repo_path));
        assert!(main.branch.is_some());
        let linked = inventory
            .iter()
            .find(|entry| entry.linked_name() == Some("linked-worktree"))
            .expect("linked identity should be retained");
        assert!(worktree_paths_match(&linked.path, &worktree_path));
        assert_eq!(linked.branch.as_deref(), Some("feature/test"));
        assert!(!linked.is_current);
        assert!(linked.cleanliness.is_clean());

        let _ = fs::remove_dir_all(&worktree_path);
        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn test_list_worktrees_from_bare_repository_has_only_linked_entries() {
        let (repo, repo_path, linked_path) = init_bare_test_repo("bare-worktree-inventory");

        assert!(worktree_paths_match(
            &repo
                .repo_dir()
                .expect("bare repository path should be available"),
            &repo_path
        ));
        let inventory = repo
            .list_worktrees()
            .expect("bare worktree inventory should succeed");

        let linked = inventory
            .first()
            .expect("linked worktree should be present");
        assert!(!linked.is_main);
        assert!(!linked.is_current);
        assert_eq!(linked.linked_name(), Some("linked-worktree"));
        assert!(worktree_paths_match(&linked.path, &linked_path));
        assert_eq!(linked.branch.as_deref(), Some("feature/bare"));
        assert!(linked.cleanliness.is_clean());
        assert!(matches!(linked.submodules, WorktreeSubmodules::None));

        let static_inventory =
            GitRepo::list_worktrees_at(&repo_path).expect("static bare inventory should succeed");
        assert_eq!(static_inventory, inventory);

        let _ = fs::remove_dir_all(&linked_path);
        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn test_list_worktrees_marks_linked_process_current_by_path() {
        let (repo, repo_path, oid) = init_test_repo("worktree-inventory-current");
        let worktree_path = add_worktree_for_branch(&repo, &repo_path, oid, "feature/test");
        let linked_repo = LiveGitRepo::from_repo(
            Repository::open(&worktree_path).expect("linked repository should open"),
        );

        let inventory = linked_repo
            .list_worktrees()
            .expect("linked worktree inventory should succeed");
        assert!(inventory.iter().any(|entry| {
            entry.is_current && worktree_paths_match(&entry.path, &worktree_path)
        }));
        assert!(
            inventory
                .iter()
                .any(|entry| entry.is_main && !entry.is_current)
        );
        let main = inventory
            .iter()
            .find(|entry| entry.is_main)
            .expect("main worktree should be listed");
        assert!(worktree_paths_equal(&main.path, &repo_path));

        let _ = fs::remove_dir_all(&worktree_path);
        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn test_list_worktrees_reports_dirty_locked_and_detached_states() {
        let (repo, repo_path, oid) = init_test_repo("worktree-inventory-statuses");
        let dirty_path = repo_path.with_extension("dirty-wt");
        add_worktree_at(&repo, oid, "feature/dirty", "dirty", &dirty_path);
        fs::write(dirty_path.join("README.md"), "modified\n").expect("tracked file should change");
        fs::write(dirty_path.join("untracked.txt"), "new\n").expect("untracked file should exist");
        repo.repo()
            .find_worktree("dirty")
            .expect("dirty worktree should exist")
            .lock(Some("owned by editor"))
            .expect("worktree should lock");

        let detached_path = repo_path.with_extension("detached-wt");
        add_worktree_at(&repo, oid, "feature/detached", "detached", &detached_path);
        Repository::open(&detached_path)
            .expect("detached worktree should open")
            .set_head_detached(oid)
            .expect("HEAD should detach");

        let inventory = repo.list_worktrees().expect("inventory should succeed");
        let dirty = inventory
            .iter()
            .find(|entry| entry.linked_name() == Some("dirty"))
            .expect("dirty worktree should be listed");
        let reasons = dirty.cleanliness.dirty_reasons();
        assert!(reasons.contains(&WorktreeDirtyReason::Worktree));
        assert!(reasons.contains(&WorktreeDirtyReason::Untracked));
        assert_eq!(dirty.lock_reason.as_deref(), Some("owned by editor"));
        assert_eq!(dirty.submodules, WorktreeSubmodules::None);

        let detached = inventory
            .iter()
            .find(|entry| entry.linked_name() == Some("detached"))
            .expect("detached worktree should be listed");
        assert_eq!(detached.branch, None);
        assert_eq!(
            detached.detached_short_sha.as_deref(),
            Some(short_sha(&oid.to_string()))
        );
        assert!(detached.state.is_valid());
        assert_eq!(detached.submodules, WorktreeSubmodules::None);

        let _ = fs::remove_dir_all(&dirty_path);
        let _ = fs::remove_dir_all(&detached_path);
        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn test_list_worktrees_preserves_unicode_and_spaced_paths() {
        let (repo, repo_path, oid) = init_test_repo("worktree-inventory-unicode-spaces");
        let path = repo_path.join("linked tree über");
        add_worktree_at(&repo, oid, "feature/unicode-space", "linked ü tree", &path);

        let inventory = repo
            .list_worktrees()
            .expect("worktree inventory should support Unicode and spaces");
        let linked = inventory
            .iter()
            .find(|entry| entry.linked_name() == Some("linked ü tree"))
            .expect("Unicode linked identity should be retained");
        assert!(worktree_paths_match(&linked.path, &path));
        assert_eq!(linked.branch.as_deref(), Some("feature/unicode-space"));
        assert!(linked.state.is_valid());
        assert!(linked.cleanliness.is_clean());

        let _ = fs::remove_dir_all(&path);
        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn test_list_worktrees_resolves_main_path_from_external_git_dir() {
        let (repo, repo_root, repo_path, oid) =
            init_test_repo_with_external_git_dir("worktree-inventory-external-git-dir");
        let linked_path = repo_path.with_extension("linked-wt");
        add_worktree_at(
            &repo,
            oid,
            "feature/external-git-dir",
            "linked-worktree",
            &linked_path,
        );
        let linked_repo = LiveGitRepo::from_repo(
            Repository::open(&linked_path).expect("linked repository should open"),
        );

        let inventory = linked_repo
            .list_worktrees()
            .expect("linked worktree inventory should succeed");
        let main = inventory
            .iter()
            .find(|entry| entry.is_main)
            .expect("main worktree should be listed");

        assert!(worktree_paths_match(&main.path, &repo_path));
        assert!(!main.is_current);

        let _ = fs::remove_dir_all(repo_root);
    }

    // --- degraded entries ----------------------------------------------------

    #[test]
    fn test_list_worktrees_marks_missing_linked_entry_without_losing_path() {
        let (repo, repo_path, oid) = init_test_repo("worktree-inventory-missing");
        let worktree_path = add_worktree_for_branch(&repo, &repo_path, oid, "feature/test");
        fs::remove_dir_all(&worktree_path).expect("worktree dir should be removed");

        let inventory = repo
            .list_worktrees()
            .expect("missing worktree inventory should succeed");
        let linked = inventory
            .iter()
            .find(|entry| entry.linked_name() == Some("linked-worktree"))
            .expect("missing linked entry should remain visible");
        assert!(worktree_paths_match(&linked.path, &worktree_path));
        assert!(linked.state.is_missing());
        assert!(linked.branch.as_deref() == Some("feature/test"));
        assert!(linked.prunable);
        assert!(matches!(linked.submodules, WorktreeSubmodules::Unknown(_)));

        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn test_list_worktrees_reports_invalid_linked_metadata() {
        let (repo, repo_path, oid) = init_test_repo("worktree-inventory-invalid");
        let worktree_path = repo_path.with_extension("invalid-wt");
        add_worktree_at(&repo, oid, "feature/invalid", "invalid", &worktree_path);
        fs::remove_dir_all(&worktree_path).expect("worktree directory should be removed");
        fs::write(&worktree_path, "not a worktree").expect("invalid path should remain present");

        let inventory = repo.list_worktrees().expect("inventory should succeed");
        let invalid = inventory
            .iter()
            .find(|entry| entry.linked_name() == Some("invalid"))
            .expect("invalid worktree should remain visible");
        assert!(matches!(invalid.state, WorktreeState::Invalid(_)));
        assert!(matches!(
            invalid.cleanliness,
            WorktreeCleanliness::Unknown(_)
        ));
        assert!(matches!(invalid.submodules, WorktreeSubmodules::Unknown(_)));
        assert!(worktree_paths_match(&invalid.path, &worktree_path));

        let _ = fs::remove_dir_all(&worktree_path);
        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn linked_metadata_fallback_preserves_identity_and_admin_path() {
        let dir = tempdir().expect("temp directory");
        let repo = Repository::init(dir.path()).expect("repository should initialize");
        let name = "broken";
        let admin_path = repo.path().join("worktrees").join(name);
        fs::create_dir_all(&admin_path).expect("worktree metadata directory should be created");
        fs::write(
            admin_path.join("HEAD"),
            "ref: refs/heads/feature/fallback\n",
        )
        .expect("worktree HEAD metadata should be written");

        let (branch, detached_short_sha) = metadata_head_identity(&repo, name);
        let entry = unknown_entry(
            WorktreeIdentity::Linked {
                name: name.to_string(),
            },
            linked_worktree_fallback_path(&repo, name),
            None,
            branch,
            detached_short_sha,
            "metadata unreadable".to_string(),
        );

        assert_eq!(entry.path, admin_path);
        assert_eq!(entry.branch.as_deref(), Some("feature/fallback"));
        assert_eq!(entry.ref_display(), "feature/fallback");
        assert!(matches!(entry.state, WorktreeState::Unknown(_)));
    }

    #[test]
    fn worktree_unknown_diagnostic_preserves_error_chain() {
        let error = anyhow::anyhow!("repository is unreadable").context("opening worktree failed");
        let diagnostic = format!(
            "unable to open linked worktree: {}",
            format_error_chain(&error)
        );
        let entry = unknown_entry(
            WorktreeIdentity::Linked {
                name: "broken".to_string(),
            },
            PathBuf::from("/repo/broken"),
            None,
            None,
            None,
            diagnostic.clone(),
        );

        assert_eq!(entry.state, WorktreeState::Unknown(diagnostic.clone()));
        assert_eq!(
            entry.cleanliness,
            WorktreeCleanliness::Unknown(diagnostic.clone())
        );
        assert_eq!(entry.submodules, WorktreeSubmodules::Unknown(diagnostic));
    }
}
