# Define the Test and Fixture Strategy

- Type: grilling
- Status: open
- Parent: switchable-work-item-backends
- Blocks: none
- Blocked by: 01-backend-boundary, 02-selection-and-config, 03-forgejo-api-adapter, 04-normalized-work-item-model, 05-tui-cli-integration

## Question

What tests and deterministic fixtures are required before implementation can be considered complete?

Cover provider selection, remote parsing, configuration precedence, unknown remotes, Forgejo API responses and errors, Azure regression behavior, normalized rendering, CLI output, TUI background loading, and public versus authenticated Forgejo access. Prefer local fixtures or mock HTTP responses over live credentials and network calls.
