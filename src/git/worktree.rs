//! Worktree vocabulary. This file holds only the value types the rest of the
//! app matches on; the operations that produce and consume them live in the
//! submodules, split by what they do to the repository:
//!
//! - [`inventory`] reads the worktree list.
//! - [`status`] reads cleanliness and submodule state.
//! - [`paths`] compares and resolves worktree paths.
//! - [`prune`] deletes stale administrative metadata.
//! - [`removal`] deletes a live worktree, directory and all.

mod inventory;
mod paths;
mod prune;
mod removal;
mod status;

use std::path::PathBuf;

pub(crate) use inventory::inventory;
#[cfg(test)]
pub(crate) use paths::worktree_paths_equal;
pub(crate) use prune::{prune_metadata, validate_worktree_prune};
pub(crate) use removal::{remove_linked_worktree, validate_worktree_removal};

// --- Identity -------------------------------------------------------------

/// Stable identity for a repository worktree.
///
/// The main worktree has no libgit2 worktree name, so it is represented by a
/// synthetic identity. Linked entries retain the exact name returned by
/// `Repository::worktrees` and can be targeted by that name later.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WorktreeIdentity {
    Main,
    Linked { name: String },
}

impl WorktreeIdentity {
    pub fn name(&self) -> &str {
        match self {
            Self::Main => "main",
            Self::Linked { name } => name,
        }
    }

    pub fn linked_name(&self) -> Option<&str> {
        match self {
            Self::Main => None,
            Self::Linked { name } => Some(name),
        }
    }
    #[cfg(test)]
    pub fn is_main(&self) -> bool {
        matches!(self, Self::Main)
    }
}

/// Inventory entry for one main or linked worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeInfo {
    pub identity: WorktreeIdentity,
    pub path: PathBuf,
    pub branch: Option<String>,
    pub detached_short_sha: Option<String>,
    pub is_main: bool,
    pub is_current: bool,
    pub cleanliness: WorktreeCleanliness,
    pub lock_reason: Option<String>,
    pub state: WorktreeState,
    /// True when libgit2 reports that cleanup may be possible. This remains
    /// separate from `state`: an invalid entry is not automatically prunable.
    pub prunable: bool,
    pub submodules: WorktreeSubmodules,
}

impl WorktreeInfo {
    pub fn name(&self) -> &str {
        self.identity.name()
    }

    pub fn linked_name(&self) -> Option<&str> {
        self.identity.linked_name()
    }

    pub fn is_locked(&self) -> bool {
        self.lock_reason.is_some()
    }

    pub fn ref_display(&self) -> String {
        self.branch.clone().unwrap_or_else(|| {
            self.detached_short_sha
                .clone()
                .map_or_else(|| "unknown".to_string(), |sha| format!("detached {sha}"))
        })
    }
}

// --- Liveness -------------------------------------------------------------

/// Structural validity of a worktree entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeState {
    Valid,
    Missing,
    Invalid(String),
    Unknown(String),
}

impl WorktreeState {
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid)
    }

    #[cfg(test)]
    pub fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Missing => "missing",
            Self::Invalid(_) => "invalid",
            Self::Unknown(_) => "unknown",
        }
    }
}

// --- Cleanliness ----------------------------------------------------------

/// Why a worktree is not clean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorktreeDirtyReason {
    Untracked,
    Index,
    Worktree,
    Conflict,
    Submodule,
}

impl WorktreeDirtyReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::Untracked => "untracked",
            Self::Index => "index",
            Self::Worktree => "worktree",
            Self::Conflict => "conflict",
            Self::Submodule => "submodule",
        }
    }
}

/// Result of checking a worktree's files without mutating it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeCleanliness {
    Clean,
    Dirty(Vec<WorktreeDirtyReason>),
    Unknown(String),
}

impl WorktreeCleanliness {
    pub fn is_clean(&self) -> bool {
        matches!(self, Self::Clean)
    }

    #[cfg(test)]
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown(_))
    }
    #[cfg(test)]
    pub fn dirty_reasons(&self) -> &[WorktreeDirtyReason] {
        match self {
            Self::Dirty(reasons) => reasons,
            _ => &[],
        }
    }
}

/// Result of inspecting whether a worktree contains submodules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeSubmodules {
    None,
    Present,
    Unknown(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_identity_preserves_linked_name_and_has_synthetic_main() {
        assert_eq!(WorktreeIdentity::Main.name(), "main");
        let identity = WorktreeIdentity::Linked {
            name: "feature with spaces/雪".to_string(),
        };
        assert_eq!(identity.name(), "feature with spaces/雪");
        assert_eq!(identity.linked_name(), Some("feature with spaces/雪"));
    }

    #[test]
    fn dirty_reason_accessors_are_safe_for_unknown_and_clean() {
        assert!(WorktreeCleanliness::Clean.is_clean());
        assert!(WorktreeCleanliness::Unknown("permission denied".into()).is_unknown());
        assert!(
            WorktreeCleanliness::Dirty(vec![WorktreeDirtyReason::Index])
                .dirty_reasons()
                .contains(&WorktreeDirtyReason::Index)
        );
    }
}
