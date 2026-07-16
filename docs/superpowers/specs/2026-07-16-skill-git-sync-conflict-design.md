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

Pending changes are auto-committed before synchronization. The engine fetches the
configured upstream and rebases local commits onto it. If the rebase encounters a
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

`conflicted` is a failure outcome, not a successful pull or push. The sheet shows
the conflict state and paths, does not show a success toast, and disables Pull and
Push until a fresh status confirms that the repository is usable. The current
release does not expose rebase Continue or Abort because the backend always aborts
failed rebases.

## Tests

- A full rebase conflict returns `conflicted` and leaves repository state clean.
- Push is not called after a rebase conflict.
- Filtered push preserves unmatched remote Skills.
- Changing the filter does not commit deletion of previously synchronized Skills.
- Concurrent edits to the same matched Skill produce a conflict instead of an
  overwrite.
- Successful operations return the actual post-operation status.
