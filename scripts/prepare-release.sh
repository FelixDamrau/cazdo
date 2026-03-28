#!/usr/bin/env bash
set -euo pipefail

version_input="${1:-}"

if [[ -z "$version_input" ]]; then
  echo "usage: scripts/prepare-release.sh <version>" >&2
  exit 1
fi

version="${version_input#v}"
tag="v${version}"

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "version must match X.Y.Z" >&2
  exit 1
fi

for path in Cargo.toml CHANGELOG.md cliff.toml; do
  if [[ ! -f "$path" ]]; then
    echo "required file missing: $path" >&2
    exit 1
  fi
done

if git rev-parse -q --verify "refs/tags/$tag" >/dev/null 2>&1; then
  echo "tag already exists: $tag" >&2
  exit 1
fi

if grep -q "^## ${tag}\b" CHANGELOG.md; then
  echo "CHANGELOG.md already contains section for $tag" >&2
  exit 1
fi

python - "$version" <<'PY'
from pathlib import Path
import re
import sys

new_version = sys.argv[1]
cargo_toml = Path("Cargo.toml")
content = cargo_toml.read_text()
match = re.search(r'(?m)^version = "([0-9]+\.[0-9]+\.[0-9]+)"$', content)
if not match:
    raise SystemExit("could not find package version in Cargo.toml")

current_version = match.group(1)

def parse(version: str) -> tuple[int, int, int]:
    return tuple(int(part) for part in version.split("."))

if parse(new_version) <= parse(current_version):
    raise SystemExit(f"new version {new_version} must be greater than current version {current_version}")

updated = content[:match.start(1)] + new_version + content[match.end(1):]
cargo_toml.write_text(updated)
PY

git-cliff --config cliff.toml --unreleased --tag "$tag" --prepend CHANGELOG.md
cargo update

echo "Prepared release $tag"
