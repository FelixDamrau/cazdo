# Cazdo Terminal Demo Workflow

Use the tape files in this directory as the single source of truth for cazdo terminal demos.

See [vhs](https://github.com/charmbracelet/vhs) for the tape recording tool.

## Render Demo

```bash
# Build the local binary used by the demo wrapper
cargo build --quiet

# Create a deterministic temp repo with a local bare origin
eval "$(./scripts/setup-vhs-demo-repo.sh)"

# Render GIF for README animation from the generated demo repo
(
  cd "$DEMO_REPO"
  vhs "$WORKSPACE_ROOT/docs/tapes/cazdo-open-nav.tape" \
    -o "$WORKSPACE_ROOT/docs/images/cazdo-open-nav.gif"
)

# Render MP4 for better quality
(
  cd "$DEMO_REPO"
  vhs "$WORKSPACE_ROOT/docs/tapes/cazdo-open-nav.tape" \
    -o "$WORKSPACE_ROOT/docs/images/cazdo-open-nav.mp4"
)

# Capture a still screenshot from 7.0s in the MP4
ffmpeg -ss 00:00:07.0 -i "$WORKSPACE_ROOT/docs/images/cazdo-open-nav.mp4" -frames:v 1 -update 1 "$WORKSPACE_ROOT/docs/images/cazdo-open-nav-still.png"

# Cleanup mp4
rm "$WORKSPACE_ROOT/docs/images/cazdo-open-nav.mp4"
```

The generated repo uses a local bare `origin` plus `CAZDO_DEMO_WORK_ITEMS`, so the tape stays independent from your current branch layout, PAT, and network access.
