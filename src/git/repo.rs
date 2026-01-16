use anyhow::{Context, Result};
use git2::{BranchType, Repository};
use regex::Regex;

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

    /// Extract the first number from a branch name (work item number)
    pub fn extract_work_item_number(&self, branch_name: &str) -> Option<u32> {
        let re = Regex::new(r"\d+").ok()?;
        let captures = re.find(branch_name)?;
        captures.as_str().parse().ok()
    }
}
