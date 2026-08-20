//! Reading how dirty a worktree is. Both entry points are deliberately
//! total: any libgit2 failure becomes an `Unknown` variant carrying the error
//! chain rather than an `Err`, because "we could not tell" is a state the UI
//! must render, not an error it should abort on.

use git2::{Repository, StatusOptions, SubmoduleIgnore, SubmoduleStatus};

use super::{WorktreeCleanliness, WorktreeDirtyReason, WorktreeSubmodules};
use crate::error::format_error_chain;

pub(crate) fn cleanliness(repo: &Repository) -> WorktreeCleanliness {
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .exclude_submodules(false);

    let statuses = match repo.statuses(Some(&mut options)) {
        Ok(statuses) => statuses,
        Err(error) => return WorktreeCleanliness::Unknown(format_error_chain(&error)),
    };

    let mut reasons = Vec::new();
    for entry in statuses.iter() {
        let status = entry.status();
        if status.is_conflicted() {
            push_reason(&mut reasons, WorktreeDirtyReason::Conflict);
        }
        if status.is_index_new()
            || status.is_index_modified()
            || status.is_index_deleted()
            || status.is_index_renamed()
            || status.is_index_typechange()
        {
            push_reason(&mut reasons, WorktreeDirtyReason::Index);
        }
        if status.is_wt_new() {
            push_reason(&mut reasons, WorktreeDirtyReason::Untracked);
        }
        if status.is_wt_modified()
            || status.is_wt_deleted()
            || status.is_wt_renamed()
            || status.is_wt_typechange()
        {
            push_reason(&mut reasons, WorktreeDirtyReason::Worktree);
        }
    }

    let submodules = match repo.submodules() {
        Ok(submodules) => submodules,
        Err(error) => return WorktreeCleanliness::Unknown(format_error_chain(&error)),
    };
    for submodule in submodules {
        let Some(name) = submodule.name() else {
            return WorktreeCleanliness::Unknown("submodule has no name".to_string());
        };
        match repo.submodule_status(name, SubmoduleIgnore::None) {
            Ok(status) if submodule_is_dirty(status) => {
                push_reason(&mut reasons, WorktreeDirtyReason::Submodule)
            }
            Ok(_) => {}
            Err(error) => return WorktreeCleanliness::Unknown(format_error_chain(&error)),
        }
    }

    if reasons.is_empty() {
        WorktreeCleanliness::Clean
    } else {
        WorktreeCleanliness::Dirty(reasons)
    }
}

pub(crate) fn submodules(repo: &Repository) -> WorktreeSubmodules {
    match repo.submodules() {
        Ok(submodules) if submodules.is_empty() => WorktreeSubmodules::None,
        Ok(_) => WorktreeSubmodules::Present,
        Err(error) => WorktreeSubmodules::Unknown(format_error_chain(&error)),
    }
}

// --- Internals ------------------------------------------------------------

fn submodule_is_dirty(status: SubmoduleStatus) -> bool {
    status.is_index_added()
        || status.is_index_deleted()
        || status.is_index_modified()
        || status.is_wd_uninitialized()
        || status.is_wd_added()
        || status.is_wd_deleted()
        || status.is_wd_modified()
        || status.is_wd_wd_modified()
        || status.is_wd_untracked()
}

fn push_reason(reasons: &mut Vec<WorktreeDirtyReason>, reason: WorktreeDirtyReason) {
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn submodule_enumeration_errors_remain_unknown() {
        let dir = tempdir().expect("temp directory");
        let repo = Repository::init(dir.path()).expect("repository should initialize");
        fs::write(dir.path().join(".gitmodules"), "[submodule\n")
            .expect("malformed submodule config should be written");

        assert!(matches!(submodules(&repo), WorktreeSubmodules::Unknown(_)));
    }
}
