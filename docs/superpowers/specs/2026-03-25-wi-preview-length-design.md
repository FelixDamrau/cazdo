# WI Preview Length Design

## Goal

Make `cazdo wi` show a bit more description context without turning it into an unbounded dump.

## Current State

- `cazdo wi` renders a compact single-line description preview.
- The preview is currently truncated to 220 characters in `src/commands.rs`.
- The command accepts an optional work item ID and otherwise resolves the work item from the current branch.

## Chosen Approach

- Increase the default description preview length from 220 to 320 characters.
- Add a `--long` flag to `cazdo wi` that increases the preview length to 600 characters.
- Keep the existing output structure and truncation behavior.

## Why

- The default output stays compact and scannable.
- Users who want more context get an explicit opt-in without needing config.
- The change is small, predictable, and fits the current CLI behavior.

## CLI Behavior

- `cazdo wi` uses the default preview length.
- `cazdo wi --long` uses the extended preview length.
- `cazdo wi 120 --long` and `cazdo wi --long 120` should both parse correctly if clap permits that ordering.

## Implementation Notes

- Update `src/cli.rs` so the `Wi` subcommand accepts a boolean `long` flag.
- Update the command handler in `src/commands.rs` to select the preview limit based on the flag.
- Keep `compact_text_preview` as the single truncation function.
- Add CLI parsing tests and preview-length tests.

## Error Handling

- No new error paths are required.
- Missing descriptions should continue to render as `(none)`.

## Testing

- Add a CLI parsing test for `cazdo wi --long`.
- Add a CLI parsing test for `cazdo wi 120 --long`.
- Add command-level tests that verify the chosen preview limits produce different truncation lengths.
