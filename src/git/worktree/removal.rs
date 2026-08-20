//! Removing a live linked worktree, directory and all.
//!
//! The inventory entry handed in is untrusted UI state that may be seconds
//! stale, so removal is a chain of gates run against the *live* repository, in
//! this order: is the entry well formed, is the worktree still registered at
//! that path, is it safe to remove at all, does its HEAD still match what the
//! user saw, and is its directory actually writable. Only then does it prune.

use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};
use git2::{Repository, WorktreeLockStatus, WorktreePruneOptions};

use super::paths::{main_worktree_path, worktree_paths_equal};
use super::status::{cleanliness, submodules};
use super::{WorktreeCleanliness, WorktreeInfo, WorktreeSubmodules};
use crate::git::branch::short_sha;

/// Validate the inventory snapshot before attempting removal.
///
/// The live worktree metadata and repository contents must still be rechecked
/// before pruning because this snapshot may be stale.
pub(crate) fn validate_worktree_removal(entry: &WorktreeInfo) -> Result<&str, String> {
    if entry.is_main {
        return Err(format!(
            "Cannot remove worktree '{}': the main worktree is protected",
            entry.path.display()
        ));
    }
    if entry.is_current {
        return Err(format!(
            "Cannot remove worktree '{}': the current worktree is protected",
            entry.path.display()
        ));
    }
    let name = entry
        .linked_name()
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| "Cannot remove malformed worktree entry: missing linked name".to_string())?;
    if entry.path.as_os_str().is_empty() {
        return Err(format!(
            "Cannot remove malformed worktree '{}': path is empty",
            name
        ));
    }
    if !entry.state.is_valid() {
        return Err(format!(
            "Cannot remove worktree '{}': inventory state is '{}'; refresh worktree inventory first",
            entry.path.display(),
            entry.state.label()
        ));
    }
    match &entry.cleanliness {
        WorktreeCleanliness::Clean => {}
        WorktreeCleanliness::Dirty(reasons) => {
            let reasons = reasons
                .iter()
                .map(|reason| reason.label())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "Cannot remove worktree '{}': it has uncommitted changes ({reasons})",
                entry.path.display()
            ));
        }
        WorktreeCleanliness::Unknown(error) => {
            return Err(format!(
                "Cannot remove worktree '{}': status is unknown ({error})",
                entry.path.display()
            ));
        }
    }
    match &entry.submodules {
        WorktreeSubmodules::None => {}
        WorktreeSubmodules::Present => {
            return Err(format!(
                "Cannot remove worktree '{}': it contains submodules",
                entry.path.display()
            ));
        }
        WorktreeSubmodules::Unknown(error) => {
            return Err(format!(
                "Cannot remove worktree '{}': submodule status is unknown ({error})",
                entry.path.display()
            ));
        }
    }
    if let Some(reason) = &entry.lock_reason {
        return Err(format!(
            "Cannot remove worktree '{}': it is locked ({reason})",
            entry.path.display()
        ));
    }
    if entry.prunable {
        return Err(format!(
            "Cannot remove worktree '{}': it is prunable or missing; refresh worktree inventory first",
            entry.path.display()
        ));
    }
    Ok(name)
}

pub(crate) fn remove_linked_worktree(repo: &Repository, selected: &WorktreeInfo) -> Result<()> {
    let name = validate_worktree_removal(selected).map_err(anyhow::Error::msg)?;
    let worktree = find_registered_worktree(repo, selected, name)?;
    validate_worktree_metadata(repo, &worktree)?;
    validate_linked_worktree(&worktree, selected)?;

    let actual_path = worktree.path();
    preflight_worktree_removal(actual_path)?;

    // GIT_WORKTREE_PRUNE_VALID allows pruning a still-valid worktree; this is intentional.
    let mut options = WorktreePruneOptions::new();
    options.valid(true).working_tree(true);
    if let Err(error) = worktree.prune(Some(&mut options)) {
        if worktree_metadata_missing(repo, name) {
            anyhow::bail!(
                "Worktree '{}' was deregistered but its directory could not be fully deleted ({}). Remove it manually.",
                actual_path.display(),
                error
            );
        }
        return Err(error)
            .with_context(|| format!("Failed to remove worktree '{}'", actual_path.display()));
    }
    Ok(())
}

// --- Gate 1: still registered where the inventory said ---------------------

fn find_registered_worktree(
    repo: &Repository,
    selected: &WorktreeInfo,
    name: &str,
) -> Result<git2::Worktree> {
    let worktree_names = repo
        .worktrees()
        .context("Cannot remove worktree: failed to list linked worktrees")?;
    if !worktree_names
        .iter()
        .flatten()
        .any(|candidate| candidate == name)
    {
        anyhow::bail!(
            "Cannot remove worktree '{}': linked worktree is no longer registered; refresh worktree inventory",
            selected.path.display()
        );
    }

    let worktree = repo.find_worktree(name).with_context(|| {
        format!(
            "Cannot remove worktree '{}': linked entry is unavailable",
            name
        )
    })?;
    let actual_path = worktree.path();
    if !worktree_paths_equal(&selected.path, actual_path) {
        anyhow::bail!(
            "Cannot remove worktree '{}': inventory path is stale (current path is '{}'); refresh worktree inventory",
            selected.path.display(),
            actual_path.display()
        );
    }
    Ok(worktree)
}

// --- Gate 2: safe to remove at all ----------------------------------------

fn validate_worktree_metadata(repo: &Repository, worktree: &git2::Worktree) -> Result<()> {
    let actual_path = worktree.path();
    if let Some(main_path) = main_worktree_path(repo)
        .context("Cannot remove worktree: failed to determine the main worktree path")?
        && worktree_paths_equal(actual_path, &main_path)
    {
        anyhow::bail!(
            "Cannot remove worktree '{}': the main worktree is protected",
            actual_path.display()
        );
    }

    if let Some(current_path) = repo.workdir()
        && worktree_paths_equal(actual_path, current_path)
    {
        anyhow::bail!(
            "Cannot remove worktree '{}': the current worktree is protected",
            actual_path.display()
        );
    }

    worktree.validate().with_context(|| {
        format!(
            "Cannot remove worktree '{}': it is invalid",
            actual_path.display()
        )
    })?;
    if worktree.is_prunable(None).with_context(|| {
        format!(
            "Cannot remove worktree '{}': unable to determine whether it is prunable",
            actual_path.display()
        )
    })? {
        anyhow::bail!(
            "Cannot remove worktree '{}': it is prunable or missing",
            actual_path.display()
        );
    }

    match worktree.is_locked().with_context(|| {
        format!(
            "Cannot remove worktree '{}': unable to determine lock status",
            actual_path.display()
        )
    })? {
        WorktreeLockStatus::Unlocked => {}
        WorktreeLockStatus::Locked(reason) => {
            anyhow::bail!(
                "Cannot remove worktree '{}': it is locked ({})",
                actual_path.display(),
                reason.as_deref().unwrap_or("locked")
            );
        }
    }

    Ok(())
}

// --- Gate 3: live state still matches the snapshot ------------------------

fn validate_linked_worktree(worktree: &git2::Worktree, selected: &WorktreeInfo) -> Result<()> {
    let actual_path = worktree.path();
    let linked_repo = Repository::open_from_worktree(worktree).with_context(|| {
        format!(
            "Cannot remove worktree '{}': unable to open its repository",
            actual_path.display()
        )
    })?;
    validate_worktree_head(&linked_repo, selected, actual_path)?;
    validate_worktree_cleanliness(&linked_repo, actual_path)?;
    validate_worktree_submodules(&linked_repo, actual_path)?;
    Ok(())
}

fn validate_worktree_head(
    linked_repo: &Repository,
    selected: &WorktreeInfo,
    actual_path: &Path,
) -> Result<()> {
    let (branch, detached_short_sha) = worktree_head_identity(linked_repo).with_context(|| {
        format!(
            "Cannot remove worktree '{}': unable to read its HEAD",
            actual_path.display()
        )
    })?;
    if branch.as_deref() != selected.branch.as_deref()
        || detached_short_sha.as_deref() != selected.detached_short_sha.as_deref()
    {
        anyhow::bail!(
            "Cannot remove worktree '{}': branch or HEAD changed; refresh worktree inventory",
            actual_path.display()
        );
    }
    Ok(())
}

fn validate_worktree_cleanliness(linked_repo: &Repository, actual_path: &Path) -> Result<()> {
    match cleanliness(linked_repo) {
        WorktreeCleanliness::Clean => Ok(()),
        WorktreeCleanliness::Dirty(reasons) => {
            let reasons = reasons
                .iter()
                .map(|reason| reason.label())
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::bail!(
                "Cannot remove worktree '{}': it has uncommitted changes ({reasons})",
                actual_path.display()
            );
        }
        WorktreeCleanliness::Unknown(error) => {
            anyhow::bail!(
                "Cannot remove worktree '{}': status is unknown ({error})",
                actual_path.display()
            );
        }
    }
}

fn validate_worktree_submodules(linked_repo: &Repository, actual_path: &Path) -> Result<()> {
    match submodules(linked_repo) {
        WorktreeSubmodules::None => Ok(()),
        WorktreeSubmodules::Present => {
            anyhow::bail!(
                "Cannot remove worktree '{}': it contains submodules",
                actual_path.display()
            );
        }
        WorktreeSubmodules::Unknown(error) => {
            anyhow::bail!(
                "Cannot remove worktree '{}': submodule status is unknown ({error})",
                actual_path.display()
            );
        }
    }
}

fn worktree_head_identity(repo: &Repository) -> Result<(Option<String>, Option<String>)> {
    let head = repo.head().context("Failed to read worktree HEAD")?;
    if head.is_branch() {
        return Ok((
            Some(
                head.shorthand()
                    .context("Failed to read worktree branch name")?
                    .to_string(),
            ),
            None,
        ));
    }

    Ok((
        None,
        head.target()
            .map(|oid| short_sha(&oid.to_string()).to_string()),
    ))
}

// --- Gate 4: the directory can actually be deleted -------------------------

static REMOVAL_PROBE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn preflight_worktree_removal(path: &Path) -> Result<()> {
    let metadata = fs::metadata(path).with_context(|| {
        format!(
            "Cannot remove worktree '{}': unable to access its directory before removal",
            path.display()
        )
    })?;
    if !metadata.is_dir() {
        bail!(
            "Cannot remove worktree '{}': its path is not a directory",
            path.display()
        );
    }

    #[cfg(windows)]
    ensure_worktree_tree_writable(path)?;

    let probe_id = REMOVAL_PROBE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut probe_path = None;
    for attempt in 0..16 {
        let candidate = path.join(format!(
            ".cazdo-removal-probe-{}-{probe_id}-{attempt}",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                drop(file);
                probe_path = Some(candidate);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "Cannot remove worktree '{}': its directory is not writable",
                        path.display()
                    )
                });
            }
        }
    }

    let Some(probe_path) = probe_path else {
        bail!(
            "Cannot remove worktree '{}': unable to create a unique writability probe",
            path.display()
        );
    };
    fs::remove_file(&probe_path).with_context(|| {
        format!(
            "Cannot remove worktree '{}': unable to clean up its writability probe",
            path.display()
        )
    })?;
    Ok(())
}

#[cfg(windows)]
fn ensure_worktree_tree_writable(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.permissions().readonly() {
        bail!(
            "Cannot remove worktree '{}': its path or contents are read-only",
            path.display()
        );
    }
    if metadata.file_type().is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            ensure_worktree_tree_writable(&entry.path())?;
        }
    }
    Ok(())
}

fn worktree_metadata_missing(repo: &Repository, name: &str) -> bool {
    if repo.find_worktree(name).is_err() {
        return true;
    }

    let common_git_dir = if repo.is_worktree() {
        let linked_git_dir = repo.path();
        let Some(common) = fs::read_to_string(linked_git_dir.join("commondir")).ok() else {
            return false;
        };
        let common = PathBuf::from(common.trim());
        if common.is_absolute() {
            common
        } else {
            linked_git_dir.join(common)
        }
    } else {
        repo.path().to_path_buf()
    };
    matches!(
        fs::metadata(common_git_dir.join("worktrees").join(name)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo::LiveGitRepo;
    use crate::git::testing::{add_worktree_at, init_test_repo, linked_entry};
    use crate::git::{GitRepo, WorktreeIdentity};
    use git2::BranchType;

    #[test]
    fn test_remove_worktree_deletes_spaced_path_and_preserves_branch() {
        let (repo, repo_path, oid) = init_test_repo("remove-worktree-success");
        let worktree_path = repo_path.join("linked tree with spaces");
        add_worktree_at(
            &repo,
            oid,
            "feature/preserved-branch",
            "linked name with spaces",
            &worktree_path,
        );
        let target = linked_entry(&repo, "linked name with spaces");

        GitRepo::remove_worktree_at(repo_path.clone(), target)
            .expect("clean linked worktree should be removed");

        assert!(!worktree_path.exists());
        assert!(
            !repo
                .repo()
                .worktrees()
                .expect("worktree list should load")
                .iter()
                .flatten()
                .any(|name| name == "linked name with spaces")
        );
        assert!(
            repo.repo()
                .find_branch("feature/preserved-branch", BranchType::Local)
                .is_ok(),
            "removing a worktree must preserve its branch"
        );
        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn test_remove_worktree_rejects_main_and_current_worktrees() {
        let (repo, repo_path, oid) = init_test_repo("remove-worktree-protected");
        let main = repo
            .list_worktrees()
            .expect("worktree inventory should succeed")
            .into_iter()
            .find(|entry| entry.is_main)
            .expect("main worktree should be present");
        let main_error = GitRepo::remove_worktree_at(repo_path.clone(), main)
            .expect_err("main worktree should be protected");
        assert!(main_error.to_string().contains("main worktree"));
        assert!(repo_path.exists());

        let linked_path = repo_path.join("current linked");
        add_worktree_at(
            &repo,
            oid,
            "feature/current-linked",
            "current-linked",
            &linked_path,
        );
        let linked_repo = LiveGitRepo::from_repo(
            Repository::open(&linked_path).expect("linked repository should open"),
        );
        let current = linked_repo
            .list_worktrees()
            .expect("linked inventory should succeed")
            .into_iter()
            .find(|entry| entry.is_current && !entry.is_main)
            .expect("current linked worktree should be present");
        let current_error = GitRepo::remove_worktree_at(repo_path.clone(), current)
            .expect_err("current worktree should be protected");
        assert!(current_error.to_string().contains("current worktree"));
        assert!(linked_path.exists());

        let _ = fs::remove_dir_all(&linked_path);
        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn test_remove_worktree_rejects_dirty_locked_and_submodule_entries() {
        let (repo, repo_path, oid) = init_test_repo("remove-worktree-unsafe");

        let dirty_path = repo_path.join("dirty");
        add_worktree_at(&repo, oid, "feature/dirty", "dirty", &dirty_path);
        fs::write(dirty_path.join("README.md"), "changed\n")
            .expect("tracked file should be modified");
        let dirty = linked_entry(&repo, "dirty");
        let dirty_error = GitRepo::remove_worktree_at(repo_path.clone(), dirty)
            .expect_err("dirty worktree should be protected");
        assert!(dirty_error.to_string().contains("uncommitted changes"));
        assert!(dirty_path.exists());

        let locked_path = repo_path.join("locked");
        add_worktree_at(&repo, oid, "feature/locked", "locked", &locked_path);
        repo.repo()
            .find_worktree("locked")
            .expect("locked worktree should exist")
            .lock(Some("in use"))
            .expect("worktree should lock");
        let locked = linked_entry(&repo, "locked");
        let locked_error = GitRepo::remove_worktree_at(repo_path.clone(), locked)
            .expect_err("locked worktree should be protected");
        assert!(locked_error.to_string().contains("locked"));
        assert!(locked_path.exists());

        let mut submodule = linked_entry(&repo, "locked");
        submodule.submodules = WorktreeSubmodules::Present;
        let submodule_error = GitRepo::remove_worktree_at(repo_path.clone(), submodule)
            .expect_err("worktrees containing submodules should be protected");
        assert!(submodule_error.to_string().contains("submodules"));
        assert!(locked_path.exists());

        let _ = fs::remove_dir_all(&dirty_path);
        let _ = fs::remove_dir_all(&locked_path);
        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn test_remove_worktree_revalidates_live_state_after_clean_snapshot() {
        let (repo, repo_path, oid) = init_test_repo("remove-worktree-live-validation");

        let dirty_path = repo_path.join("live-dirty");
        add_worktree_at(&repo, oid, "feature/live-dirty", "live-dirty", &dirty_path);
        let dirty = linked_entry(&repo, "live-dirty");
        assert!(dirty.state.is_valid());
        assert!(dirty.cleanliness.is_clean());
        assert_eq!(dirty.lock_reason, None);
        assert_eq!(dirty.submodules, WorktreeSubmodules::None);
        assert!(!dirty.prunable);
        fs::write(dirty_path.join("README.md"), "changed after snapshot\n")
            .expect("tracked file should be modified after snapshot");

        let dirty_error = GitRepo::remove_worktree_at(repo_path.clone(), dirty)
            .expect_err("live cleanliness validation should reject the worktree");
        assert!(dirty_error.to_string().contains("uncommitted changes"));
        assert!(dirty_path.exists());
        assert!(
            repo.repo().find_worktree("live-dirty").is_ok(),
            "live dirtiness rejection must preserve worktree metadata"
        );

        let locked_path = repo_path.join("live-locked");
        add_worktree_at(
            &repo,
            oid,
            "feature/live-locked",
            "live-locked",
            &locked_path,
        );
        let locked = linked_entry(&repo, "live-locked");
        assert!(locked.state.is_valid());
        assert!(locked.cleanliness.is_clean());
        assert_eq!(locked.lock_reason, None);
        assert_eq!(locked.submodules, WorktreeSubmodules::None);
        assert!(!locked.prunable);
        repo.repo()
            .find_worktree("live-locked")
            .expect("locked worktree should exist")
            .lock(Some("acquired after snapshot"))
            .expect("worktree should lock after snapshot");

        let locked_error = GitRepo::remove_worktree_at(repo_path.clone(), locked)
            .expect_err("live lock validation should reject the worktree");
        assert!(locked_error.to_string().contains("locked"));
        assert!(locked_path.exists());
        assert!(
            repo.repo().find_worktree("live-locked").is_ok(),
            "live lock rejection must preserve worktree metadata"
        );

        let missing_path = repo_path.join("live-missing");
        add_worktree_at(
            &repo,
            oid,
            "feature/live-missing",
            "live-missing",
            &missing_path,
        );
        let missing = linked_entry(&repo, "live-missing");
        assert!(missing.state.is_valid());
        assert!(missing.cleanliness.is_clean());
        assert_eq!(missing.lock_reason, None);
        assert_eq!(missing.submodules, WorktreeSubmodules::None);
        assert!(!missing.prunable);
        fs::remove_dir_all(&missing_path).expect("worktree path should disappear after snapshot");

        let missing_error = GitRepo::remove_worktree_at(repo_path.clone(), missing)
            .expect_err("live metadata validation should reject a missing worktree");
        assert!(missing_error.to_string().contains("invalid"));
        assert!(!missing_path.exists());
        assert!(
            repo.repo().find_worktree("live-missing").is_ok(),
            "live missing-path rejection must preserve worktree metadata"
        );

        let _ = fs::remove_dir_all(&dirty_path);
        let _ = fs::remove_dir_all(&locked_path);
        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn test_remove_worktree_rejects_missing_invalid_and_status_unknown_entries() {
        let (repo, repo_path, oid) = init_test_repo("remove-worktree-invalid");

        let missing_path = repo_path.join("missing");
        add_worktree_at(&repo, oid, "feature/missing", "missing", &missing_path);
        fs::remove_dir_all(&missing_path).expect("worktree directory should be removable");
        let missing = linked_entry(&repo, "missing");
        let missing_error = GitRepo::remove_worktree_at(repo_path.clone(), missing)
            .expect_err("missing worktree should be protected");
        assert!(missing_error.to_string().contains("state is 'missing'"));
        assert!(
            repo.repo().find_worktree("missing").is_ok(),
            "rejected missing worktree metadata should remain"
        );

        let invalid_path = repo_path.join("invalid");
        add_worktree_at(
            &repo,
            oid,
            "feature/invalid-removal",
            "invalid",
            &invalid_path,
        );
        fs::remove_dir_all(&invalid_path).expect("worktree directory should be removable");
        fs::write(&invalid_path, "not a worktree").expect("invalid path should remain present");
        let invalid = linked_entry(&repo, "invalid");
        let invalid_error = GitRepo::remove_worktree_at(repo_path.clone(), invalid)
            .expect_err("invalid worktree should be protected");
        assert!(invalid_error.to_string().contains("state is 'invalid'"));
        assert!(invalid_path.exists());

        let unknown_path = repo_path.join("unknown status");
        add_worktree_at(
            &repo,
            oid,
            "feature/status-unknown",
            "unknown",
            &unknown_path,
        );
        fs::write(unknown_path.join(".gitmodules"), "[submodule\n")
            .expect("malformed submodule config should be written");
        let unknown = linked_entry(&repo, "unknown");
        assert!(matches!(
            unknown.cleanliness,
            WorktreeCleanliness::Unknown(_)
        ));
        let unknown_error = GitRepo::remove_worktree_at(repo_path.clone(), unknown)
            .expect_err("unknown status should be protected");
        assert!(unknown_error.to_string().contains("status is unknown"));
        assert!(unknown_path.exists());

        let _ = fs::remove_file(&invalid_path);
        let _ = fs::remove_dir_all(&unknown_path);
        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn test_remove_worktree_rejects_stale_and_malformed_inventory_entries() {
        let (repo, repo_path, oid) = init_test_repo("remove-worktree-stale");
        let path = repo_path.join("stale");
        add_worktree_at(&repo, oid, "feature/stale", "stale", &path);
        let target = linked_entry(&repo, "stale");

        let mut stale_path = target.clone();
        stale_path.path = repo_path.join("different");
        let stale_error = GitRepo::remove_worktree_at(repo_path.clone(), stale_path)
            .expect_err("stale path should be protected");
        assert!(stale_error.to_string().contains("inventory path is stale"));
        assert!(path.exists());
        let mut changed_head = target.clone();
        changed_head.branch = Some("feature/other".to_string());

        let changed_head_error = GitRepo::remove_worktree_at(repo_path.clone(), changed_head)
            .expect_err("changed worktree HEAD should be protected");
        assert!(
            changed_head_error
                .to_string()
                .contains("branch or HEAD changed")
        );
        assert!(path.exists());

        let mut prunable = target.clone();
        prunable.prunable = true;
        let prunable_error = GitRepo::remove_worktree_at(repo_path.clone(), prunable)
            .expect_err("prunable worktree should be protected");
        assert!(prunable_error.to_string().contains("prunable"));
        assert!(path.exists());
        let mut malformed = target;
        malformed.identity = WorktreeIdentity::Linked {
            name: String::new(),
        };
        let malformed_error = GitRepo::remove_worktree_at(repo_path.clone(), malformed)
            .expect_err("malformed identity should be protected");
        assert!(malformed_error.to_string().contains("malformed"));
        assert!(path.exists());

        let _ = fs::remove_dir_all(&path);
        let _ = fs::remove_dir_all(repo_path);
    }
}
