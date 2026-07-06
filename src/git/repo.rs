use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use git2::{BranchType, Repository};

use crate::pattern::is_protected;

const ORIGIN_REMOTE: &str = "origin";

/// Extract the first number from a branch name (work item number)
pub fn extract_work_item_number(branch_name: &str) -> Option<u32> {
    let start = branch_name.find(|c: char| c.is_ascii_digit())?;
    let num_str: String = branch_name[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    num_str.parse().ok()
}

/// Safely get the short SHA (first 7 characters)
pub fn short_sha(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BranchScope {
    Local,
    Remote,
}

impl BranchScope {
    pub fn is_remote(self) -> bool {
        matches!(self, Self::Remote)
    }
}

#[derive(Debug, Clone)]
pub struct RepoBranch {
    pub key: String,
    pub display_name: String,
    pub branch_name: String,
    pub remote_name: Option<String>,
    pub scope: BranchScope,
    pub is_current: bool,
}

/// Remote tracking status for a branch
#[derive(Debug, Clone)]
pub enum RemoteStatus {
    /// No upstream configured
    LocalOnly,
    /// Status used for remote-tracking branches themselves
    RemoteTracking,
    /// Synced with remote
    UpToDate,
    /// Local has commits not on remote
    Ahead(usize),
    /// Remote has commits not on local
    Behind(usize),
    /// Both local and remote have diverged
    Diverged { ahead: usize, behind: usize },
    /// Upstream configured but ref doesn't exist
    Gone,
}

/// Branch status information
#[derive(Debug, Clone)]
pub struct BranchStatus {
    pub remote_status: RemoteStatus,
    pub last_commit_author: Option<String>,
    pub last_commit_time: Option<i64>, // Unix timestamp
}

#[derive(Debug, Clone)]
pub enum DeleteResult {
    Local { commit_sha: String },
    Remote,
}

/// Branch fields needed to order branch lists: locals first, the current
/// branch first within locals, then by display name.
pub trait BranchOrder {
    fn scope(&self) -> BranchScope;
    fn is_current(&self) -> bool;
    fn display_name(&self) -> &str;
}

pub fn compare_branch_order<T: BranchOrder>(a: &T, b: &T) -> std::cmp::Ordering {
    fn key<T: BranchOrder>(branch: &T) -> (u8, bool, &str) {
        (
            branch.scope().is_remote() as u8,
            !branch.is_current(),
            branch.display_name(),
        )
    }
    key(a).cmp(&key(b))
}

impl BranchOrder for RepoBranch {
    fn scope(&self) -> BranchScope {
        self.scope
    }
    fn is_current(&self) -> bool {
        self.is_current
    }
    fn display_name(&self) -> &str {
        &self.display_name
    }
}

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
        protected_patterns: &[String],
    ) -> Result<DeleteResult>;
    fn prune_remote_tracking_branch(&self, branch_name: &str) -> Result<()>;
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
        self.backend.checkout_branch(scope, branch_name, remote_name)
    }

    pub fn delete_branch(
        &self,
        scope: BranchScope,
        branch_name: &str,
        remote_name: Option<&str>,
        protected_patterns: &[String],
    ) -> Result<DeleteResult> {
        self.backend
            .delete_branch(scope, branch_name, remote_name, protected_patterns)
    }

    pub fn prune_remote_tracking_branch(&self, branch_name: &str) -> Result<()> {
        self.backend.prune_remote_tracking_branch(branch_name)
    }

    pub fn repo_dir(&self) -> Result<PathBuf> {
        self.backend.repo_dir()
    }

    pub(crate) fn current_local_branch_name(&self) -> Result<Option<String>> {
        self.backend.current_local_branch_name()
    }
}

struct LiveGitRepo {
    repo: Repository,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingLocalBranchAction {
    CheckoutLocal,
}

impl LiveGitRepo {
    /// Open the git repository in the current directory
    pub fn open_current_dir() -> Result<Self> {
        let repo = Repository::discover(".")
            .context("Not a git repository (or any of the parent directories)")?;
        Ok(Self { repo })
    }
}

impl GitBackend for LiveGitRepo {
    /// Get all local branches plus origin remote branches.
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
        protected_patterns: &[String],
    ) -> Result<DeleteResult> {
        if is_protected(branch_name, protected_patterns) {
            anyhow::bail!("Cannot delete protected branch '{}'", branch_name);
        }

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

    fn repo_dir(&self) -> Result<PathBuf> {
        Ok(self.command_dir()?.to_path_buf())
    }

    fn current_local_branch_name(&self) -> Result<Option<String>> {
        current_local_branch_name(&self.repo)
    }
}

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

fn current_local_branch_name(repo: &Repository) -> Result<Option<String>> {
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

fn origin_branch_name(name: &str) -> Option<&str> {
    let branch_name = name.strip_prefix(ORIGIN_REMOTE)?.strip_prefix('/')?;
    if branch_name == "HEAD" {
        return None;
    }
    Some(branch_name)
}

fn parse_ls_remote_heads(output: &str) -> HashSet<String> {
    let mut branches = HashSet::new();

    for line in output.lines() {
        let mut parts = line.split_whitespace();
        let _sha = parts.next();
        let Some(ref_name) = parts.next() else {
            continue;
        };
        let Some(branch_name) = ref_name.strip_prefix("refs/heads/") else {
            continue;
        };
        branches.insert(branch_name.to_string());
    }

    branches
}

fn remote_branch_status(
    last_commit_author: Option<String>,
    last_commit_time: Option<i64>,
) -> BranchStatus {
    BranchStatus {
        remote_status: RemoteStatus::RemoteTracking,
        last_commit_author,
        last_commit_time,
    }
}

fn existing_local_branch_action(
    branch_name: &str,
    remote_ref_name: &str,
    current_branch: Option<&str>,
    upstream_name_result: Result<Option<String>>,
) -> Result<ExistingLocalBranchAction> {
    let upstream_name = upstream_name_result?;

    if upstream_name.as_deref() == Some(remote_ref_name) {
        return Ok(ExistingLocalBranchAction::CheckoutLocal);
    }

    if current_branch == Some(branch_name) {
        anyhow::bail!("Already on branch '{}'", branch_name);
    }

    anyhow::bail!(
        "Local branch '{}' already exists but is not tracking '{}' (currently tracks: {}).",
        branch_name,
        remote_ref_name,
        upstream_name.as_deref().unwrap_or("<none>")
    );
}

fn handle_upstream_setup_result<F>(branch_name: &str, result: Result<()>, cleanup: F) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    if let Err(error) = result {
        if let Err(cleanup_error) = cleanup() {
            anyhow::bail!(
                "Failed to set upstream for '{}': {}; additionally, failed to clean up orphaned local branch: {}",
                branch_name,
                error,
                cleanup_error
            );
        }

        anyhow::bail!("Failed to set upstream for '{}': {}", branch_name, error);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_extract_work_item_number() {
        assert_eq!(extract_work_item_number("feature/12345-login"), Some(12345));
        assert_eq!(extract_work_item_number("bugfix-42-fix-crash"), Some(42));
        assert_eq!(extract_work_item_number("12345-some-feature"), Some(12345));

        assert_eq!(extract_work_item_number("main"), None);
        assert_eq!(extract_work_item_number("develop"), None);
        assert_eq!(extract_work_item_number("no-numbers-here"), None);

        assert_eq!(extract_work_item_number(""), None);
        assert_eq!(extract_work_item_number("v2.1.0"), Some(2));
    }

    #[test]
    fn test_short_sha() {
        assert_eq!(short_sha("1234567890"), "1234567");
        assert_eq!(short_sha("12345"), "12345");
        assert_eq!(short_sha(""), "");
    }

    #[test]
    fn test_origin_branch_name_accepts_origin_branch() {
        assert_eq!(
            origin_branch_name("origin/feature/123"),
            Some("feature/123")
        );
    }

    #[test]
    fn test_origin_branch_name_rejects_symbolic_head() {
        assert_eq!(origin_branch_name("origin/HEAD"), None);
    }

    #[test]
    fn test_origin_branch_name_rejects_other_remotes() {
        assert_eq!(origin_branch_name("upstream/main"), None);
    }

    #[test]
    fn test_parse_ls_remote_heads_extracts_branch_names() {
        let output = "abc refs/heads/main\ndef refs/heads/feature/123\n";
        let branches = parse_ls_remote_heads(output);

        assert!(branches.contains("main"));
        assert!(branches.contains("feature/123"));
    }

    #[test]
    fn test_parse_ls_remote_heads_ignores_non_head_refs() {
        let output = "abc refs/tags/v1\ndef refs/remotes/origin/main\n";
        let branches = parse_ls_remote_heads(output);

        assert!(branches.is_empty());
    }

    #[test]
    fn test_preserve_upstream_error_when_cleanup_succeeds() {
        let result = handle_upstream_setup_result(
            "feature/test",
            Err(anyhow!("set upstream failed")),
            || Ok(()),
        );

        let error = result.expect_err("upstream setup should fail");
        assert_eq!(
            error.to_string(),
            "Failed to set upstream for 'feature/test': set upstream failed"
        );
    }

    #[test]
    fn test_preserve_upstream_error_when_cleanup_fails() {
        let result = handle_upstream_setup_result(
            "feature/test",
            Err(anyhow!("set upstream failed")),
            || Err(anyhow!("delete failed")),
        );

        let error = result.expect_err("upstream setup should fail");
        assert_eq!(
            error.to_string(),
            "Failed to set upstream for 'feature/test': set upstream failed; additionally, failed to clean up orphaned local branch: delete failed"
        );
    }

    #[test]
    fn test_remote_branch_status_uses_remote_tracking_variant() {
        let status = remote_branch_status(Some("Alice".to_string()), Some(123));

        assert!(matches!(status.remote_status, RemoteStatus::RemoteTracking));
        assert_eq!(status.last_commit_author.as_deref(), Some("Alice"));
        assert_eq!(status.last_commit_time, Some(123));
    }

    #[test]
    fn test_existing_local_branch_action_returns_checkout_when_tracking_target() {
        let action = existing_local_branch_action(
            "feature/test",
            "origin/feature/test",
            Some("main"),
            Ok(Some("origin/feature/test".to_string())),
        )
        .expect("matching upstream should reuse local branch");

        assert_eq!(action, ExistingLocalBranchAction::CheckoutLocal);
    }

    #[test]
    fn test_existing_local_branch_action_reports_current_upstream() {
        let error = existing_local_branch_action(
            "feature/test",
            "origin/feature/test",
            Some("main"),
            Ok(Some("origin/other".to_string())),
        )
        .expect_err("different upstream should error");

        assert_eq!(
            error.to_string(),
            "Local branch 'feature/test' already exists but is not tracking 'origin/feature/test' (currently tracks: origin/other)."
        );
    }

    #[test]
    fn test_existing_local_branch_action_reports_already_on_branch() {
        let error = existing_local_branch_action(
            "feature/test",
            "origin/feature/test",
            Some("feature/test"),
            Ok(None),
        )
        .expect_err("current branch should error");

        assert_eq!(error.to_string(), "Already on branch 'feature/test'");
    }

    #[test]
    fn test_existing_local_branch_action_propagates_upstream_name_error() {
        let error = existing_local_branch_action(
            "feature/test",
            "origin/feature/test",
            Some("main"),
            Err(git2::Error::from_str("upstream name failure").into()),
        )
        .expect_err("upstream name failure should propagate");

        assert_eq!(error.to_string(), "upstream name failure");
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
    fn test_checked_out_worktree_path_reports_linked_worktree() {
        let (repo, repo_path, oid) = init_test_repo("linked-worktree");
        let worktree_path = add_worktree_for_branch(&repo, &repo_path, oid, "feature/test");

        let checked_out_path = repo
            .checked_out_worktree_path("feature/test")
            .expect("worktree lookup should succeed");

        let _ = fs::remove_dir_all(&worktree_path);
        let _ = fs::remove_dir_all(repo_path);
        assert_eq!(checked_out_path, Some(worktree_path));
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

    fn init_test_repo(name: &str) -> (LiveGitRepo, PathBuf, git2::Oid) {
        let repo_path = std::env::temp_dir().join(format!(
            "cazdo-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after epoch")
                .as_nanos()
        ));

        fs::create_dir_all(&repo_path).expect("temp repo dir should be created");
        let repo = Repository::init(&repo_path).expect("repo should initialize");

        fs::write(repo_path.join("README.md"), "hello\n").expect("file should be written");

        let mut index = repo.index().expect("repo index should load");
        index
            .add_path(Path::new("README.md"))
            .expect("file should be staged");
        let tree_id = index.write_tree().expect("tree should write");
        let tree = repo.find_tree(tree_id).expect("tree should load");
        let signature =
            git2::Signature::now("Test User", "test@example.com").expect("signature should create");
        let oid = repo
            .commit(Some("HEAD"), &signature, &signature, "init", &tree, &[])
            .expect("commit should succeed");
        drop(tree);

        (LiveGitRepo { repo }, repo_path, oid)
    }

    fn add_worktree_for_branch(
        repo: &LiveGitRepo,
        repo_path: &Path,
        oid: git2::Oid,
        branch_name: &str,
    ) -> PathBuf {
        let commit = repo.repo.find_commit(oid).expect("commit should be found");
        let branch = repo
            .repo
            .branch(branch_name, &commit, false)
            .expect("branch should be created");
        let reference = branch.into_reference();
        let mut options = git2::WorktreeAddOptions::new();
        options.reference(Some(&reference));

        let worktree_path = repo_path.with_extension("wt");
        repo.repo
            .worktree("linked-worktree", &worktree_path, Some(&options))
            .expect("worktree should be added");

        worktree_path
    }
}
