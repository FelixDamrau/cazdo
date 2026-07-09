# Cazdo Terminal Demo Workflow

The tape files in this directory are the single source of truth for cazdo
terminal demos. See [vhs](https://github.com/charmbracelet/vhs) for the tape
recording tool (required).

## Regenerate assets

```bash
./scripts/render-demo.sh
```

This builds the demo binary, creates a deterministic temp repo (local bare
`origin` plus the `CAZDO_DEMO_WORK_ITEMS` fixture), renders
`cazdo-open-nav.tape`, and writes:

- `docs/images/cazdo-open-nav.gif` — the README animation
- `docs/images/cazdo-open-nav-still.png` — the hero still, captured by the
  tape's own `Screenshot` command

The temp repo is removed when the script finishes. Using a local bare `origin`
plus the fixture keeps the tape independent from your current branch layout,
PAT, and network access.
