//! Git access, layered so the pure logic can be tested without a repository.
//!
//! - [`branch`] holds the branch vocabulary and the name/ordering logic.
//! - [`repo`] is the [`GitRepo`] facade plus its libgit2 adapter.
//! - [`worktree`] holds the worktree vocabulary and its read/write operations.

mod branch;
mod repo;
mod worktree;

#[cfg(test)]
mod fixture;
#[cfg(test)]
mod testing;

pub use branch::{
    BranchOrder, BranchScope, BranchStatus, DeleteResult, RemoteStatus, RepoBranch,
    compare_branch_order, extract_work_item_number, short_sha,
};
pub use repo::{GitRepo, list_origin_remote_heads_in_dir};
pub use worktree::{WorktreeCleanliness, WorktreeInfo, WorktreeState, WorktreeSubmodules};

#[cfg(test)]
pub use fixture::FixtureGitRepo;
#[cfg(test)]
pub use worktree::{WorktreeDirtyReason, WorktreeIdentity};

pub(crate) use worktree::{validate_worktree_prune, validate_worktree_removal};
