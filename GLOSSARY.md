# Branch and worktree management

cazdo connects Git branches to Azure DevOps work items and manages the
repository's worktrees.

## Language

**Work item**:
An Azure DevOps task, bug, or other tracked item identified by its numeric ID.
A branch may refer to a work item through its name.

**Worktree inventory**:
A snapshot of the repository's main and linked worktrees, including their
locations, branch or detached HEAD, and observed safety state.

**Main worktree**:
The repository's original working directory, which cazdo protects from removal
and metadata pruning.

**Linked worktree**:
An additional working directory registered with the same repository, with its own
HEAD and index.

**Current worktree**:
The worktree in which cazdo is running. It may be the main worktree or a linked
worktree and is protected from removal and metadata pruning.

**Metadata prune**:
Removal of the registration for a missing linked worktree. Its recorded path and
branch are preserved.
_Avoid_: Worktree removal, branch deletion.

**Worktree removal**:
Removal of a linked worktree's working directory and registration. Its branch is
preserved; cazdo permits removal only when its safety checks pass.
_Avoid_: Metadata prune, branch deletion.

**Remote freshness**:
Whether a remote-tracking branch still exists on origin, as observed during the
current session. A branch confirmed absent is stale.
