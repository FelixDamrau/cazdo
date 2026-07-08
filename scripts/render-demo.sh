#!/usr/bin/env bash

# Regenerate the README demo assets (GIF + hero still) from the tape.
#
# Builds the demo binary, spins up a deterministic demo repo, renders the tape
# with vhs, and drops the assets into docs/images/. The still is captured by the
# tape's own `Screenshot` command, so there is no ffmpeg/timestamp coupling.

set -euo pipefail

workspace_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tape="$workspace_root/docs/tapes/cazdo-open-nav.tape"
gif_out="$workspace_root/docs/images/cazdo-open-nav.gif"
still_name="cazdo-open-nav-still.png"
still_out="$workspace_root/docs/images/$still_name"

echo "Building demo binary..."
cargo build --quiet --manifest-path "$workspace_root/Cargo.toml"

echo "Creating deterministic demo repo..."
demo_env="$("$workspace_root/scripts/setup-vhs-demo-repo.sh")"
while IFS='=' read -r key value; do
  declare "$key=$value"
done <<< "$demo_env"

# Remove this run's temp repo even if the build/render fails partway.
trap 'rm -rf -- "${DEMO_ROOT:-}"' EXIT

echo "Rendering tape with vhs..."
(
  cd "$DEMO_REPO"
  vhs "$tape" -o "$gif_out"
  mv "$still_name" "$still_out"
)

echo "Done:"
echo "  $gif_out"
echo "  $still_out"
