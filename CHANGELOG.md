# Changelog

All notable changes to this project are documented in this file.

## v0.1.16 - 2026-03-26

### Features

- **tui:** Add origin remote branch view (#34)
- **cli:** Extend wi preview and add --long flag (#37)

### Bug Fixes

- **config:** Redact PAT display and harden unix config permissions (#33)

### CI

- **github-actions:** Commit Cargo.lock and enforce locked builds (#32)

## Historical Releases

The release history from `v0.1.0` through `v0.1.15` predates changelog generation with conventional commits and `git-cliff`.

- `v0.1.0` to `v0.1.4`: initial cargo-dist release pipeline, protected branches, config-based PAT support, and early branch/work-item workflow improvements.
- `v0.1.5` to `v0.1.8`: better Azure DevOps error handling, branch/work item UX cleanup, Windows-safe ANSI output, and documentation/demo improvements.
- `v0.1.10` to `v0.1.13`: TUI usability work including branch scrollbars, PAT validation improvements, and broader cleanup/testing.
- `v0.1.14` to `v0.1.15`: compact `wi` previews and improved TUI branch selection/error handling.
