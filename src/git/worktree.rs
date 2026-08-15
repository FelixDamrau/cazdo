use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use git2::{Repository, StatusOptions, SubmoduleIgnore, SubmoduleStatus, WorktreeLockStatus};

use super::repo::short_sha;

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
/// Structural validity of a worktree entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeState {
    Valid,
    Missing,
    Prunable,
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
            Self::Prunable => "prunable",
            Self::Invalid(_) => "invalid",
            Self::Unknown(_) => "unknown",
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

    #[cfg(test)]
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

/// Compare paths for current-worktree marking while preserving the original
/// paths in the public inventory. Canonicalization handles `..`, symlinks, and
/// relative paths; Windows comparisons additionally ignore case for drive and
/// UNC paths.
pub fn worktree_paths_equal(left: &Path, right: &Path) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());

    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

/// Build an inventory using only libgit2 repository/worktree/status APIs and
/// filesystem metadata. No Git command is spawned.
pub(crate) fn inventory(repo: &Repository) -> Result<Vec<WorktreeInfo>> {
    let current_path = repo.workdir().map(Path::to_path_buf);
    let main_path = main_worktree_path(repo).context("Failed to determine main worktree path")?;

    let mut entries = Vec::new();
    entries.push(inspect_entry(
        repo,
        WorktreeIdentity::Main,
        main_path,
        current_path.as_deref(),
        true,
    ));

    let worktrees = repo
        .worktrees()
        .context("Failed to list linked worktrees")?;
    for name in worktrees.iter().flatten() {
        let identity = WorktreeIdentity::Linked {
            name: name.to_string(),
        };
        match repo.find_worktree(name) {
            Ok(worktree) => {
                let path = worktree.path().to_path_buf();
                let lock_reason = match worktree.is_locked() {
                    Ok(WorktreeLockStatus::Unlocked) => None,
                    Ok(WorktreeLockStatus::Locked(reason)) => {
                        Some(reason.unwrap_or_else(|| "locked".to_string()))
                    }
                    Err(error) => Some(format!("lock status unknown: {error}")),
                };
                let validated = worktree.validate();
                let prunable = worktree.is_prunable(None).unwrap_or(false);
                let state = if !path.exists() {
                    WorktreeState::Missing
                } else if let Err(error) = validated {
                    WorktreeState::Invalid(error.to_string())
                } else if prunable {
                    WorktreeState::Prunable
                } else {
                    WorktreeState::Valid
                };

                entries.push(inspect_entry_with_state(
                    repo,
                    identity,
                    path,
                    current_path.as_deref(),
                    false,
                    lock_reason,
                    state,
                    prunable,
                ));
            }
            Err(error) => entries.push(unknown_entry(
                identity,
                linked_worktree_path(repo, name).unwrap_or_default(),
                current_path.as_deref(),
                format!("unable to open linked worktree: {error}"),
            )),
        }
    }

    entries.sort_by(|left, right| {
        left.is_main
            .cmp(&right.is_main)
            .reverse()
            .then_with(|| left.name().cmp(right.name()))
    });
    Ok(entries)
}

fn inspect_entry(
    inventory_repo: &Repository,
    identity: WorktreeIdentity,
    path: PathBuf,
    current_path: Option<&Path>,
    is_main: bool,
) -> WorktreeInfo {
    let state = if path.exists() {
        WorktreeState::Valid
    } else {
        WorktreeState::Missing
    };
    inspect_entry_with_state(
        inventory_repo,
        identity,
        path,
        current_path,
        is_main,
        None,
        state,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn inspect_entry_with_state(
    inventory_repo: &Repository,
    identity: WorktreeIdentity,
    path: PathBuf,
    current_path: Option<&Path>,
    is_main: bool,
    lock_reason: Option<String>,
    mut state: WorktreeState,
    prunable: bool,
) -> WorktreeInfo {
    let same_as_inventory = worktree_paths_equal(&path, inventory_repo.workdir().unwrap_or(&path));
    let (branch, detached_short_sha, cleanliness, submodules) = if same_as_inventory {
        let (branch, detached_short_sha) = head_identity(inventory_repo);
        let cleanliness = cleanliness(inventory_repo);
        let submodules = submodules(inventory_repo);
        (branch, detached_short_sha, cleanliness, submodules)
    } else {
        match Repository::open(&path) {
            Ok(repository) => {
                let (branch, detached_short_sha) = head_identity(&repository);
                let cleanliness = cleanliness(&repository);
                let submodules = submodules(&repository);
                (branch, detached_short_sha, cleanliness, submodules)
            }
            Err(error) => {
                let error = error.to_string();
                if path.exists() && state.is_valid() {
                    state = WorktreeState::Invalid(error.clone());
                }
                let cleanliness = WorktreeCleanliness::Unknown(format!(
                    "unable to open worktree repository: {error}"
                ));
                let submodules = WorktreeSubmodules::Unknown(error);
                if let Some(name) = identity.linked_name() {
                    let (branch, detached_short_sha) = metadata_head_identity(inventory_repo, name);
                    (branch, detached_short_sha, cleanliness, submodules)
                } else {
                    (None, None, cleanliness, submodules)
                }
            }
        }
    };

    WorktreeInfo {
        identity,
        path: path.clone(),
        branch,
        detached_short_sha,
        is_main,
        is_current: current_path.is_some_and(|current| worktree_paths_equal(current, &path)),
        cleanliness,
        lock_reason,
        state,
        prunable,
        submodules,
    }
}

fn submodules(repo: &Repository) -> WorktreeSubmodules {
    match repo.submodules() {
        Ok(submodules) if submodules.is_empty() => WorktreeSubmodules::None,
        Ok(_) => WorktreeSubmodules::Present,
        Err(error) => WorktreeSubmodules::Unknown(error.to_string()),
    }
}

fn unknown_entry(
    identity: WorktreeIdentity,
    path: PathBuf,
    current_path: Option<&Path>,
    error: String,
) -> WorktreeInfo {
    WorktreeInfo {
        is_current: current_path.is_some_and(|current| worktree_paths_equal(current, &path)),
        is_main: false,
        identity,
        path,
        branch: None,
        detached_short_sha: None,
        cleanliness: WorktreeCleanliness::Unknown(error.clone()),
        lock_reason: None,
        submodules: WorktreeSubmodules::Unknown(error.clone()),
        state: WorktreeState::Unknown(error),
        prunable: false,
    }
}

fn metadata_head_identity(repo: &Repository, name: &str) -> (Option<String>, Option<String>) {
    let Some(common_git_dir) = common_git_dir(repo) else {
        return (None, None);
    };
    let head_path = common_git_dir.join("worktrees").join(name).join("HEAD");
    let Ok(head) = fs::read_to_string(head_path) else {
        return (None, None);
    };
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref: refs/heads/") {
        (Some(reference.to_string()), None)
    } else {
        (
            None,
            (!head.is_empty()).then(|| short_sha(head).to_string()),
        )
    }
}

fn linked_worktree_path(repo: &Repository, name: &str) -> Option<PathBuf> {
    let common_git_dir = common_git_dir(repo)?;
    let gitdir_path = common_git_dir.join("worktrees").join(name).join("gitdir");
    let gitdir = fs::read_to_string(gitdir_path).ok()?;
    let gitdir = PathBuf::from(gitdir.trim());
    let gitdir = if gitdir.is_absolute() {
        gitdir
    } else {
        common_git_dir.join(gitdir)
    };
    gitdir.parent().map(Path::to_path_buf)
}

fn head_identity(repo: &Repository) -> (Option<String>, Option<String>) {
    let Ok(head) = repo.head() else {
        return (None, None);
    };

    if head.is_branch() {
        (head.shorthand().map(str::to_string), None)
    } else {
        (
            None,
            head.target()
                .map(|oid| short_sha(&oid.to_string()).to_string()),
        )
    }
}

fn cleanliness(repo: &Repository) -> WorktreeCleanliness {
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .exclude_submodules(false);

    let statuses = match repo.statuses(Some(&mut options)) {
        Ok(statuses) => statuses,
        Err(error) => return WorktreeCleanliness::Unknown(error.to_string()),
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
        Err(error) => return WorktreeCleanliness::Unknown(error.to_string()),
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
            Err(error) => return WorktreeCleanliness::Unknown(error.to_string()),
        }
    }

    if reasons.is_empty() {
        WorktreeCleanliness::Clean
    } else {
        WorktreeCleanliness::Dirty(reasons)
    }
}

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

fn main_worktree_path(repo: &Repository) -> Result<PathBuf> {
    if !repo.is_worktree() {
        return repo
            .workdir()
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow!("bare repositories do not have a main worktree"));
    }

    let common = common_git_dir(repo).context("linked worktree has no common git directory")?;
    if let Ok(main_repo) = Repository::open(&common)
        && let Some(workdir) = main_repo.workdir()
    {
        return Ok(workdir.to_path_buf());
    }
    let common = fs::canonicalize(&common).with_context(|| {
        format!(
            "failed to resolve common git directory '{}'",
            common.display()
        )
    })?;
    common
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow!("common git directory has no parent"))
}

fn common_git_dir(repo: &Repository) -> Option<PathBuf> {
    if !repo.is_worktree() {
        return Some(repo.path().to_path_buf());
    }

    let linked_git_dir = repo.path();
    let common = fs::read_to_string(linked_git_dir.join("commondir")).ok()?;
    let common = PathBuf::from(common.trim());
    Some(if common.is_absolute() {
        common
    } else {
        linked_git_dir.join(common)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

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
    fn path_comparison_handles_relative_components() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("nested");
        fs::create_dir(&nested).unwrap();
        assert!(worktree_paths_equal(
            &nested,
            &dir.path().join("nested/..").join("nested")
        ));
    }

    #[cfg(windows)]
    #[test]
    fn path_comparison_handles_drive_and_unc_case_differences() {
        assert!(worktree_paths_equal(
            Path::new(r"C:\Users\Demo\Work Tree"),
            Path::new(r"c:\users\demo\work tree"),
        ));
        assert!(worktree_paths_equal(
            Path::new(r"\\Server\Share\Work Tree"),
            Path::new(r"\\server\share\work tree"),
        ));
    }

    #[test]
    fn submodule_enumeration_errors_remain_unknown() {
        let dir = tempdir().expect("temp directory");
        let repo = Repository::init(dir.path()).expect("repository should initialize");
        fs::write(dir.path().join(".gitmodules"), "[submodule\n")
            .expect("malformed submodule config should be written");

        assert!(matches!(submodules(&repo), WorktreeSubmodules::Unknown(_)));
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
