//! Path arithmetic for worktrees. Comparisons must survive relative
//! components, missing trailing separators, and Windows drive/UNC case
//! differences, so every path check in this crate funnels through here.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use git2::Repository;

/// Compare paths for current-worktree marking while preserving the original
/// paths in the public inventory. Canonicalization handles `..`, symlinks, and
/// relative paths; missing paths are component-normalized to remove trailing
/// separators. Windows comparisons additionally ignore case for drive and UNC
/// paths.
pub fn worktree_paths_equal(left: &Path, right: &Path) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| normalized(left));
    let right = fs::canonicalize(right).unwrap_or_else(|_| normalized(right));

    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

pub(super) fn normalized(path: &Path) -> PathBuf {
    path.components().collect()
}

pub(crate) fn main_worktree_path(repo: &Repository) -> Result<Option<PathBuf>> {
    if !repo.is_worktree() {
        return Ok(repo.workdir().map(normalized));
    }

    let common = common_git_dir(repo).context("linked worktree has no common git directory")?;
    if let Ok(main_repo) = Repository::open(&common) {
        if let Some(workdir) = main_repo.workdir() {
            return Ok(Some(normalized(workdir)));
        }
        return Ok(None);
    }
    let common = fs::canonicalize(&common).with_context(|| {
        format!(
            "failed to resolve common git directory '{}'",
            common.display()
        )
    })?;
    Ok(Some(common.parent().map(Path::to_path_buf).ok_or_else(
        || anyhow!("common git directory has no parent"),
    )?))
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

// --- Linked worktree admin paths ------------------------------------------

pub(super) fn linked_worktree_path(repo: &Repository, name: &str) -> Option<PathBuf> {
    let admin_path = linked_worktree_admin_path(repo, name)?;
    let gitdir = fs::read_to_string(admin_path.join("gitdir")).ok()?;
    let gitdir = PathBuf::from(gitdir.trim());
    let gitdir = if gitdir.is_absolute() {
        gitdir
    } else {
        admin_path.join(gitdir)
    };
    gitdir.parent().map(Path::to_path_buf)
}

pub(super) fn linked_worktree_admin_path(repo: &Repository, name: &str) -> Option<PathBuf> {
    common_git_dir(repo).map(|common_git_dir| common_git_dir.join("worktrees").join(name))
}

pub(super) fn linked_worktree_fallback_path(repo: &Repository, name: &str) -> PathBuf {
    linked_worktree_path(repo, name)
        .or_else(|| linked_worktree_admin_path(repo, name))
        .unwrap_or_else(|| PathBuf::from(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // --- path comparison -----------------------------------------------------

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

    #[test]
    fn path_comparison_normalizes_missing_trailing_separator() {
        let dir = tempdir().expect("temp directory");
        let missing = dir.path().join("missing");
        let missing_with_separator = PathBuf::from(format!(
            "{}{}",
            missing.display(),
            std::path::MAIN_SEPARATOR
        ));

        assert!(!missing.exists());
        assert!(worktree_paths_equal(&missing, &missing_with_separator));
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
    fn main_worktree_path_has_no_trailing_separator() {
        let dir = tempdir().expect("temp directory");
        let repo = Repository::init(dir.path()).expect("repository should initialize");

        let path = main_worktree_path(&repo)
            .expect("main worktree path lookup should succeed")
            .expect("non-bare repository should have a main worktree path");

        assert!(!path.to_string_lossy().ends_with(std::path::MAIN_SEPARATOR));
    }

    // --- linked worktree resolution ------------------------------------------

    #[test]
    fn linked_worktree_path_resolves_relative_gitdir_from_admin_directory() {
        let dir = tempdir().expect("temp directory");
        let repo = Repository::init(dir.path()).expect("repository should initialize");
        let linked_path = dir.path().join("linked");
        fs::create_dir_all(&linked_path).expect("linked worktree directory should be created");
        fs::write(linked_path.join(".git"), "gitdir: placeholder\n")
            .expect("linked worktree git file should be written");

        let name = "relative";
        let admin_path = repo.path().join("worktrees").join(name);
        fs::create_dir_all(&admin_path).expect("worktree metadata directory should be created");
        fs::write(admin_path.join("gitdir"), "../../../linked/.git\n")
            .expect("relative gitdir metadata should be written");

        let resolved = linked_worktree_path(&repo, name)
            .expect("relative gitdir metadata should resolve")
            .canonicalize()
            .expect("resolved worktree path should exist");
        assert_eq!(
            resolved,
            linked_path
                .canonicalize()
                .expect("linked worktree path should exist")
        );
    }

    #[cfg(windows)]
    #[test]
    fn linked_worktree_path_resolves_windows_relative_gitdir() {
        let dir = tempdir().expect("temp directory");
        let repo = Repository::init(dir.path()).expect("repository should initialize");
        let linked_path = dir.path().join("linked");
        fs::create_dir_all(&linked_path).expect("linked worktree directory should be created");
        fs::write(linked_path.join(".git"), "gitdir: placeholder\n")
            .expect("linked worktree git file should be written");

        let name = "windows-relative";
        let admin_path = repo.path().join("worktrees").join(name);
        fs::create_dir_all(&admin_path).expect("worktree metadata directory should be created");
        fs::write(admin_path.join("gitdir"), "..\\..\\..\\linked\\.git\n")
            .expect("Windows relative gitdir metadata should be written");

        let resolved = linked_worktree_path(&repo, name)
            .expect("Windows relative gitdir metadata should resolve")
            .canonicalize()
            .expect("resolved worktree path should exist");
        assert_eq!(
            resolved,
            linked_path
                .canonicalize()
                .expect("linked worktree path should exist")
        );
    }
}
