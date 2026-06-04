# Windows Support Proposal

## Goal

Add a native Windows desktop app that mirrors the macOS experience as closely as practical:

- tray-first shell
- usage summary
- provider limits
- settings
- update flow
- local-only data access through the existing Rust core

## Constraints

- Reuse the Rust core and data model.
- Keep macOS-specific code isolated.
- Follow the existing brand and copy conventions.
- Prefer a native Windows shell instead of a web wrapper.

## Proposed Direction

Build a Windows app in .NET using a native shell and a shared C FFI bridge to the Rust core.
The Windows UI should be split into the same conceptual areas as macOS:

1. Summary / dashboard panel
2. Provider limits
3. Settings
4. Update handling

## Rollout Plan

### Phase 1

- Create the Windows project skeleton.
- Add the Rust FFI bridge for Windows.
- Show the tray icon and open the main window.
- Render a basic summary view from local data.

### Phase 2

- Add limits and provider status.
- Add settings persistence.
- Match macOS color, spacing, and typography as closely as the Windows platform allows.

### Phase 3

- Add update download/install flow.
- Add packaging / installer support.
- Add release automation.

