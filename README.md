# cazdo

![CI](https://github.com/FelixDamrau/cazdo/actions/workflows/ci.yml/badge.svg)
![Release](https://github.com/FelixDamrau/cazdo/actions/workflows/release.yml/badge.svg)
![GitHub release](https://img.shields.io/github/v/release/FelixDamrau/cazdo)

*Cats Do Console Azure DevOps.*

`cazdo` is a TUI for Azure DevOps that bridges the gap between your git workflow and issue tracking.

It scans your local branches, extracts work item IDs from their names (e.g., `feature/123-login`), and fetches the corresponding details from Azure DevOps. This allows you to view acceptance criteria, descriptions, and status directly alongside your code.

## Installation

### Linux & macOS
```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/FelixDamrau/cazdo/releases/latest/download/cazdo-installer.sh | sh
```

### Windows (PowerShell)
```powershell
irm https://github.com/FelixDamrau/cazdo/releases/latest/download/cazdo-installer.ps1 | iex
```

## Updating

To update to the latest version, run the installation command again or use:
```bash
cazdo-update
```

## Configuration

### Config File

| Platform | Path |
|----------|------|
| Linux | `~/.config/cazdo/config.toml` |
| macOS | `~/Library/Application Support/cazdo/config.toml` |
| Windows | `%APPDATA%\cazdo\config.toml` |

Example config:

```toml
[azure_devops]
organization_url = "https://dev.azure.com/your-org"
```

Or run `cazdo config` to set up interactively.

### Personal Access Token

Set the `CAZDO_PAT` environment variable with your Azure DevOps PAT:

```bash
export CAZDO_PAT="your-pat-token"
```

The PAT needs **Work Items (Read)** scope.

## Usage

```bash
# Interactive TUI - browse all branches and their work items
cazdo

# Configure interactively
cazdo config

# Show current configuration
cazdo config --show
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `j` / `k` / `Arrow keys` | Navigate branches |
| `o` / `Enter` | Open work item in browser |
| `r` | Refresh current work item |
| `PgUp` / `PgDn` | Scroll work item details |
| `Ctrl+u` / `Ctrl+d` | Scroll half page (vim-style) |
| `q` / `Esc` | Quit |

## Branch Naming

cazdo extracts the **first sequence of digits** found in the branch name to use as the Work Item ID.

| Branch Name | Detected WI |
|-------------|-------------|
| `wi123` | #123 |
| `feature/123-add-login` | #123 |
| `bugfix/issue-42` | #42 |
| `release/v2.1-fix-123` | #2 |

Pattern: First sequence of digits found in the string.

## License

MIT
