# Cazdo Terminal Demo Workflow

Use the tape files in this directory as the single source of truth for cazdo terminal demos.

See [vhs](https://github.com/charmbracelet/vhs) for the tape recording tool.

## Render Demo

```bash
# Render GIF for README animation
vhs docs/tapes/cazdo-open-nav.tape

# Render MP4 for better quality
vhs docs/tapes/cazdo-open-nav.tape -o docs/images/cazdo-open-nav.mp4

# Capture a still screenshot from 7.0s in the MP4
ffmpeg -ss 00:00:07.0 -i docs/images/cazdo-open-nav.mp4 -frames:v 1 -update 1 docs/images/cazdo-open-nav-still.png

# Cleanup mp4
rm docs/images/cazdo-open-nav.mp4 
```
