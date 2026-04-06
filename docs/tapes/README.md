# Cazdo Terminal Demo Workflow

Use the tape files in this directory as the single source of truth for cazdo terminal demos.

See [vhs](https://github.com/charmbracelet/vhs) for the tape recording tool.

## Render Demo

These commands assume Fish shell.

```fish
# Build the local binary used by the demo wrapper
cargo build --quiet

# Create a deterministic temp repo with a local bare origin
for line in (./scripts/setup-vhs-demo-repo.sh)
    set parts (string split -m1 '=' $line)
    set -gx $parts[1] $parts[2]
end

# Render GIF for README animation from the generated demo repo
pushd $DEMO_REPO
vhs $WORKSPACE_ROOT/docs/tapes/cazdo-open-nav.tape \
    -o $WORKSPACE_ROOT/docs/images/cazdo-open-nav.gif
popd

# Render MP4 for better quality
pushd $DEMO_REPO
vhs $WORKSPACE_ROOT/docs/tapes/cazdo-open-nav.tape \
    -o $WORKSPACE_ROOT/docs/images/cazdo-open-nav.mp4
popd

# Capture a still screenshot while work item #103 and its description are visible
ffmpeg -ss 00:00:02.2 -i $WORKSPACE_ROOT/docs/images/cazdo-open-nav.mp4 -frames:v 1 -update 1 $WORKSPACE_ROOT/docs/images/cazdo-open-nav-still.png

# Cleanup mp4
rm $WORKSPACE_ROOT/docs/images/cazdo-open-nav.mp4

# Remove temporary demo repos when you no longer need them
./scripts/cleanup-vhs-demo-repos.sh
```

The generated repo uses a local bare `origin` plus `CAZDO_DEMO_WORK_ITEMS`, so the tape stays independent from your current branch layout, PAT, and network access.
