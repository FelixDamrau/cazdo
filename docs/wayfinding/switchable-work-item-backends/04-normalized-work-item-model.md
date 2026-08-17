# Define the Normalized Work-Item Model

- Type: grilling
- Status: open
- Parent: switchable-work-item-backends
- Blocks: 05-tui-cli-integration, 06-test-and-fixture-strategy
- Blocked by: 01-backend-boundary, 03-forgejo-api-adapter

## Question

Which provider-independent fields and display semantics belong in the shared work-item model consumed by the TUI and CLI?

Compare Azure DevOps work items with Forgejo issues, especially title, numeric ID, type, state, assignee, labels/tags, body/rich text, URL, and missing fields. Avoid forcing Forgejo concepts into Azure-specific enums. Keep provider-native JSON versus unified JSON output deferred unless this decision requires a minimal boundary for it.
