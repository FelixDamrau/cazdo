#!/usr/bin/env bash

set -euo pipefail

workspace_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
demo_root="$(mktemp -d "${TMPDIR:-/tmp}/cazdo-vhs-demo-XXXXXX")"
origin_dir="$demo_root/origin.git"
repo_dir="$demo_root/repo"
fixture_path="$workspace_root/docs/tapes/demo-work-items.json"
binary_path="$workspace_root/target/debug/cazdo"
wrapper_path="$repo_dir/run-cazdo-demo.sh"

git init --bare --quiet "$origin_dir"
git init --initial-branch=main --quiet "$repo_dir"
git -C "$repo_dir" config user.name "Cazdo Demo"
git -C "$repo_dir" config user.email "demo@example.com"
git -C "$repo_dir" remote add origin "$origin_dir"

printf 'Cazdo VHS demo repo\n' > "$repo_dir/README.md"
git -C "$repo_dir" add README.md
git -C "$repo_dir" commit --quiet -m "chore: initialize demo repository"
git -C "$repo_dir" push --quiet -u origin main

create_branch() {
  local branch_name="$1"
  local commit_message="$2"
  local file_name="${3:-demo.txt}"
  local push_mode="${4:-local}"

  git -C "$repo_dir" checkout -q -B "$branch_name"
  printf '%s\n' "$branch_name" > "$repo_dir/$file_name"
  git -C "$repo_dir" add "$file_name"
  git -C "$repo_dir" commit --quiet -m "$commit_message"

  if [[ "$push_mode" == "push" || "$push_mode" == "remote-only" ]]; then
    git -C "$repo_dir" push --quiet -u origin "$branch_name"
  fi

  git -C "$repo_dir" checkout -q main

  if [[ "$push_mode" == "remote-only" ]]; then
    git -C "$repo_dir" branch -D "$branch_name" >/dev/null 2>&1
  fi
}

create_branch "feature/101-branch-filtering" "feat: add filtering demo branch" "feature-101.txt" push
create_branch "feature/102-filter-shared-terms" "feat: add shared-term filter branch" "feature-102.txt" local
create_branch "feature/103-work-item-preview" "feat: add default preview branch" "feature-103.txt" push
create_branch "chore/docs-refresh" "docs: add no-work-item branch" "docs-refresh.txt" local
create_branch "bugfix/999-missing-demo-item" "fix: add missing item branch" "bugfix-999.txt" local
create_branch "feature/104-filter-origin-view" "feat: add remote filtering branch" "feature-104.txt" remote-only
create_branch "feature/105-remote-loading" "feat: add remote loading branch" "feature-105.txt" remote-only
create_branch "release/106-demo-polish" "chore: add release demo branch" "release-106.txt" remote-only

git -C "$repo_dir" checkout -q feature/103-work-item-preview

cat > "$wrapper_path" <<EOF
#!/usr/bin/env bash
set -euo pipefail
export CAZDO_DEMO_WORK_ITEMS="$fixture_path"
exec "$binary_path" "\$@"
EOF
chmod +x "$wrapper_path"

printf 'DEMO_ROOT=%s\n' "$demo_root"
printf 'DEMO_REPO=%s\n' "$repo_dir"
printf 'DEMO_ORIGIN=%s\n' "$origin_dir"
printf 'DEMO_WORK_ITEMS=%s\n' "$fixture_path"
printf 'DEMO_LAUNCHER=%s\n' "$wrapper_path"
printf 'WORKSPACE_ROOT=%s\n' "$workspace_root"
