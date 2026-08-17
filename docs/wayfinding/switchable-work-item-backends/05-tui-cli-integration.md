# Define TUI and CLI Integration

- Type: grilling
- Status: open
- Parent: switchable-work-item-backends
- Blocks: 06-test-and-fixture-strategy
- Blocked by: 02-selection-and-config, 04-normalized-work-item-model

## Question

How should the TUI and CLI consume the selected backend without knowing whether an item came from Azure DevOps or Forgejo?

Resolve client construction and lifetime, background loading, refresh behavior, verification commands, provider-aware errors, browser links, branch-number semantics, and any user-visible naming changes. Preserve current Azure behavior where the normalized model permits it.
