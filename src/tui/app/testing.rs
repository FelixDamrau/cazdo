//! Branch fixtures shared by the `App` tests.

use super::*;

pub(crate) fn branch(
    key: &str,
    display_name: &str,
    branch_name: &str,
    scope: BranchScope,
    is_current: bool,
    is_protected: bool,
    work_item_id: Option<u32>,
) -> BranchInfo {
    BranchInfo {
        key: key.to_string(),
        display_name: display_name.to_string(),
        branch_name: branch_name.to_string(),
        remote_name: (scope == BranchScope::Remote).then(|| "origin".to_string()),
        scope,
        work_item_id,
        is_current,
        is_protected,
        is_stale: false,
    }
}

pub(crate) fn create_test_branches() -> Vec<BranchInfo> {
    vec![
        branch(
            "refs/heads/main",
            "main",
            "main",
            BranchScope::Local,
            true,
            true,
            None,
        ),
        branch(
            "refs/heads/feature/123",
            "feature/123",
            "feature/123",
            BranchScope::Local,
            false,
            false,
            Some(123),
        ),
        branch(
            "refs/remotes/origin/feature/456",
            "origin/feature/456",
            "feature/456",
            BranchScope::Remote,
            false,
            false,
            Some(456),
        ),
    ]
}
