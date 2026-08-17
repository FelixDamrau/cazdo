# Switchable Work-Item Backends

## Destination

Define an implementation-ready, provider-neutral read-only backend design that preserves Azure DevOps behavior, adds Forgejo Issues support, and leaves a clear seam for a future GitHub Issues adapter.

Backend selection should normally inspect the `origin` Git remote, while explicit configuration can override detection. Forgejo repository identity should come from the remote URL.

## Notes

- This is a decisions-first milestone. Implementation will be tracked separately.
- The current application is Azure DevOps-specific.
- Initial provider scope is Azure DevOps plus Forgejo.
- GitHub support is out of scope for implementation, but the boundary should not prevent a future adapter.
- Initial backend capabilities are read-only: fetch an item, provide normalized data, open its URL, and verify connectivity.
- Raw/native JSON versus unified JSON output is deferred.

## Decisions so far

- [Define the Provider-Neutral Backend Boundary](01-backend-boundary.md) — Construct a context-bound trait-backed backend before UI/CLI entry; expose normalized lookup, native diagnostic lookup, source kind, and connectivity verification only.

## Not yet specified

- Configuration keys, precedence, and environment-variable names.
- Supported Forgejo remote URL forms, API endpoint derivation, and authentication details.
- The normalized representation of state, type, labels, assignees, and rich text.
- How provider selection and failures are presented in the TUI and CLI.
- Whether provider-native JSON and unified JSON become separate output modes.

## Out of scope

- Implementing a GitHub backend in this milestone.
- Creating, editing, or commenting on issues/work items.
- Replacing the existing Git branch backend.
- Deciding raw/native JSON output before the provider-neutral read path is established.
