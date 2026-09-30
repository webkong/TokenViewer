# Skill Git Sync and Conflict Handling

## Goal

Make Skill Git synchronization follow normal Git safety rules while preserving the
existing filtered-push feature. A failed synchronization must not leave the
repository in an in-progress rebase, and a filtered push must never delete or
silently overwrite an unselected remote Skill.

## Repository Invariants

- A sync operation starts only when the repository state is clean and the index
  has no unresolved conflicts.
- Pull and full push use one fetch-and-rebase implementation.
- A rebase error is never treated as an empty commit without checking its error
  class and repository state.
- Any rebase conflict aborts the rebase before returning to Swift.
- Push is never attempted after a conflict or incomplete rebase.
- Every completed operation returns a freshly computed repository status.

## Full Pull and Push

Legacy Pull stashes tracked and untracked changes and restores them after remote
integration. Legacy Push auto-commits pending changes after checking for unresolved
conflicts. The engine fetches the configured upstream and rebases local commits onto it. If the rebase encounters a
conflict, it records the conflicted paths, aborts the rebase, and returns a
`conflicted` status. Full push runs only after the same rebase helper succeeds.

## Filtered Push

The filter controls which Skill paths contribute local changes. It does not define
the complete desired contents of the remote repository.

The filtered candidate tree starts from the synchronization base, removes and
re-adds only paths matched by the filter, and preserves every unmatched path. The
candidate is merged against the newly fetched remote tree using the prior remote
or merge-base tree as the three-way merge base. Conflicts in matched paths return
`conflicted`; no commit or push occurs. A successful merged tree is committed on
top of the fetched remote commit and pushed without deleting unmatched paths.

Local files outside the filter remain local changes. Updating the synchronization
baseline must not discard their working-tree contents.

## UI Behavior

Default Sync prepares a persistent plan without checking out conflicts into live
Skills. Git objects retain the base, local and remote snapshots; the task records
per-file choices (local, remote or edited text), and survives sheet/app closure.
The UI shows outgoing/incoming paths and a conflict editor. Configuration and
legacy Pull/Push/Force actions are secondary to preparing and applying this plan.

Preparing uses a three-way tree merge. Applying revalidates the HEAD, index,
worktree, configured remote and fetched remote head. A changed input requires a
new plan. Publication is non-force, and its state is persisted separately from
local application. A retry after publication checks the published commit and
local inputs before applying. Recovery snapshots stay outside the active Skills
tree. A failed local application retains the task and backup references.

Filters are explicitly called upload scope. Unselected remote files are retained;
unselected local edits are preserved on disk. Selected existing files receive the
actual merge result, including remote edits. Deleting a selection from the filter
does not delete the remote Skill. Legacy unresolved Git/stash conflicts block Sync
and must be resolved before preparing a task. Force actions remain confirmed and
are unavailable while a prepared task exists.

## Tests

- A full rebase conflict returns `conflicted` and leaves repository state clean.
- Push is not called after a rebase conflict.
- Filtered push preserves unmatched remote Skills.
- Changing the filter does not commit deletion of previously synchronized Skills.
- Concurrent edits to the same matched Skill produce a conflict instead of an
  overwrite.
- Successful operations return the actual post-operation status.
