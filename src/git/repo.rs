//! The public git surface. [`GitRepo`] is a thin facade over a [`GitBackend`]:
//! the live libgit2 adapter in production, an in-memory fixture in tests.
//! Splitting the facade from the adapter is what lets the TUI be tested without
//! touching a real repository.

pub(crate) mod live;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use git2::Repository;

pub(crate) use self::live::LiveGitRepo;
pub use self::live::list_origin_remote_heads_in_dir;
use super::branch::{BranchScope, BranchStatus, DeleteResult, RepoBranch};
use super::worktree::{
    WorktreeIdentity, WorktreeInfo, inventory, remove_linked_worktree, validate_worktree_prune,
};
use crate::pattern::is_protected;

/// Public git interface: a concrete facade over a backend that is the live
/// libgit2 adapter in production and an in-memory fixture in tests.
pub struct GitRepo {
    backend: Box<dyn GitBackend>,
}

pub(crate) trait GitBackend {
    fn list_branches(&self) -> Result<Vec<RepoBranch>>;
    fn get_branch_status(
        &self,
        scope: BranchScope,
        branch_name: &str,
        remote_name: Option<&str>,
    ) -> Result<BranchStatus>;
    fn checkout_branch(
        &self,
        scope: BranchScope,
        branch_name: &str,
        remote_name: Option<&str>,
    ) -> Result<()>;
    fn delete_branch(
        &self,
        scope: BranchScope,
        branch_name: &str,
        remote_name: Option<&str>,
    ) -> Result<DeleteResult>;
    fn prune_remote_tracking_branch(&self, branch_name: &str) -> Result<()>;
    fn prune_worktree_metadata_by_identity(
        &self,
        identity: &WorktreeIdentity,
        expected_path: &Path,
    ) -> Result<()>;
    fn repo_dir(&self) -> Result<PathBuf>;
    fn current_local_branch_name(&self) -> Result<Option<String>>;
}

impl GitRepo {
    pub fn open_current_dir() -> Result<Self> {
        Ok(Self {
            backend: Box::new(LiveGitRepo::open_current_dir()?),
        })
    }

    #[cfg(test)]
    pub fn fixture(fixture: super::fixture::FixtureGitRepo) -> Self {
        Self {
            backend: Box::new(fixture),
        }
    }

    pub fn list_branches(&self) -> Result<Vec<RepoBranch>> {
        self.backend.list_branches()
    }

    pub fn get_branch_status(
        &self,
        scope: BranchScope,
        branch_name: &str,
        remote_name: Option<&str>,
    ) -> Result<BranchStatus> {
        self.backend
            .get_branch_status(scope, branch_name, remote_name)
    }

    pub fn checkout_branch(
        &self,
        scope: BranchScope,
        branch_name: &str,
        remote_name: Option<&str>,
    ) -> Result<()> {
        self.backend
            .checkout_branch(scope, branch_name, remote_name)
    }

    pub fn delete_branch(
        &self,
        scope: BranchScope,
        branch_name: &str,
        remote_name: Option<&str>,
        protected_patterns: &[String],
    ) -> Result<DeleteResult> {
        if is_protected(branch_name, protected_patterns) {
            anyhow::bail!("Cannot delete protected branch '{}'", branch_name);
        }
        self.backend.delete_branch(scope, branch_name, remote_name)
    }

    pub fn prune_remote_tracking_branch(&self, branch_name: &str) -> Result<()> {
        self.backend.prune_remote_tracking_branch(branch_name)
    }

    /// Remove only stale administrative metadata for a missing linked worktree.
    ///
    /// The inventory entry is treated as untrusted UI state: the live backend
    /// reopens and revalidates the worktree before pruning anything.
    pub fn prune_worktree_metadata(&self, entry: &WorktreeInfo) -> Result<()> {
        let name = match validate_worktree_prune(entry) {
            Ok(name) => name,
            Err(error) => bail!("{error}"),
        };

        self.backend.prune_worktree_metadata_by_identity(
            &WorktreeIdentity::Linked {
                name: name.to_string(),
            },
            &entry.path,
        )
    }

    /// Reopen an owned repository path before revalidating and removing
    /// a linked worktree. The owned inputs let this operation run on a
    /// blocking worker without borrowing TUI state or a live `GitRepo`.
    pub(crate) fn remove_worktree_at(repo_path: PathBuf, worktree: WorktreeInfo) -> Result<()> {
        let repo = Repository::discover(&repo_path).with_context(|| {
            format!("Failed to discover repository at '{}'", repo_path.display())
        })?;
        remove_linked_worktree(&repo, &worktree)
    }

    pub fn repo_dir(&self) -> Result<PathBuf> {
        self.backend.repo_dir()
    }

    pub(crate) fn list_worktrees_at(path: &Path) -> Result<Vec<WorktreeInfo>> {
        let repo = Repository::discover(path)
            .with_context(|| format!("Failed to discover repository at '{}'", path.display()))?;
        inventory(&repo)
    }

    pub(crate) fn current_local_branch_name(&self) -> Result<Option<String>> {
        self.backend.current_local_branch_name()
    }
}
