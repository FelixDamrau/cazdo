//! Branch vocabulary: the value types the rest of the app speaks in, plus the
//! pure name and ordering logic that needs no repository access.

use std::collections::HashSet;

use anyhow::Result;

pub(crate) const ORIGIN_REMOTE: &str = "origin";

// --- Identity -------------------------------------------------------------

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

// --- Status ---------------------------------------------------------------

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

// --- Ordering -------------------------------------------------------------

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

// --- Name parsing ---------------------------------------------------------

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

pub(crate) fn origin_branch_name(name: &str) -> Option<&str> {
    let branch_name = name.strip_prefix(ORIGIN_REMOTE)?.strip_prefix('/')?;
    if branch_name == "HEAD" {
        return None;
    }
    Some(branch_name)
}

pub(crate) fn parse_ls_remote_heads(output: &str) -> HashSet<String> {
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

pub(crate) fn remote_branch_status(
    last_commit_author: Option<String>,
    last_commit_time: Option<i64>,
) -> BranchStatus {
    BranchStatus {
        remote_status: RemoteStatus::RemoteTracking,
        last_commit_author,
        last_commit_time,
    }
}

// --- Checkout decisions ---------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExistingLocalBranchAction {
    CheckoutLocal,
}

pub(crate) fn existing_local_branch_action(
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

pub(crate) fn handle_upstream_setup_result<F>(
    branch_name: &str,
    result: Result<()>,
    cleanup: F,
) -> Result<()>
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

    // --- name parsing --------------------------------------------------------

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
    fn test_remote_branch_status_uses_remote_tracking_variant() {
        let status = remote_branch_status(Some("Alice".to_string()), Some(123));

        assert!(matches!(status.remote_status, RemoteStatus::RemoteTracking));
        assert_eq!(status.last_commit_author.as_deref(), Some("Alice"));
        assert_eq!(status.last_commit_time, Some(123));
    }

    // --- checkout decisions --------------------------------------------------

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
}
