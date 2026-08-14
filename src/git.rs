#[cfg(test)]
mod fixture;
mod repo;
mod worktree;

#[cfg(test)]
pub use fixture::FixtureGitRepo;
pub use repo::{
    BranchOrder, BranchScope, BranchStatus, DeleteResult, GitRepo, RemoteStatus, RepoBranch,
    compare_branch_order, extract_work_item_number, list_origin_remote_heads_in_dir, short_sha,
};
#[cfg(test)]
pub use worktree::WorktreeDirtyReason;
pub use worktree::{
    WorktreeCleanliness, WorktreeIdentity, WorktreeInfo, WorktreeState, WorktreeSubmodules,
};
