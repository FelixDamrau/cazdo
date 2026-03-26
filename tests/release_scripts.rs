use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::tempdir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn script_path(name: &str) -> PathBuf {
    repo_root().join("scripts").join(name)
}

fn write_file(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write file");
}

fn run_bash_script(script: &str, working_dir: &Path, args: &[&str]) -> std::process::Output {
    let mut command = Command::new("bash");
    command.arg(script_path(script));
    for arg in args {
        command.arg(arg);
    }
    command
        .current_dir(working_dir)
        .output()
        .expect("run bash script")
}

#[test]
fn prepare_release_updates_version_and_prepends_changelog_section() {
    let temp = tempdir().expect("tempdir");
    write_file(
        &temp.path().join("Cargo.toml"),
        r#"[package]
name = "cazdo"
version = "0.1.15"
edition = "2024"
"#,
    );
    write_file(
        &temp.path().join("CHANGELOG.md"),
        "# Changelog\n\n## Historical Releases\n\n- Older history\n",
    );
    write_file(
        &temp.path().join("cliff.toml"),
        "[changelog]\nbody = \"\"\n",
    );

    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).expect("create bin dir");
    let git_cliff = bin_dir.join("git-cliff");
    write_file(
        &git_cliff,
        r#"#!/usr/bin/env bash
set -euo pipefail

tag=""
prepend=""

while (($#)); do
  case "$1" in
    --tag)
      tag="$2"
      shift 2
      ;;
    --prepend)
      prepend="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done

tmp="$(mktemp)"
{
  printf '## %s - 2026-03-26\n\n' "$tag"
  printf '### Features\n\n'
  printf -- '- Added generated notes\n\n'
  cat "$prepend"
} > "$tmp"
mv "$tmp" "$prepend"
"#,
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = fs::metadata(&git_cliff).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&git_cliff, perms).expect("chmod");
    }

    let mut paths = vec![bin_dir];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(paths).expect("join path");

    let output = Command::new("bash")
        .arg(script_path("prepare-release.sh"))
        .arg("0.1.16")
        .current_dir(temp.path())
        .env("PATH", path)
        .output()
        .expect("run prepare-release script");

    assert!(
        output.status.success(),
        "script failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let cargo_toml = fs::read_to_string(temp.path().join("Cargo.toml")).expect("read Cargo.toml");
    assert!(cargo_toml.contains("version = \"0.1.16\""));

    let changelog = fs::read_to_string(temp.path().join("CHANGELOG.md")).expect("read changelog");
    assert!(changelog.starts_with("## v0.1.16 - 2026-03-26"));
    assert!(changelog.contains("### Features\n\n- Added generated notes"));
    assert!(changelog.contains("## Historical Releases"));
}

#[test]
fn extract_release_notes_returns_only_requested_section() {
    let temp = tempdir().expect("tempdir");
    write_file(
        &temp.path().join("CHANGELOG.md"),
        "# Changelog\n\n## v0.1.16 - 2026-03-26\n\n### Features\n\n- Add thing\n\n## v0.1.15 - 2026-03-20\n\n### Fixes\n\n- Fix thing\n",
    );

    let output = run_bash_script("extract-release-notes.sh", temp.path(), &["v0.1.16"]);

    assert!(
        output.status.success(),
        "script failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let notes = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(notes.contains("## v0.1.16 - 2026-03-26"));
    assert!(notes.contains("- Add thing"));
    assert!(!notes.contains("v0.1.15"));
}

#[test]
fn check_release_pr_requires_matching_changelog_when_version_changes() {
    let temp = tempdir().expect("tempdir");

    let init = Command::new("git")
        .args(["init"])
        .current_dir(temp.path())
        .output()
        .expect("git init");
    assert!(init.status.success());

    let config_name = Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(temp.path())
        .output()
        .expect("git config user.name");
    assert!(config_name.status.success());

    let config_email = Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(temp.path())
        .output()
        .expect("git config user.email");
    assert!(config_email.status.success());

    write_file(
        &temp.path().join("Cargo.toml"),
        r#"[package]
name = "cazdo"
version = "0.1.15"
edition = "2024"
"#,
    );
    write_file(
        &temp.path().join("CHANGELOG.md"),
        "# Changelog\n\n## v0.1.15 - 2026-03-20\n\n- Old notes\n",
    );

    let add = Command::new("git")
        .args(["add", "Cargo.toml", "CHANGELOG.md"])
        .current_dir(temp.path())
        .output()
        .expect("git add");
    assert!(add.status.success());

    let commit = Command::new("git")
        .args(["commit", "-m", "base"])
        .current_dir(temp.path())
        .output()
        .expect("git commit");
    assert!(commit.status.success());

    let base_sha = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(temp.path())
            .output()
            .expect("git rev-parse base")
            .stdout,
    )
    .expect("utf8 sha")
    .trim()
    .to_string();

    write_file(
        &temp.path().join("Cargo.toml"),
        r#"[package]
name = "cazdo"
version = "0.1.16"
edition = "2024"
"#,
    );

    let output = run_bash_script("check-release-pr.sh", temp.path(), &[&base_sha, "HEAD"]);

    assert!(
        !output.status.success(),
        "script should fail without matching changelog"
    );
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("CHANGELOG.md must contain a section for v0.1.16"));
}

#[test]
fn check_release_pr_skips_when_version_is_unchanged() {
    let temp = tempdir().expect("tempdir");

    let init = Command::new("git")
        .args(["init"])
        .current_dir(temp.path())
        .output()
        .expect("git init");
    assert!(init.status.success());

    let config_name = Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(temp.path())
        .output()
        .expect("git config user.name");
    assert!(config_name.status.success());

    let config_email = Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(temp.path())
        .output()
        .expect("git config user.email");
    assert!(config_email.status.success());

    write_file(
        &temp.path().join("Cargo.toml"),
        r#"[package]
name = "cazdo"
version = "0.1.16"
edition = "2024"
"#,
    );
    write_file(
        &temp.path().join("CHANGELOG.md"),
        "# Changelog\n\n## v0.1.16 - 2026-03-26\n\n- Notes\n",
    );

    let add = Command::new("git")
        .args(["add", "Cargo.toml", "CHANGELOG.md"])
        .current_dir(temp.path())
        .output()
        .expect("git add");
    assert!(add.status.success());

    let commit = Command::new("git")
        .args(["commit", "-m", "base"])
        .current_dir(temp.path())
        .output()
        .expect("git commit");
    assert!(commit.status.success());

    let base_sha = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(temp.path())
            .output()
            .expect("git rev-parse base")
            .stdout,
    )
    .expect("utf8 sha")
    .trim()
    .to_string();

    let output = run_bash_script("check-release-pr.sh", temp.path(), &[&base_sha, "HEAD"]);

    assert!(
        output.status.success(),
        "script failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Cargo.toml version unchanged"));
}
