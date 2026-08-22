# Cazdo terminal demo workflow

The tape files in this directory define the cazdo terminal demos. See [vhs](https://github.com/charmbracelet/vhs) for the tape recording tool (required).

## Regenerate assets

```bash
./scripts/render-demo.sh
```

This builds the demo binary, creates a deterministic temp repo with a local bare
`origin` and the `CAZDO_DEMO_WORK_ITEMS` fixture, renders
`cazdo-open-nav.tape`, and writes:

- `docs/images/cazdo-open-nav.gif` — the README animation
- `docs/images/cazdo-open-nav-still.png` — the hero still, captured by the
  tape's own `Screenshot` command

The script removes the temp repo when it finishes. The local bare `origin`
and fixture keep the tape independent of your current branch layout, PAT, and
network access.
