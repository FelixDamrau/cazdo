# Define the Provider-Neutral Backend Boundary

- Type: grilling
- Status: closed
- Assignee: OpenCode
- Parent: switchable-work-item-backends
- Blocks: 02-selection-and-config, 04-normalized-work-item-model, 05-tui-cli-integration, 06-test-and-fixture-strategy
- Blocked by: none

## Question

What provider-neutral interface should replace the current Azure-specific client boundary while supporting the existing read-only operations: fetch a normalized work item by numeric ID, optionally expose provider data, open the item URL, and verify connectivity?

The decision must account for the current live/fixture provider split, async behavior, error handling, and a future GitHub adapter without adding mutation capabilities prematurely.

## Resolution

Use a trait-backed, provider-neutral backend selected before either the TUI or CLI command path begins. Each concrete backend is constructed with its repository/project context, so lookup accepts only a numeric `u64` work-item ID.

The initial contract exposes:

- `kind()` for source identification in diagnostics and native JSON output.
- `get_work_item(id)` for the normalized model.
- `get_raw_item(id)` for provider-native diagnostic JSON.
- `verify_connection()` for backend connectivity checks.

Adapters translate their native API payloads internally and return `anyhow::Result` with actionable backend context. The UI and CLI open browser links from the normalized work-item URL and contain no provider-specific branching.

The contract intentionally excludes listing, search, issue/work-item mutation, comments, and any other capabilities beyond the current read-only behavior.
