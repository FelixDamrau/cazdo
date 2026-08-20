//! Shared fixtures for the git module's tests: real temporary repositories,
//! built once here so each submodule's tests describe only what they assert.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use git2::Repository;

use super::repo::LiveGitRepo;
use super::worktree::{WorktreeInfo, worktree_paths_equal};

// --- Repositories ---------------------------------------------------------

pub(crate) fn init_test_repo(name: &str) -> (LiveGitRepo, PathBuf, git2::Oid) {
    let repo_path = std::env::temp_dir().join(format!(
        "cazdo-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));

    fs::create_dir_all(&repo_path).expect("temp repo dir should be created");
    let repo = Repository::init(&repo_path).expect("repo should initialize");

    fs::write(repo_path.join("README.md"), "hello\n").expect("file should be written");

    let mut index = repo.index().expect("repo index should load");
    index
        .add_path(Path::new("README.md"))
        .expect("file should be staged");
    let tree_id = index.write_tree().expect("tree should write");
    let tree = repo.find_tree(tree_id).expect("tree should load");
    let signature =
        git2::Signature::now("Test User", "test@example.com").expect("signature should create");
    let oid = repo
        .commit(Some("HEAD"), &signature, &signature, "init", &tree, &[])
        .expect("commit should succeed");
    drop(tree);

    (LiveGitRepo::from_repo(repo), repo_path, oid)
}

pub(crate) fn init_bare_test_repo(name: &str) -> (LiveGitRepo, PathBuf, PathBuf) {
    let repo_path = std::env::temp_dir().join(format!(
        "cazdo-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    let linked_path = repo_path.with_extension("wt");
    fs::create_dir_all(&repo_path).expect("temp bare repo dir should be created");
    let repo = Repository::init_bare(&repo_path).expect("bare repo should initialize");

    let blob = repo.blob(b"hello\n").expect("blob should be written");
    let mut treebuilder = repo.treebuilder(None).expect("treebuilder should open");
    treebuilder
        .insert("README.md", blob, 0o100644)
        .expect("tree entry should be written");
    let tree_id = treebuilder.write().expect("tree should write");
    drop(treebuilder);
    let tree = repo.find_tree(tree_id).expect("tree should load");
    let signature =
        git2::Signature::now("Test User", "test@example.com").expect("signature should create");
    let oid = repo
        .commit(
            Some("refs/heads/main"),
            &signature,
            &signature,
            "init",
            &tree,
            &[],
        )
        .expect("commit should succeed");
    drop(tree);
    let commit = repo.find_commit(oid).expect("commit should be found");
    let branch = repo
        .branch("feature/bare", &commit, false)
        .expect("branch should be created");
    let reference = branch.into_reference();
    let mut options = git2::WorktreeAddOptions::new();
    options.reference(Some(&reference));
    repo.worktree("linked-worktree", &linked_path, Some(&options))
        .expect("linked worktree should be added");
    drop(reference);
    drop(commit);

    (LiveGitRepo::from_repo(repo), repo_path, linked_path)
}

pub(crate) fn init_test_repo_with_external_git_dir(
    name: &str,
) -> (LiveGitRepo, PathBuf, PathBuf, git2::Oid) {
    let repo_root = std::env::temp_dir().join(format!(
        "cazdo-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    let repo_path = repo_root.join("worktree");
    let git_dir = repo_root.join("git-dir");

    fs::create_dir_all(&repo_path).expect("temp repo dir should be created");
    let mut options = git2::RepositoryInitOptions::new();
    options.workdir_path(&repo_path);
    let repo =
        Repository::init_opts(&git_dir, &options).expect("separate git dir should initialize");

    fs::write(repo_path.join("README.md"), "hello\n").expect("file should be written");

    let mut index = repo.index().expect("repo index should load");
    index
        .add_path(Path::new("README.md"))
        .expect("file should be staged");
    let tree_id = index.write_tree().expect("tree should write");
    let tree = repo.find_tree(tree_id).expect("tree should load");
    let signature =
        git2::Signature::now("Test User", "test@example.com").expect("signature should create");
    let oid = repo
        .commit(Some("HEAD"), &signature, &signature, "init", &tree, &[])
        .expect("commit should succeed");
    drop(tree);

    (LiveGitRepo::from_repo(repo), repo_root, repo_path, oid)
}

// --- Worktrees ------------------------------------------------------------

pub(crate) fn add_worktree_at(
    repo: &LiveGitRepo,
    oid: git2::Oid,
    branch_name: &str,
    name: &str,
    path: &Path,
) -> PathBuf {
    let commit = repo
        .repo()
        .find_commit(oid)
        .expect("commit should be found");
    let branch = repo
        .repo()
        .branch(branch_name, &commit, false)
        .expect("branch should be created");
    let reference = branch.into_reference();
    let mut options = git2::WorktreeAddOptions::new();
    options.reference(Some(&reference));
    repo.repo()
        .worktree(name, path, Some(&options))
        .expect("worktree should be added");
    path.to_path_buf()
}

pub(crate) fn add_worktree_for_branch(
    repo: &LiveGitRepo,
    repo_path: &Path,
    oid: git2::Oid,
    branch_name: &str,
) -> PathBuf {
    let commit = repo
        .repo()
        .find_commit(oid)
        .expect("commit should be found");
    let branch = repo
        .repo()
        .branch(branch_name, &commit, false)
        .expect("branch should be created");
    let reference = branch.into_reference();
    let mut options = git2::WorktreeAddOptions::new();
    options.reference(Some(&reference));

    let worktree_path = repo_path.with_extension("wt");
    repo.repo()
        .worktree("linked-worktree", &worktree_path, Some(&options))
        .expect("worktree should be added");

    worktree_path
}

pub(crate) fn linked_entry(repo: &LiveGitRepo, name: &str) -> WorktreeInfo {
    repo.list_worktrees()
        .expect("worktree inventory should succeed")
        .into_iter()
        .find(|entry| entry.linked_name() == Some(name))
        .expect("linked worktree should be present")
}

// --- Assertions -----------------------------------------------------------

pub(crate) fn worktree_paths_match(left: &Path, right: &Path) -> bool {
    if worktree_paths_equal(left, right) {
        return true;
    }

    let (Some(left_name), Some(left_parent), Some(right_name), Some(right_parent)) = (
        left.file_name(),
        left.parent(),
        right.file_name(),
        right.parent(),
    ) else {
        return false;
    };

    left_name == right_name && worktree_paths_equal(left_parent, right_parent)
}
