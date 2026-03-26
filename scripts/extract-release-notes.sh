#!/usr/bin/env bash
set -euo pipefail

tag="${1:-}"

if [[ -z "$tag" ]]; then
  echo "usage: scripts/extract-release-notes.sh <tag>" >&2
  exit 1
fi

if [[ ! -f CHANGELOG.md ]]; then
  echo "CHANGELOG.md not found" >&2
  exit 1
fi

notes="$({
  awk -v tag="$tag" '
    $0 ~ ("^## " tag "( |$)") { printing = 1 }
    printing {
      if ($0 ~ /^## / && $0 !~ ("^## " tag "( |$)")) {
        exit
      }
      print
    }
  ' CHANGELOG.md
} || true)"

if [[ -z "$notes" ]]; then
  echo "No changelog section found for $tag" >&2
  exit 1
fi

printf '%s\n' "$notes"
