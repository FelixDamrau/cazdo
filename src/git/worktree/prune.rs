//! Deleting stale administrative metadata for a worktree whose directory is
//! already gone. `validate_worktree_prune` is the pure gate over an untrusted
//! inventory entry; `prune_metadata` re-checks the same facts against live
//! repository state before touching anything on disk.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use git2::{Repository, WorktreeLockStatus};

use super::inventory::valid_head_metadata;
use super::paths::{
    linked_worktree_admin_path, linked_worktree_path, main_worktree_path, worktree_paths_equal,
};
use super::{WorktreeIdentity, WorktreeInfo, WorktreeState};

pub(crate) fn validate_worktree_prune(entry: &WorktreeInfo) -> Result<&str, String> {
    if entry.is_main {
        return Err("Cannot prune the main worktree".to_string());
    }
    let Some(name) = entry.identity.linked_name() else {
        return Err(
            "Cannot prune worktree: selected entry is not a valid linked worktree".to_string(),
        );
    };
    if name.trim().is_empty() {
        return Err(
            "Cannot prune worktree: selected entry is not a valid linked worktree".to_string(),
        );
    }
    if entry.is_current {
        return Err("Cannot prune the current worktree".to_string());
    }
    if entry.is_locked() {
        return Err(format!(
            "Cannot prune locked worktree '{}': {}",
            entry.name(),
            entry.lock_reason.as_deref().unwrap_or("worktree is locked")
        ));
    }
    if !matches!(entry.state, WorktreeState::Missing) {
        return Err(format!(
            "Cannot prune worktree '{}': only entries marked missing and prunable are allowed",
            entry.name()
        ));
    }
    if !entry.prunable {
        return Err(format!(
            "Cannot prune worktree '{}': stale metadata is not marked prunable",
            entry.name()
        ));
    }
    Ok(name)
}

/// Prune only the administrative directory for a missing, safely identifiable
/// linked worktree. No force flags are used, and the working-tree path is
/// never touched.
///
/// The inventory entry that led here may be stale, so every fact it implied is
/// re-checked against the live repository. The gates below run in order and
/// each one only rejects; nothing is written until all of them pass.
pub(crate) fn prune_metadata(
    repo: &Repository,
    identity: &WorktreeIdentity,
    expected_path: &Path,
) -> Result<()> {
    let Some(name) = identity.linked_name() else {
        bail!("Cannot prune the main worktree; only linked worktrees are supported");
    };
    let worktree = repo
        .find_worktree(name)
        .with_context(|| format!("Cannot open linked worktree metadata '{name}'"))?;
    let path = worktree.path().to_path_buf();

    ensure_metadata_path_matches(name, &path, expected_path)?;
    ensure_not_protected(repo, &path)?;
    ensure_unlocked(&worktree, name)?;
    ensure_directory_gone(name, &path)?;
    ensure_admin_metadata_intact(repo, name, &path)?;
    ensure_prunable(&worktree, name)?;

    worktree
        .prune(None)
        .with_context(|| format!("Failed to prune stale metadata for worktree '{name}'"))
}

/// The entry the user selected must still name the worktree git knows about.
fn ensure_metadata_path_matches(name: &str, path: &Path, expected_path: &Path) -> Result<()> {
    if !worktree_paths_equal(path, expected_path) {
        bail!(
            "Cannot prune worktree '{}': selected path '{}' no longer matches metadata path '{}'",
            name,
            expected_path.display(),
            path.display()
        );
    }
    Ok(())
}

/// The main worktree and the one we are running in are never prunable, however
/// the inventory entry described them.
fn ensure_not_protected(repo: &Repository, path: &Path) -> Result<()> {
    if let Some(main_path) =
        main_worktree_path(repo).context("Cannot determine main worktree path before pruning")?
        && worktree_paths_equal(path, &main_path)
    {
        bail!("Cannot prune the main worktree");
    }
    if let Some(current_path) = repo.workdir()
        && worktree_paths_equal(path, current_path)
    {
        bail!("Cannot prune the current worktree");
    }
    Ok(())
}

fn ensure_unlocked(worktree: &git2::Worktree, name: &str) -> Result<()> {
    match worktree
        .is_locked()
        .with_context(|| format!("Cannot determine lock status for worktree '{name}'"))?
    {
        WorktreeLockStatus::Unlocked => Ok(()),
        WorktreeLockStatus::Locked(reason) => bail!(
            "Cannot prune locked worktree '{}': {}",
            name,
            reason.unwrap_or_else(|| "worktree is locked".to_string())
        ),
    }
}

/// Pruning metadata for a worktree that is actually still on disk would leave
/// the user with an orphaned directory, so refuse it.
fn ensure_directory_gone(name: &str, path: &Path) -> Result<()> {
    if path.exists() {
        bail!(
            "Cannot prune worktree '{}': path '{}' still exists; only missing worktrees are supported",
            name,
            path.display()
        );
    }
    Ok(())
}

/// The admin directory must be complete and self-consistent — a well formed
/// `gitdir` pointing back at the recorded path, and a readable `HEAD`. Half
/// written metadata is left alone for a human to look at.
fn ensure_admin_metadata_intact(repo: &Repository, name: &str, path: &Path) -> Result<()> {
    let admin_path = linked_worktree_admin_path(repo, name).ok_or_else(|| {
        anyhow!("Cannot determine administrative metadata path for worktree '{name}'")
    })?;
    if !admin_path.is_dir() {
        bail!(
            "Cannot prune worktree '{}': administrative metadata '{}' is malformed",
            name,
            admin_path.display()
        );
    }

    let gitdir = fs::read_to_string(admin_path.join("gitdir")).with_context(|| {
        format!("Cannot prune worktree '{name}': administrative metadata is missing gitdir")
    })?;
    if gitdir.trim().is_empty() || !gitdir.trim_end().ends_with(".git") {
        bail!(
            "Cannot prune worktree '{}': administrative gitdir metadata is malformed",
            name
        );
    }
    let linked_path = linked_worktree_path(repo, name).ok_or_else(|| {
        anyhow!(
            "Cannot prune worktree '{}': administrative gitdir metadata is malformed",
            name
        )
    })?;
    if !worktree_paths_equal(&linked_path, path) {
        bail!(
            "Cannot prune worktree '{}': administrative path does not match recorded worktree path '{}'",
            name,
            path.display()
        );
    }

    let head = fs::read_to_string(admin_path.join("HEAD")).with_context(|| {
        format!("Cannot prune worktree '{name}': administrative metadata is missing HEAD")
    })?;
    if !valid_head_metadata(&head) {
        bail!(
            "Cannot prune worktree '{}': administrative HEAD metadata is malformed",
            name
        );
    }
    Ok(())
}

/// Last gate: libgit2 must independently agree the metadata is stale.
fn ensure_prunable(worktree: &git2::Worktree, name: &str) -> Result<()> {
    let prunable = worktree
        .is_prunable(None)
        .with_context(|| format!("Cannot determine whether worktree '{name}' is prunable"))?;
    if !prunable {
        bail!(
            "Cannot prune worktree '{}': metadata is not marked prunable",
            name
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo::live::current_local_branch_name;
    use crate::git::repo::{GitBackend, LiveGitRepo};
    use crate::git::testing::{add_worktree_at, init_test_repo};
    use git2::BranchType;
    use std::fs;

    #[test]
    fn test_prune_missing_worktree_metadata_preserves_path_and_branch_with_spaces() {
        let (repo, repo_path, oid) = init_test_repo("worktree-prune-spaces");
        let worktree_path = repo_path.join("missing linked path");
        add_worktree_at(
            &repo,
            oid,
            "feature/preserved",
            "linked with spaces",
            &worktree_path,
        );
        fs::remove_dir_all(&worktree_path).expect("missing worktree path should be removed");

        let entry = repo
            .list_worktrees()
            .expect("inventory should succeed")
            .into_iter()
            .find(|entry| entry.linked_name() == Some("linked with spaces"))
            .expect("linked entry should be present");
        assert!(entry.state.is_missing());
        assert!(entry.prunable);

        let admin_path = repo
            .repo()
            .path()
            .join("worktrees")
            .join("linked with spaces");
        repo.prune_worktree_metadata_by_identity(&entry.identity, &entry.path)
            .expect("missing worktree metadata should be pruned");

        assert!(!worktree_path.exists());
        assert!(!admin_path.exists());
        assert!(
            repo.repo()
                .find_branch("feature/preserved", BranchType::Local)
                .is_ok(),
            "pruning metadata must preserve the associated branch"
        );
        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn test_prune_worktree_metadata_rejects_locked_entry_without_mutating_metadata() {
        let (repo, repo_path, oid) = init_test_repo("worktree-prune-locked");
        let worktree_path = repo_path.with_extension("locked-wt");
        add_worktree_at(&repo, oid, "feature/locked", "locked", &worktree_path);
        repo.repo()
            .find_worktree("locked")
            .expect("worktree should exist")
            .lock(Some("in use"))
            .expect("worktree should lock");
        fs::remove_dir_all(&worktree_path).expect("missing worktree path should be removed");
        let admin_path = repo.repo().path().join("worktrees").join("locked");
        let head_before =
            fs::read_to_string(admin_path.join("HEAD")).expect("HEAD metadata should exist");

        let entry = repo
            .list_worktrees()
            .expect("inventory should succeed")
            .into_iter()
            .find(|entry| entry.linked_name() == Some("locked"))
            .expect("linked entry should be present");
        let error = repo
            .prune_worktree_metadata_by_identity(&entry.identity, &entry.path)
            .expect_err("locked metadata must be protected");

        assert!(error.to_string().contains("locked"));
        assert!(admin_path.exists());
        assert_eq!(
            fs::read_to_string(admin_path.join("HEAD")).expect("HEAD metadata should remain"),
            head_before
        );
        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn test_prune_worktree_metadata_rejects_valid_entry_without_mutating_metadata() {
        let (repo, repo_path, oid) = init_test_repo("worktree-prune-valid");
        let worktree_path = repo_path.with_extension("valid-wt");
        add_worktree_at(&repo, oid, "feature/valid", "valid", &worktree_path);
        let admin_path = repo.repo().path().join("worktrees").join("valid");
        let entry = repo
            .list_worktrees()
            .expect("inventory should succeed")
            .into_iter()
            .find(|entry| entry.linked_name() == Some("valid"))
            .expect("linked entry should be present");

        let error = repo
            .prune_worktree_metadata_by_identity(&entry.identity, &entry.path)
            .expect_err("existing worktree metadata must be protected");

        assert!(error.to_string().contains("missing"));
        assert!(worktree_path.exists());
        assert!(admin_path.exists());
        let _ = fs::remove_dir_all(&worktree_path);
        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn test_prune_worktree_metadata_rejects_malformed_missing_entry_without_mutating_metadata() {
        let (repo, repo_path, oid) = init_test_repo("worktree-prune-malformed");
        let worktree_path = repo_path.with_extension("malformed-wt");
        add_worktree_at(&repo, oid, "feature/malformed", "malformed", &worktree_path);
        fs::remove_dir_all(&worktree_path).expect("missing worktree path should be removed");
        let admin_path = repo.repo().path().join("worktrees").join("malformed");
        fs::write(admin_path.join("HEAD"), "not a valid HEAD\n")
            .expect("malformed HEAD should be written");

        let entry = repo
            .list_worktrees()
            .expect("inventory should succeed")
            .into_iter()
            .find(|entry| entry.linked_name() == Some("malformed"))
            .expect("linked entry should be present");
        assert!(entry.state.is_missing());
        assert!(entry.prunable);
        let error = repo
            .prune_worktree_metadata_by_identity(&entry.identity, &entry.path)
            .expect_err("malformed metadata must be protected");

        assert!(error.to_string().contains("malformed"));
        assert!(admin_path.exists());
        assert_eq!(
            fs::read_to_string(admin_path.join("HEAD")).expect("HEAD metadata should remain"),
            "not a valid HEAD\n"
        );
        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn test_prune_worktree_metadata_rejects_main_entry_without_mutating_repository() {
        let (repo, repo_path, _oid) = init_test_repo("worktree-prune-main");
        let main = repo
            .list_worktrees()
            .expect("inventory should succeed")
            .into_iter()
            .find(|entry| entry.is_main)
            .expect("main entry should be present");
        let initial_branch = current_local_branch_name(repo.repo())
            .expect("current branch lookup should succeed")
            .expect("test repository should have a local branch");

        let error = repo
            .prune_worktree_metadata_by_identity(&main.identity, &main.path)
            .expect_err("main worktree must remain protected");

        assert!(error.to_string().contains("main"));
        assert!(
            repo.repo()
                .find_branch(&initial_branch, BranchType::Local)
                .is_ok()
        );
        assert!(repo_path.exists());
        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn test_prune_worktree_metadata_rejects_stale_selected_path_without_mutation() {
        let (repo, repo_path, oid) = init_test_repo("worktree-prune-stale-path");
        let worktree_path = repo_path.with_extension("stale-wt");
        add_worktree_at(&repo, oid, "feature/stale", "stale", &worktree_path);
        fs::remove_dir_all(&worktree_path).expect("missing worktree path should be removed");
        let admin_path = repo.repo().path().join("worktrees").join("stale");
        let entry = repo
            .list_worktrees()
            .expect("inventory should succeed")
            .into_iter()
            .find(|entry| entry.linked_name() == Some("stale"))
            .expect("linked entry should be present");
        let stale_path = repo_path.join("different missing path");

        let error = repo
            .prune_worktree_metadata_by_identity(&entry.identity, &stale_path)
            .expect_err("stale selected path must be rejected");

        assert!(error.to_string().contains("selected path"));
        assert!(admin_path.exists());
        let _ = fs::remove_dir_all(repo_path);
    }

    #[test]
    fn test_prune_worktree_metadata_rejects_current_worktree_after_path_disappears() {
        let (repo, repo_path, oid) = init_test_repo("worktree-prune-current");
        let worktree_path = repo_path.with_extension("current-wt");
        add_worktree_at(&repo, oid, "feature/current", "current", &worktree_path);
        let current_repo = LiveGitRepo::from_repo(
            Repository::open(&worktree_path).expect("linked repository should open"),
        );
        fs::remove_dir_all(&worktree_path).expect("current worktree path should be removed");
        let entry = current_repo
            .list_worktrees()
            .expect("inventory should succeed")
            .into_iter()
            .find(|entry| entry.linked_name() == Some("current"))
            .expect("current linked entry should be present");
        let admin_path = repo.repo().path().join("worktrees").join("current");

        let error = current_repo
            .prune_worktree_metadata_by_identity(&entry.identity, &entry.path)
            .expect_err("current worktree metadata must remain protected");

        assert!(error.to_string().contains("current"));
        assert!(admin_path.exists());
        let _ = fs::remove_dir_all(repo_path);
    }
}
