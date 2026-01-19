use anyhow::{Context, Result};
use git2::{BranchType, Repository};

/// Branches that cannot be deleted (main/master)
pub const PROTECTED_BRANCHES: &[&str] = &["main", "master"];

/// Extract the first number from a branch name (work item number)
pub fn extract_work_item_number(branch_name: &str) -> Option<u32> {
    let start = branch_name.find(|c: char| c.is_ascii_digit())?;
    let num_str: String = branch_name[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    num_str.parse().ok()
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
            // Detached HEAD state
            let commit = head.peel_to_commit().context("Failed to get HEAD commit")?;
            let short_id = commit.id().to_string();
            Ok(format!("(detached HEAD at {})", &short_id[..7]))
        }
    }

    /// Get all local branches
    pub fn list_branches(&self) -> Result<Vec<String>> {
        let current = self.current_branch().ok();
        let mut branches: Vec<String> = Vec::new();

        let branch_iter = self
            .repo
            .branches(Some(BranchType::Local))
            .context("Failed to list branches")?;

        for branch_result in branch_iter {
            let (branch, _) = branch_result.context("Failed to read branch")?;
            if let Some(name) = branch.name().ok().flatten() {
                branches.push(name.to_string());
            }
        }

        // Sort branches, but put current branch first
        branches.sort_by(|a, b| {
            let a_current = current.as_ref().map(|c| c == a).unwrap_or(false);
            let b_current = current.as_ref().map(|c| c == b).unwrap_or(false);

            match (a_current, b_current) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.cmp(b),
            }
        });

        Ok(branches)
    }

    /// Get status information for a branch
    pub fn get_branch_status(&self, branch_name: &str) -> Result<BranchStatus> {
        let branch = self
            .repo
            .find_branch(branch_name, BranchType::Local)
            .with_context(|| format!("Branch '{}' not found", branch_name))?;

        // Get last commit info
        let (last_commit_author, last_commit_time) = if let Ok(reference) = branch.get().resolve() {
            if let Ok(commit) = reference.peel_to_commit() {
                let author = commit.author();
                let name = author.name().map(|s| s.to_string());
                let time = commit.time().seconds();
                (name, Some(time))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        // Get remote tracking status
        let remote_status = self.get_remote_status(&branch);

        Ok(BranchStatus {
            remote_status,
            last_commit_author,
            last_commit_time,
        })
    }

    /// Determine the remote tracking status for a branch
    fn get_remote_status(&self, branch: &git2::Branch) -> RemoteStatus {
        // Try to get upstream branch
        let upstream = match branch.upstream() {
            Ok(upstream) => upstream,
            Err(e) => {
                // Check if upstream was configured but is now gone
                if e.code() == git2::ErrorCode::NotFound {
                    // Try to check if there's upstream config for this branch
                    if let Some(ref_name) = branch.get().name()
                        && self.repo.branch_upstream_name(ref_name).is_ok()
                    {
                        return RemoteStatus::Gone;
                    }
                }
                return RemoteStatus::LocalOnly;
            }
        };

        // Get OIDs for comparison
        let local_oid = match branch.get().resolve().and_then(|r| r.peel_to_commit()) {
            Ok(commit) => commit.id(),
            Err(_) => return RemoteStatus::LocalOnly,
        };

        let remote_oid = match upstream.get().resolve().and_then(|r| r.peel_to_commit()) {
            Ok(commit) => commit.id(),
            Err(_) => return RemoteStatus::Gone,
        };

        // Calculate ahead/behind
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

    /// Delete a local branch and return the commit SHA it was pointing to
    /// Returns an error if trying to delete the current branch or main/master
    pub fn delete_branch(&self, branch_name: &str) -> Result<String> {
        if PROTECTED_BRANCHES.contains(&branch_name) {
            anyhow::bail!("Cannot delete protected branch '{}'", branch_name);
        }

        // Check if trying to delete the current branch
        let current = self.current_branch()?;
        if current == branch_name {
            anyhow::bail!("Cannot delete the current branch");
        }

        // Find the branch
        let mut branch = self
            .repo
            .find_branch(branch_name, BranchType::Local)
            .with_context(|| format!("Branch '{}' not found", branch_name))?;

        // Get the commit SHA before deletion
        let commit_sha = branch
            .get()
            .resolve()
            .and_then(|r| r.peel_to_commit())
            .map(|c| c.id().to_string())
            .with_context(|| format!("Failed to get commit for branch '{}'", branch_name))?;

        // Delete the branch
        branch
            .delete()
            .with_context(|| format!("Failed to delete branch '{}'", branch_name))?;

        Ok(commit_sha)
    }
}
