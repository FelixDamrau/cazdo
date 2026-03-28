#!/usr/bin/env bash
set -euo pipefail

base_ref="${1:-}"
head_ref="${2:-HEAD}"
script_dir="$(cd -- "$(dirname -- "$0")" && pwd)"

if [[ -z "$base_ref" ]]; then
  echo "usage: scripts/check-release-pr.sh <base-ref> [head-ref]" >&2
  exit 1
fi

base_version="$({ git show "${base_ref}:Cargo.toml" 2>/dev/null || true; } | python -c 'import re, sys; data=sys.stdin.read(); match=re.search(r"(?m)^version = \"([0-9]+\.[0-9]+\.[0-9]+)\"$", data); print(match.group(1) if match else "")')"

if [[ -z "$base_version" ]]; then
  echo "could not read package version from Cargo.toml at ${base_ref}" >&2
  exit 1
fi

head_version="$(python - <<'PY'
from pathlib import Path
import re

content = Path('Cargo.toml').read_text()
match = re.search(r'(?m)^version = "([0-9]+\.[0-9]+\.[0-9]+)"$', content)
if not match:
    raise SystemExit('could not read package version from current Cargo.toml')
print(match.group(1))
PY
)"

if [[ "$base_version" == "$head_version" ]]; then
  echo "Cargo.toml version unchanged (${head_version}); skipping release changelog check"
  exit 0
fi

tag="v${head_version}"

if ! "$script_dir/extract-release-notes.sh" "$tag" >/dev/null; then
  echo "CHANGELOG.md must contain a section for ${tag} when Cargo.toml version changes from ${base_version} to ${head_version}" >&2
  exit 1
fi

echo "Release metadata check passed for ${tag}"
