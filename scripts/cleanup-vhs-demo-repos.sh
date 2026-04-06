#!/usr/bin/env bash

set -euo pipefail

tmp_root="${TMPDIR:-/tmp}"
pattern="$tmp_root"/cazdo-vhs-demo-*

shopt -s nullglob
repos=( $pattern )
shopt -u nullglob

if [[ ${#repos[@]} -eq 0 ]]; then
  printf 'No VHS demo repos found in %s\n' "$tmp_root"
  exit 0
fi

printf 'Found %d VHS demo repo(s):\n' "${#repos[@]}"
for repo in "${repos[@]}"; do
  printf '  %s\n' "$repo"
done

printf 'Delete all of these? [y/N] '
read -r reply

case "$reply" in
  y|Y|yes|YES)
    rm -rf -- "${repos[@]}"
    printf 'Deleted %d VHS demo repo(s).\n' "${#repos[@]}"
    ;;
  *)
    printf 'Aborted.\n'
    ;;
esac
