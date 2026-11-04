//! Worktree mutations own both snapshot checks and live revalidation. Callers
//! select an inventory entry; this module checks identity, protection, repository
//! contents and filesystem access before invoking libgit2's destructive prune.

use std::fs::{self, OpenOptions};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, anyhow, bail};
use git2::{Repository, WorktreeLockStatus, WorktreePruneOptions};

use super::{
    WorktreeCleanliness, WorktreeIdentity, WorktreeInfo, WorktreeState, WorktreeSubmodules,
    cleanliness, linked_worktree_admin_path, linked_worktree_path, main_worktree_path,
    read_head_identity, submodules, worktree_paths_equal,
};

pub(crate) fn validate_worktree_prune(entry: &WorktreeInfo) -> Result<&str, String> {
    if entry.is_main {
        return Err("Cannot prune the main worktree".to_string());
    }
    let Some(name) = entry.identity.linked_name() else {
        return Err(
            "Cannot prune worktree: selected entry is not a valid linked worktree".to_string(),
        );
    };
    if name.trim().is_empty() {
        return Err(
            "Cannot prune worktree: selected entry is not a valid linked worktree".to_string(),
        );
    }
    if entry.is_current {
        return Err("Cannot prune the current worktree".to_string());
    }
    if entry.is_locked() {
        return Err(format!(
            "Cannot prune locked worktree '{}': {}",
            entry.name(),
            entry.lock_reason.as_deref().unwrap_or("worktree is locked")
        ));
    }
    if !matches!(entry.state, WorktreeState::Missing) {
        return Err(format!(
            "Cannot prune worktree '{}': only entries marked missing and prunable are allowed",
            entry.name()
        ));
    }
    if !entry.prunable {
        return Err(format!(
            "Cannot prune worktree '{}': stale metadata is not marked prunable",
            entry.name()
        ));
    }
    Ok(name)
}

/// Validate the inventory snapshot before attempting removal.
///
/// The live worktree metadata and repository contents must still be rechecked
/// before pruning because this snapshot may be stale.
pub(crate) fn validate_worktree_removal(entry: &WorktreeInfo) -> Result<&str, String> {
    if entry.is_main {
        return Err(format!(
            "Cannot remove worktree '{}': the main worktree is protected",
            entry.path.display()
        ));
    }
    if entry.is_current {
        return Err(format!(
            "Cannot remove worktree '{}': the current worktree is protected",
            entry.path.display()
        ));
    }
    let name = entry
        .linked_name()
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| "Cannot remove malformed worktree entry: missing linked name".to_string())?;
    if entry.path.as_os_str().is_empty() {
        return Err(format!(
            "Cannot remove malformed worktree '{}': path is empty",
            name
        ));
    }
    if !entry.state.is_valid() {
        return Err(format!(
            "Cannot remove worktree '{}': inventory state is '{}'; refresh worktree inventory first",
            entry.path.display(),
            entry.state.label()
        ));
    }
    validate_cleanliness(&entry.cleanliness, &entry.path)?;
    validate_submodules(&entry.submodules, &entry.path)?;
    if let Some(reason) = &entry.lock_reason {
        return Err(format!(
            "Cannot remove worktree '{}': it is locked ({reason})",
            entry.path.display()
        ));
    }
    if entry.prunable {
        return Err(format!(
            "Cannot remove worktree '{}': it is prunable or missing; refresh worktree inventory first",
            entry.path.display()
        ));
    }
    Ok(name)
}

/// Prune only the administrative directory for a missing, safely identifiable
/// linked worktree. No force flags are used, and the working-tree path is
/// never touched.
pub(crate) fn prune_metadata(
    repo: &Repository,
    identity: &WorktreeIdentity,
    expected_path: &Path,
) -> Result<()> {
    let Some(name) = identity.linked_name() else {
        bail!("Cannot prune the main worktree; only linked worktrees are supported");
    };
    let worktree = repo
        .find_worktree(name)
        .with_context(|| format!("Cannot open linked worktree metadata '{name}'"))?;
    let path = worktree.path().to_path_buf();
    if !worktree_paths_equal(&path, expected_path) {
        bail!(
            "Cannot prune worktree '{}': selected path '{}' no longer matches metadata path '{}'",
            name,
            expected_path.display(),
            path.display()
        );
    }
    let main_path =
        main_worktree_path(repo).context("Cannot determine main worktree path before pruning")?;
    if worktree_paths_equal(&path, &main_path) {
        bail!("Cannot prune the main worktree");
    }
    if let Some(current_path) = repo.workdir()
        && worktree_paths_equal(&path, current_path)
    {
        bail!("Cannot prune the current worktree");
    }
    match worktree
        .is_locked()
        .with_context(|| format!("Cannot determine lock status for worktree '{name}'"))?
    {
        WorktreeLockStatus::Unlocked => {}
        WorktreeLockStatus::Locked(reason) => {
            bail!(
                "Cannot prune locked worktree '{}': {}",
                name,
                reason.unwrap_or_else(|| "worktree is locked".to_string())
            );
        }
    }

    if path.exists() {
        bail!(
            "Cannot prune worktree '{}': path '{}' still exists; only missing worktrees are supported",
            name,
            path.display()
        );
    }

    let admin_path = linked_worktree_admin_path(repo, name).ok_or_else(|| {
        anyhow!("Cannot determine administrative metadata path for worktree '{name}'")
    })?;
    if !admin_path.is_dir() {
        bail!(
            "Cannot prune worktree '{}': administrative metadata '{}' is malformed",
            name,
            admin_path.display()
        );
    }

    let gitdir = fs::read_to_string(admin_path.join("gitdir")).with_context(|| {
        format!(
            "Cannot prune worktree '{}': administrative metadata is missing gitdir",
            name
        )
    })?;
    if gitdir.trim().is_empty() || !gitdir.trim_end().ends_with(".git") {
        bail!(
            "Cannot prune worktree '{}': administrative gitdir metadata is malformed",
            name
        );
    }
    let linked_path = linked_worktree_path(repo, name).ok_or_else(|| {
        anyhow!(
            "Cannot prune worktree '{}': administrative gitdir metadata is malformed",
            name
        )
    })?;
    if !worktree_paths_equal(&linked_path, &path) {
        bail!(
            "Cannot prune worktree '{}': administrative path does not match recorded worktree path '{}'",
            name,
            path.display()
        );
    }

    let head = fs::read_to_string(admin_path.join("HEAD")).with_context(|| {
        format!(
            "Cannot prune worktree '{}': administrative metadata is missing HEAD",
            name
        )
    })?;
    if !valid_head_metadata(&head) {
        bail!(
            "Cannot prune worktree '{}': administrative HEAD metadata is malformed",
            name
        );
    }

    let prunable = worktree
        .is_prunable(None)
        .with_context(|| format!("Cannot determine whether worktree '{name}' is prunable"))?;
    if !prunable {
        bail!(
            "Cannot prune worktree '{}': metadata is not marked prunable",
            name
        );
    }

    worktree
        .prune(None)
        .with_context(|| format!("Failed to prune stale metadata for worktree '{name}'"))
}

fn valid_head_metadata(head: &str) -> bool {
    let head = head.trim();
    if let Some(branch) = head.strip_prefix("ref: refs/heads/") {
        !branch.is_empty() && !branch.contains(char::is_whitespace)
    } else {
        git2::Oid::from_str(head).is_ok()
    }
}

static REMOVAL_PROBE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn preflight_worktree_removal(path: &Path) -> Result<()> {
    let metadata = fs::metadata(path).with_context(|| {
        format!(
            "Cannot remove worktree '{}': unable to access its directory before removal",
            path.display()
        )
    })?;
    if !metadata.is_dir() {
        bail!(
            "Cannot remove worktree '{}': its path is not a directory",
            path.display()
        );
    }

    #[cfg(windows)]
    ensure_worktree_tree_writable(path)?;

    let probe_id = REMOVAL_PROBE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut probe_path = None;
    for attempt in 0..16 {
        let candidate = path.join(format!(
            ".cazdo-removal-probe-{}-{probe_id}-{attempt}",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                drop(file);
                probe_path = Some(candidate);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "Cannot remove worktree '{}': its directory is not writable",
                        path.display()
                    )
                });
            }
        }
    }

    let Some(probe_path) = probe_path else {
        bail!(
            "Cannot remove worktree '{}': unable to create a unique writability probe",
            path.display()
        );
    };
    fs::remove_file(&probe_path).with_context(|| {
        format!(
            "Cannot remove worktree '{}': unable to clean up its writability probe",
            path.display()
        )
    })?;
    Ok(())
}

#[cfg(windows)]
fn ensure_worktree_tree_writable(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.permissions().readonly() {
        bail!(
            "Cannot remove worktree '{}': its path or contents are read-only",
            path.display()
        );
    }
    if metadata.file_type().is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            ensure_worktree_tree_writable(&entry.path())?;
        }
    }
    Ok(())
}

fn worktree_metadata_missing(repo: &Repository, name: &str) -> bool {
    if repo.find_worktree(name).is_err() {
        return true;
    }

    let Some(admin_path) = linked_worktree_admin_path(repo, name) else {
        return false;
    };
    matches!(
        fs::metadata(admin_path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    )
}

/// Remove a clean linked worktree, preserving its branch. A snapshot never
/// authorizes deletion by itself: all safety checks run against live state.
pub(crate) fn remove(repo: &Repository, selected: &WorktreeInfo) -> Result<()> {
    let name = validate_worktree_removal(selected).map_err(anyhow::Error::msg)?;
    let worktree = find_registered_worktree(repo, selected, name)?;
    validate_worktree_metadata(repo, &worktree)?;
    validate_linked_worktree(&worktree, selected)?;

    let actual_path = worktree.path();
    preflight_worktree_removal(actual_path)?;

    // GIT_WORKTREE_PRUNE_VALID allows pruning a still-valid worktree; this is intentional.
    let mut options = WorktreePruneOptions::new();
    options.valid(true).working_tree(true);
    if let Err(error) = worktree.prune(Some(&mut options)) {
        if worktree_metadata_missing(repo, name) {
            anyhow::bail!(
                "Worktree '{}' was deregistered but its directory could not be fully deleted ({}). Remove it manually.",
                actual_path.display(),
                error
            );
        }
        return Err(error)
            .with_context(|| format!("Failed to remove worktree '{}'", actual_path.display()));
    }
    Ok(())
}

fn find_registered_worktree(
    repo: &Repository,
    selected: &WorktreeInfo,
    name: &str,
) -> Result<git2::Worktree> {
    let worktree_names = repo
        .worktrees()
        .context("Cannot remove worktree: failed to list linked worktrees")?;
    if !worktree_names
        .iter()
        .flatten()
        .any(|candidate| candidate == name)
    {
        anyhow::bail!(
            "Cannot remove worktree '{}': linked worktree is no longer registered; refresh worktree inventory",
            selected.path.display()
        );
    }

    let worktree = repo.find_worktree(name).with_context(|| {
        format!(
            "Cannot remove worktree '{}': linked entry is unavailable",
            name
        )
    })?;
    let actual_path = worktree.path();
    if !worktree_paths_equal(&selected.path, actual_path) {
        anyhow::bail!(
            "Cannot remove worktree '{}': inventory path is stale (current path is '{}'); refresh worktree inventory",
            selected.path.display(),
            actual_path.display()
        );
    }
    Ok(worktree)
}

fn validate_worktree_metadata(repo: &Repository, worktree: &git2::Worktree) -> Result<()> {
    let actual_path = worktree.path();
    let main_path = main_worktree_path(repo)
        .context("Cannot remove worktree: failed to determine the main worktree path")?;
    if worktree_paths_equal(actual_path, &main_path) {
        anyhow::bail!(
            "Cannot remove worktree '{}': the main worktree is protected",
            actual_path.display()
        );
    }

    let current_path = repo
        .workdir()
        .context("Cannot remove worktree: current worktree path is unavailable")?;
    if worktree_paths_equal(actual_path, current_path) {
        anyhow::bail!(
            "Cannot remove worktree '{}': the current worktree is protected",
            actual_path.display()
        );
    }

    worktree.validate().with_context(|| {
        format!(
            "Cannot remove worktree '{}': it is invalid",
            actual_path.display()
        )
    })?;
    if worktree.is_prunable(None).with_context(|| {
        format!(
            "Cannot remove worktree '{}': unable to determine whether it is prunable",
            actual_path.display()
        )
    })? {
        anyhow::bail!(
            "Cannot remove worktree '{}': it is prunable or missing",
            actual_path.display()
        );
    }

    match worktree.is_locked().with_context(|| {
        format!(
            "Cannot remove worktree '{}': unable to determine lock status",
            actual_path.display()
        )
    })? {
        WorktreeLockStatus::Unlocked => {}
        WorktreeLockStatus::Locked(reason) => {
            anyhow::bail!(
                "Cannot remove worktree '{}': it is locked ({})",
                actual_path.display(),
                reason.as_deref().unwrap_or("locked")
            );
        }
    }

    Ok(())
}

fn validate_linked_worktree(worktree: &git2::Worktree, selected: &WorktreeInfo) -> Result<()> {
    let actual_path = worktree.path();
    let linked_repo = Repository::open_from_worktree(worktree).with_context(|| {
        format!(
            "Cannot remove worktree '{}': unable to open its repository",
            actual_path.display()
        )
    })?;
    validate_worktree_head(&linked_repo, selected, actual_path)?;
    validate_cleanliness(&cleanliness(&linked_repo), actual_path).map_err(anyhow::Error::msg)?;
    validate_submodules(&submodules(&linked_repo), actual_path).map_err(anyhow::Error::msg)?;
    Ok(())
}

fn validate_worktree_head(
    linked_repo: &Repository,
    selected: &WorktreeInfo,
    actual_path: &Path,
) -> Result<()> {
    let (branch, detached_short_sha) = read_head_identity(linked_repo).with_context(|| {
        format!(
            "Cannot remove worktree '{}': unable to read its HEAD",
            actual_path.display()
        )
    })?;
    if branch.as_deref() != selected.branch.as_deref()
        || detached_short_sha.as_deref() != selected.detached_short_sha.as_deref()
    {
        anyhow::bail!(
            "Cannot remove worktree '{}': branch or HEAD changed; refresh worktree inventory",
            actual_path.display()
        );
    }
    Ok(())
}

fn validate_cleanliness(status: &WorktreeCleanliness, path: &Path) -> Result<(), String> {
    match status {
        WorktreeCleanliness::Clean => Ok(()),
        WorktreeCleanliness::Dirty(reasons) => {
            let reasons = reasons
                .iter()
                .map(|reason| reason.label())
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "Cannot remove worktree '{}': it has uncommitted changes ({reasons})",
                path.display()
            ))
        }
        WorktreeCleanliness::Unknown(error) => Err(format!(
            "Cannot remove worktree '{}': status is unknown ({error})",
            path.display()
        )),
    }
}

fn validate_submodules(status: &WorktreeSubmodules, path: &Path) -> Result<(), String> {
    match status {
        WorktreeSubmodules::None => Ok(()),
        WorktreeSubmodules::Present => Err(format!(
            "Cannot remove worktree '{}': it contains submodules",
            path.display()
        )),
        WorktreeSubmodules::Unknown(error) => Err(format!(
            "Cannot remove worktree '{}': submodule status is unknown ({error})",
            path.display()
        )),
    }
}
