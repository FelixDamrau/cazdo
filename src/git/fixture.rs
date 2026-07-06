use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};

use super::repo::{BranchScope, BranchStatus, DeleteResult, GitBackend, RepoBranch};

/// In-memory `GitRepo` backend for tests: returns preset checkout/delete/prune
/// outcomes. Ops it isn't configured for (branch listing, status, freshness) are
/// unsupported and error.
#[derive(Default)]
pub struct FixtureGitRepo {
    checkout_result: Option<Result<(), String>>,
    delete_result: Option<Result<DeleteResult, String>>,
    prune_result: Option<Result<(), String>>,
}

impl FixtureGitRepo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_checkout_result(mut self, result: Result<(), String>) -> Self {
        self.checkout_result = Some(result);
        self
    }

    pub fn with_delete_result(mut self, result: Result<DeleteResult, String>) -> Self {
        self.delete_result = Some(result);
        self
    }

    pub fn with_prune_result(mut self, result: Result<(), String>) -> Self {
        self.prune_result = Some(result);
        self
    }
}

impl GitBackend for FixtureGitRepo {
    fn list_branches(&self) -> Result<Vec<RepoBranch>> {
        bail!("fixture git repo: list_branches unsupported")
    }

    fn get_branch_status(
        &self,
        _scope: BranchScope,
        _branch_name: &str,
        _remote_name: Option<&str>,
    ) -> Result<BranchStatus> {
        bail!("fixture git repo: get_branch_status unsupported")
    }

    fn checkout_branch(
        &self,
        _scope: BranchScope,
        _branch_name: &str,
        _remote_name: Option<&str>,
    ) -> Result<()> {
        preset("checkout_branch", &self.checkout_result)
    }

    fn delete_branch(
        &self,
        _scope: BranchScope,
        _branch_name: &str,
        _remote_name: Option<&str>,
    ) -> Result<DeleteResult> {
        match &self.delete_result {
            Some(Ok(result)) => Ok(result.clone()),
            Some(Err(message)) => Err(anyhow!(message.clone())),
            None => bail!("fixture git repo: no delete result configured"),
        }
    }

    fn prune_remote_tracking_branch(&self, _branch_name: &str) -> Result<()> {
        preset("prune_remote_tracking_branch", &self.prune_result)
    }

    fn repo_dir(&self) -> Result<PathBuf> {
        bail!("fixture git repo: repo_dir unsupported")
    }

    fn current_local_branch_name(&self) -> Result<Option<String>> {
        Ok(None)
    }
}

fn preset(op: &str, result: &Option<Result<(), String>>) -> Result<()> {
    match result {
        Some(Ok(())) => Ok(()),
        Some(Err(message)) => Err(anyhow!(message.clone())),
        None => bail!("fixture git repo: no {op} result configured"),
    }
}
