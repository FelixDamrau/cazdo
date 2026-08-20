//! The libgit2 adapter behind [`GitBackend`](super::GitBackend). Everything in
//! here touches a real repository; the pure decisions it defers to live in
//! [`crate::git::branch`].

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use git2::{BranchType, Repository};

use super::GitBackend;
use crate::git::branch::{
    BranchScope, BranchStatus, DeleteResult, ExistingLocalBranchAction, ORIGIN_REMOTE,
    RemoteStatus, RepoBranch, compare_branch_order, existing_local_branch_action,
    handle_upstream_setup_result, origin_branch_name, parse_ls_remote_heads, remote_branch_status,
};
use crate::git::worktree::{WorktreeIdentity, prune_metadata};
#[cfg(test)]
use crate::git::worktree::{WorktreeInfo, inventory};

pub(crate) struct LiveGitRepo {
    repo: Repository,
}

impl LiveGitRepo {
    /// Open the git repository in the current directory
    pub fn open_current_dir() -> Result<Self> {
        let repo = Repository::discover(".")
            .context("Not a git repository (or any of the parent directories)")?;
        Ok(Self { repo })
    }
    /// Build an adapter over an already-open repository. Test-only: production
    /// code goes through [`Self::open_current_dir`].
    #[cfg(test)]
    pub(crate) fn from_repo(repo: Repository) -> Self {
        Self { repo }
    }

    /// The underlying libgit2 handle, so tests can arrange the fixture
    /// repository this adapter was built on.
    #[cfg(test)]
    pub(crate) fn repo(&self) -> &Repository {
        &self.repo
    }

    #[cfg(test)]
    pub(crate) fn list_worktrees(&self) -> Result<Vec<WorktreeInfo>> {
        inventory(&self.repo)
    }
}

impl GitBackend for LiveGitRepo {
    fn list_branches(&self) -> Result<Vec<RepoBranch>> {
        let current = self.current_local_branch_name().ok().flatten();
        let mut branches: Vec<RepoBranch> = Vec::new();

        let local_iter = self
            .repo
            .branches(Some(BranchType::Local))
            .context("Failed to list local branches")?;

        for branch_result in local_iter {
            let (branch, _) = branch_result.context("Failed to read local branch")?;
            if let Some(name) = branch.name().ok().flatten() {
                branches.push(RepoBranch {
                    key: format!("refs/heads/{name}"),
                    display_name: name.to_string(),
                    branch_name: name.to_string(),
                    remote_name: None,
                    scope: BranchScope::Local,
                    is_current: current.as_ref().is_some_and(|c| c == name),
                });
            }
        }

        let remote_iter = self
            .repo
            .branches(Some(BranchType::Remote))
            .context("Failed to list remote branches")?;

        for branch_result in remote_iter {
            let (branch, _) = branch_result.context("Failed to read remote branch")?;
            let Some(name) = branch.name().ok().flatten() else {
                continue;
            };
            let Some(branch_name) = origin_branch_name(name) else {
                continue;
            };

            branches.push(RepoBranch {
                key: format!("refs/remotes/{ORIGIN_REMOTE}/{branch_name}"),
                display_name: format!("{ORIGIN_REMOTE}/{branch_name}"),
                branch_name: branch_name.to_string(),
                remote_name: Some(ORIGIN_REMOTE.to_string()),
                scope: BranchScope::Remote,
                is_current: false,
            });
        }

        branches.sort_by(compare_branch_order);

        Ok(branches)
    }

    /// Get status information for a branch.
    fn get_branch_status(
        &self,
        scope: BranchScope,
        branch_name: &str,
        remote_name: Option<&str>,
    ) -> Result<BranchStatus> {
        match scope {
            BranchScope::Local => self.get_local_branch_status(branch_name),
            BranchScope::Remote => self.get_remote_branch_status(branch_name, remote_name),
        }
    }

    /// Checkout a branch by scope/name.
    fn checkout_branch(
        &self,
        scope: BranchScope,
        branch_name: &str,
        remote_name: Option<&str>,
    ) -> Result<()> {
        self.ensure_checkout_safe()?;

        match scope {
            BranchScope::Local => self.checkout_local_branch(branch_name),
            BranchScope::Remote => self.checkout_remote_branch(branch_name, remote_name),
        }
    }

    /// Delete a branch by scope/name.
    fn delete_branch(
        &self,
        scope: BranchScope,
        branch_name: &str,
        remote_name: Option<&str>,
    ) -> Result<DeleteResult> {
        match scope {
            BranchScope::Local => self.delete_local_branch(branch_name),
            BranchScope::Remote => self.delete_remote_branch(branch_name, remote_name),
        }
    }

    /// Remove the local tracking ref for a stale remote branch.
    fn prune_remote_tracking_branch(&self, branch_name: &str) -> Result<()> {
        let tracking_ref = format!("{ORIGIN_REMOTE}/{branch_name}");

        let output = Command::new("git")
            .args(["branch", "-dr", &tracking_ref])
            .current_dir(self.command_dir()?)
            .output()
            .with_context(|| format!("Failed to run git branch -dr {tracking_ref}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let message = if !stderr.is_empty() { stderr } else { stdout };
            anyhow::bail!(
                "Failed to prune tracking ref '{}': {}",
                tracking_ref,
                message
            );
        }

        Ok(())
    }

    fn prune_worktree_metadata_by_identity(
        &self,
        identity: &WorktreeIdentity,
        expected_path: &Path,
    ) -> Result<()> {
        prune_metadata(&self.repo, identity, expected_path)
    }
    fn repo_dir(&self) -> Result<PathBuf> {
        Ok(self
            .repo
            .workdir()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.repo.path().to_path_buf()))
    }

    fn current_local_branch_name(&self) -> Result<Option<String>> {
        current_local_branch_name(&self.repo)
    }
}

// --- Branch queries and mutations -----------------------------------------

impl LiveGitRepo {
    fn command_dir(&self) -> Result<&Path> {
        self.repo
            .workdir()
            .or_else(|| self.repo.path().parent())
            .context("Failed to determine repository working directory")
    }

    fn get_local_branch_status(&self, branch_name: &str) -> Result<BranchStatus> {
        let branch = self
            .repo
            .find_branch(branch_name, BranchType::Local)
            .with_context(|| format!("Branch '{}' not found", branch_name))?;

        let (last_commit_author, last_commit_time) = last_commit_details(&branch);
        let remote_status = self.get_remote_status(&branch);

        Ok(BranchStatus {
            remote_status,
            last_commit_author,
            last_commit_time,
        })
    }

    fn get_remote_branch_status(
        &self,
        branch_name: &str,
        remote_name: Option<&str>,
    ) -> Result<BranchStatus> {
        let remote_name = remote_name.unwrap_or(ORIGIN_REMOTE);
        let branch = self
            .repo
            .find_branch(&format!("{remote_name}/{branch_name}"), BranchType::Remote)
            .with_context(|| format!("Remote branch '{remote_name}/{branch_name}' not found"))?;

        let (last_commit_author, last_commit_time) = last_commit_details(&branch);

        Ok(remote_branch_status(last_commit_author, last_commit_time))
    }

    fn get_remote_status(&self, branch: &git2::Branch) -> RemoteStatus {
        let upstream = match branch.upstream() {
            Ok(upstream) => upstream,
            Err(e) => {
                if e.code() == git2::ErrorCode::NotFound
                    && let Some(ref_name) = branch.get().name()
                    && self.repo.branch_upstream_name(ref_name).is_ok()
                {
                    return RemoteStatus::Gone;
                }
                return RemoteStatus::LocalOnly;
            }
        };

        let local_oid = match branch.get().resolve().and_then(|r| r.peel_to_commit()) {
            Ok(commit) => commit.id(),
            Err(_) => return RemoteStatus::LocalOnly,
        };

        let remote_oid = match upstream.get().resolve().and_then(|r| r.peel_to_commit()) {
            Ok(commit) => commit.id(),
            Err(_) => return RemoteStatus::Gone,
        };

        match self.repo.graph_ahead_behind(local_oid, remote_oid) {
            Ok((ahead, behind)) => match (ahead, behind) {
                (0, 0) => RemoteStatus::UpToDate,
                (ahead, 0) => RemoteStatus::Ahead(ahead),
                (0, behind) => RemoteStatus::Behind(behind),
                (ahead, behind) => RemoteStatus::Diverged { ahead, behind },
            },
            Err(_) => RemoteStatus::LocalOnly,
        }
    }

    fn ensure_checkout_safe(&self) -> Result<()> {
        let statuses = self
            .repo
            .statuses(None)
            .context("Failed to get repository status")?;

        let has_conflicts = statuses.iter().any(|s| {
            let status = s.status();
            status.intersects(
                git2::Status::INDEX_NEW
                    | git2::Status::INDEX_MODIFIED
                    | git2::Status::INDEX_DELETED
                    | git2::Status::INDEX_RENAMED
                    | git2::Status::INDEX_TYPECHANGE
                    | git2::Status::WT_NEW
                    | git2::Status::WT_MODIFIED
                    | git2::Status::WT_DELETED
                    | git2::Status::WT_RENAMED
                    | git2::Status::WT_TYPECHANGE
                    | git2::Status::CONFLICTED,
            )
        });

        if has_conflicts {
            anyhow::bail!(
                "Cannot checkout branch: you have uncommitted changes. Commit or stash them first."
            );
        }

        Ok(())
    }

    fn checkout_local_branch(&self, branch_name: &str) -> Result<()> {
        if self.current_local_branch_name()?.as_deref() == Some(branch_name) {
            anyhow::bail!("Already on branch '{}'", branch_name);
        }

        let branch = self
            .repo
            .find_branch(branch_name, BranchType::Local)
            .with_context(|| format!("Branch '{}' not found", branch_name))?;

        let commit = branch
            .get()
            .peel_to_commit()
            .with_context(|| format!("Failed to resolve branch '{}' to a commit", branch_name))?;

        self.checkout_commit_to_local_branch(branch_name, &commit)
    }

    fn checkout_remote_branch(&self, branch_name: &str, remote_name: Option<&str>) -> Result<()> {
        let remote_name = remote_name.unwrap_or(ORIGIN_REMOTE);
        let remote_ref_name = format!("{remote_name}/{branch_name}");

        if let Ok(local_branch) = self.repo.find_branch(branch_name, BranchType::Local) {
            let current = self.current_local_branch_name()?;
            let upstream_name_result = match local_branch.upstream() {
                Ok(upstream) => upstream
                    .name()
                    .map(|name| name.map(str::to_owned))
                    .with_context(|| format!("Failed to read upstream name for '{branch_name}'")),
                Err(error) if error.code() == git2::ErrorCode::NotFound => Ok(None),
                Err(error) => Err(error.into()),
            };

            match existing_local_branch_action(
                branch_name,
                &remote_ref_name,
                current.as_deref(),
                upstream_name_result,
            )? {
                ExistingLocalBranchAction::CheckoutLocal => {
                    return self.checkout_local_branch(branch_name);
                }
            }
        }

        let remote_branch = self
            .repo
            .find_branch(&remote_ref_name, BranchType::Remote)
            .with_context(|| format!("Remote branch '{}' not found", remote_ref_name))?;

        let commit = remote_branch
            .get()
            .peel_to_commit()
            .with_context(|| format!("Failed to resolve remote branch '{}'", remote_ref_name))?;

        let mut local_branch = self
            .repo
            .branch(branch_name, &commit, false)
            .with_context(|| format!("Failed to create local branch '{}'", branch_name))?;

        handle_upstream_setup_result(
            branch_name,
            local_branch
                .set_upstream(Some(&remote_ref_name))
                .with_context(|| {
                    format!(
                        "Failed to set upstream for '{}' to '{}'",
                        branch_name, remote_ref_name
                    )
                }),
            || {
                local_branch.delete().with_context(|| {
                    format!(
                        "Failed to clean up local branch '{}' after upstream setup failure",
                        branch_name
                    )
                })
            },
        )?;

        self.checkout_commit_to_local_branch(branch_name, &commit)
    }

    fn checkout_commit_to_local_branch(
        &self,
        branch_name: &str,
        commit: &git2::Commit,
    ) -> Result<()> {
        if let Some(path) = self.checked_out_worktree_path(branch_name)? {
            anyhow::bail!(
                "Branch '{}' is already used by worktree at '{}'",
                branch_name,
                path.display()
            );
        }

        let tree = commit
            .tree()
            .with_context(|| format!("Failed to get tree for branch '{}'", branch_name))?;

        self.repo
            .checkout_tree(tree.as_object(), None)
            .with_context(|| format!("Failed to checkout tree for branch '{}'", branch_name))?;

        self.repo
            .set_head(&format!("refs/heads/{}", branch_name))
            .with_context(|| format!("Failed to set HEAD to branch '{}'", branch_name))?;

        Ok(())
    }

    fn checked_out_worktree_path(&self, branch_name: &str) -> Result<Option<PathBuf>> {
        let command_dir = self.command_dir()?;
        let current_dir = command_dir.canonicalize().with_context(|| {
            format!(
                "Failed to resolve repository working directory '{}'",
                command_dir.display()
            )
        })?;

        let worktrees = self.repo.worktrees().context("Failed to list worktrees")?;
        for name in &worktrees {
            let Some(name) = name else {
                continue;
            };
            let Ok(worktree) = self.repo.find_worktree(name) else {
                continue;
            };
            if worktree.validate().is_err() {
                continue;
            }

            let worktree_path = worktree.path().to_path_buf();
            let Ok(canonical_worktree_path) = worktree_path.canonicalize() else {
                continue;
            };
            if canonical_worktree_path == current_dir {
                continue;
            }

            let Ok(worktree_repo) = Repository::open_from_worktree(&worktree) else {
                continue;
            };
            if current_local_branch_name(&worktree_repo)
                .ok()
                .flatten()
                .as_deref()
                == Some(branch_name)
            {
                return Ok(Some(worktree_path));
            }
        }

        Ok(None)
    }

    fn delete_local_branch(&self, branch_name: &str) -> Result<DeleteResult> {
        if self.current_local_branch_name()?.as_deref() == Some(branch_name) {
            anyhow::bail!("Cannot delete the current branch");
        }

        let mut branch = self
            .repo
            .find_branch(branch_name, BranchType::Local)
            .with_context(|| format!("Branch '{}' not found", branch_name))?;

        let commit_sha = branch
            .get()
            .resolve()
            .and_then(|r| r.peel_to_commit())
            .map(|c| c.id().to_string())
            .with_context(|| format!("Failed to get commit for branch '{}'", branch_name))?;

        branch
            .delete()
            .with_context(|| format!("Failed to delete branch '{}'", branch_name))?;

        Ok(DeleteResult::Local { commit_sha })
    }

    fn delete_remote_branch(
        &self,
        branch_name: &str,
        remote_name: Option<&str>,
    ) -> Result<DeleteResult> {
        let remote_name = remote_name.unwrap_or(ORIGIN_REMOTE);
        let remote_ref_name = format!("{remote_name}/{branch_name}");

        self.repo
            .find_branch(&remote_ref_name, BranchType::Remote)
            .with_context(|| format!("Remote branch '{}' not found", remote_ref_name))?;

        if self.current_branch_tracks_remote(&remote_ref_name)? {
            anyhow::bail!(
                "Cannot delete remote branch '{}': the current local branch tracks it.",
                remote_ref_name
            );
        }

        let output = Command::new("git")
            .args(["push", remote_name, "--delete", branch_name])
            .current_dir(self.command_dir()?)
            .output()
            .with_context(|| {
                format!("Failed to run git push {remote_name} --delete {branch_name}")
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let message = if !stderr.is_empty() { stderr } else { stdout };
            anyhow::bail!(
                "Failed to delete remote branch '{}': {}",
                remote_ref_name,
                message
            );
        }

        Ok(DeleteResult::Remote)
    }

    fn current_branch_tracks_remote(&self, remote_ref_name: &str) -> Result<bool> {
        let Some(current) = self.current_local_branch_name()? else {
            return Ok(false);
        };

        let branch = self
            .repo
            .find_branch(&current, BranchType::Local)
            .with_context(|| format!("Current branch '{}' not found", current))?;

        let Ok(upstream) = branch.upstream() else {
            return Ok(false);
        };

        Ok(upstream.name().ok().flatten() == Some(remote_ref_name))
    }
}

// --- Repository-level helpers ---------------------------------------------

fn last_commit_details(branch: &git2::Branch) -> (Option<String>, Option<i64>) {
    if let Ok(reference) = branch.get().resolve()
        && let Ok(commit) = reference.peel_to_commit()
    {
        let author = commit.author();
        let name = author.name().map(|s| s.to_string());
        let time = commit.time().seconds();
        return (name, Some(time));
    }

    (None, None)
}

pub(crate) fn current_local_branch_name(repo: &Repository) -> Result<Option<String>> {
    let head = repo.head().context("Failed to get HEAD reference")?;
    if !head.is_branch() {
        return Ok(None);
    }

    let branch_name = head
        .shorthand()
        .context("Failed to get branch name")?
        .to_string();
    Ok(Some(branch_name))
}

pub fn list_origin_remote_heads_in_dir(dir: &Path) -> Result<HashSet<String>> {
    let output = Command::new("git")
        .args(["ls-remote", "--heads", ORIGIN_REMOTE])
        .current_dir(dir)
        .output()
        .context("Failed to run git ls-remote --heads origin")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if !stderr.is_empty() { stderr } else { stdout };
        anyhow::bail!("Failed to check origin branches: {}", message);
    }

    Ok(parse_ls_remote_heads(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testing::{add_worktree_for_branch, init_test_repo, worktree_paths_match};
    use std::fs;

    #[test]
    fn test_current_local_branch_name_returns_none_in_detached_head() {
        let (repo, repo_path, oid) = init_test_repo("detached-head-local-name");
        repo.repo
            .set_head_detached(oid)
            .expect("detached head should be set");

        let branch_name = repo
            .current_local_branch_name()
            .expect("branch lookup should succeed");

        let _ = fs::remove_dir_all(repo_path);
        assert_eq!(branch_name, None);
    }

    #[test]
    fn test_current_branch_tracks_remote_returns_false_in_detached_head() {
        let (repo, repo_path, oid) = init_test_repo("detached-head");
        repo.repo
            .set_head_detached(oid)
            .expect("detached head should be set");

        let result = repo.current_branch_tracks_remote("origin/main");

        let _ = fs::remove_dir_all(repo_path);
        assert!(!result.expect("detached head should not error"));
    }

    #[test]
    fn test_checked_out_worktree_path_reports_linked_worktree() {
        let (repo, repo_path, oid) = init_test_repo("linked-worktree");
        let worktree_path = add_worktree_for_branch(&repo, &repo_path, oid, "feature/test");

        let checked_out_path = repo
            .checked_out_worktree_path("feature/test")
            .expect("worktree lookup should succeed");

        let _ = fs::remove_dir_all(&worktree_path);
        let _ = fs::remove_dir_all(repo_path);
        let checked_out_path = checked_out_path.expect("linked worktree path should be present");
        assert!(worktree_paths_match(&checked_out_path, &worktree_path));
    }

    #[test]
    fn test_checked_out_worktree_path_skips_invalid_worktree() {
        let (repo, repo_path, oid) = init_test_repo("invalid-worktree");
        let worktree_path = add_worktree_for_branch(&repo, &repo_path, oid, "feature/test");
        fs::remove_dir_all(&worktree_path).expect("worktree dir should be removed");

        let checked_out_path = repo
            .checked_out_worktree_path("feature/test")
            .expect("worktree lookup should succeed");

        let _ = fs::remove_dir_all(repo_path);
        assert_eq!(checked_out_path, None);
    }
}
