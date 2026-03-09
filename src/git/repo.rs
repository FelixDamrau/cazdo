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

pub struct GitRepo {
    repo: Repository,
}

impl GitRepo {
    /// Open the git repository in the current directory
    pub fn open_current_dir() -> Result<Self> {
        let repo = Repository::discover(".")
            .context("Not a git repository (or any of the parent directories)")?;
        Ok(Self { repo })
    }

    /// Get the name of the current branch
    pub fn current_branch(&self) -> Result<String> {
        let head = self.repo.head().context("Failed to get HEAD reference")?;

        if head.is_branch() {
            let branch_name = head
                .shorthand()
                .context("Failed to get branch name")?
                .to_string();
            Ok(branch_name)
        } else {
            let commit = head.peel_to_commit().context("Failed to get HEAD commit")?;
            let short_id = commit.id().to_string();
            Ok(format!("(detached HEAD at {})", short_sha(&short_id)))
        }
    }

    /// Get all local branches plus origin remote branches.
    pub fn list_branches(&self) -> Result<Vec<RepoBranch>> {
        let current = self.current_branch().ok();
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

        branches.sort_by(|a, b| match (a.scope, b.scope) {
            (BranchScope::Local, BranchScope::Remote) => std::cmp::Ordering::Less,
            (BranchScope::Remote, BranchScope::Local) => std::cmp::Ordering::Greater,
            (BranchScope::Local, BranchScope::Local) => match (a.is_current, b.is_current) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.display_name.cmp(&b.display_name),
            },
            (BranchScope::Remote, BranchScope::Remote) => a.display_name.cmp(&b.display_name),
        });

        Ok(branches)
    }

    /// Get status information for a branch.
    pub fn get_branch_status(
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
    pub fn checkout_branch(
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

        match scope {
            BranchScope::Local => self.delete_local_branch(branch_name),
            BranchScope::Remote => self.delete_remote_branch(branch_name, remote_name),
        }
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

        Ok(BranchStatus {
            remote_status: RemoteStatus::LocalOnly,
            last_commit_author,
            last_commit_time,
        })
    }

    fn get_remote_status(&self, branch: &git2::Branch) -> RemoteStatus {
        let upstream = match branch.upstream() {
            Ok(upstream) => upstream,
            Err(e) => {
                if e.code() == git2::ErrorCode::NotFound {
                    if let Some(ref_name) = branch.get().name()
                        && self.repo.branch_upstream_name(ref_name).is_ok()
                    {
                        return RemoteStatus::Gone;
                    }
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
        let current = self.current_branch()?;
        if current == branch_name {
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
            if let Ok(upstream) = local_branch.upstream() {
                if upstream.name().ok().flatten() == Some(remote_ref_name.as_str()) {
                    return self.checkout_local_branch(branch_name);
                }
            }

            let current = self.current_branch()?;
            if current == branch_name {
                anyhow::bail!("Already on branch '{}'", branch_name);
            }

            anyhow::bail!(
                "Local branch '{}' already exists but is not tracking '{}'.",
                branch_name,
                remote_ref_name
            );
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

        local_branch
            .set_upstream(Some(&remote_ref_name))
            .with_context(|| {
                format!(
                    "Failed to set upstream for '{}' to '{}'",
                    branch_name, remote_ref_name
                )
            })?;

        self.checkout_commit_to_local_branch(branch_name, &commit)
    }

    fn checkout_commit_to_local_branch(
        &self,
        branch_name: &str,
        commit: &git2::Commit,
    ) -> Result<()> {
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

    fn delete_local_branch(&self, branch_name: &str) -> Result<DeleteResult> {
        let current = self.current_branch()?;
        if current == branch_name {
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
        let current = self.current_branch()?;
        let branch = self
            .repo
            .find_branch(&current, BranchType::Local)
            .with_context(|| format!("Current branch '{}' not found", current))?;

        let Ok(upstream) = branch.upstream() else {
            return Ok(false);
        };

        Ok(upstream.name().ok().flatten() == Some(remote_ref_name))
    }

    fn command_dir(&self) -> Result<&std::path::Path> {
        self.repo
            .workdir()
            .or_else(|| self.repo.path().parent())
            .context("Failed to determine repository working directory")
    }
}

fn last_commit_details(branch: &git2::Branch) -> (Option<String>, Option<i64>) {
    if let Ok(reference) = branch.get().resolve() {
        if let Ok(commit) = reference.peel_to_commit() {
            let author = commit.author();
            let name = author.name().map(|s| s.to_string());
            let time = commit.time().seconds();
            return (name, Some(time));
        }
    }

    (None, None)
}

fn origin_branch_name(name: &str) -> Option<&str> {
    let branch_name = name.strip_prefix("origin/")?;
    if branch_name == "HEAD" {
        return None;
    }
    Some(branch_name)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
