# Define Backend Selection and Configuration

- Type: grilling
- Status: in_progress
- Assignee: OpenCode
- Parent: switchable-work-item-backends
- Blocks: 05-tui-cli-integration, 06-test-and-fixture-strategy
- Blocked by: 01-backend-boundary

## Question

How should automatic backend selection and explicit configuration work?

The design must define:

- `origin` remote precedence when multiple remotes exist.
- Host recognition for Forgejo and future providers.
- Explicit backend override syntax.
- Configured default behavior for unknown or missing remotes.
- Error behavior when automatic detection cannot decide.
- Backend-specific credential sources and precedence.
