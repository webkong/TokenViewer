# Windows UI Port — Design

**Date**: 2026-08-13
**Status**: Approved (brainstorming)
**Scope**: Port the macOS SwiftUI UI surface to the existing native WPF Windows app, plus the Rust-side Windows data paths needed to feed it.

## Goal

Bring the macOS TokenViewer experience to the existing `windows/TokenViewer/` WPF/.NET 8 scaffold, so a Windows user gets:

- a tray-first shell (compact popover panel + main dashboard window)
- a full usage dashboard (range picker, metric cards, trend chart, heatmap, model/agent breakdown, daily table)
- limits cards, settings, and About aligned with macOS
- local-only data through the same Rust core (`tokenviewer_core.dll`)

## Decisions (from brainstorming)

1. **Direction**: Continue the **native WPF** scaffold. Not the WebView2 + React pattern used by TokenTrackerWin. This matches the existing `openspec/changes/windows-support/proposal.md` ("prefer a native Windows shell instead of a web wrapper") and reuses the existing CoreBridge/Rust-DLL wiring.
2. **Scope this round**: Usage full dashboard, Limits/Settings/About alignment, Tray compact panel. **Skills manager is deferred** (heaviest macOS module; separate effort).
3. **Data**: Include Rust parser Windows-path work this round so the dashboard has real data to render.
4. **Implementation approach**: Extend the single WPF project mirroring the macOS directory layout; **hand-drawn charts** (no chart-library dependency); borderless tray popover window; port the macOS L10n EN/ZH catalog.
5. **Build/verify**: New GitHub Actions workflow builds the Windows app (Rust DLL + `dotnet publish`) and uploads artifacts. Local macOS work verifies only the Rust side (`cargo test`) plus code review.

## Architecture

The Windows app talks to the Rust core through the same C-ABI JSON FFI as macOS:

```
tokenviewer_core.dll ⇄ CoreBridge (P/Invoke, C string JSON) → ViewModel (Dispatcher) → XAML View
```

- FFI contracts are platform-neutral and carry over unchanged (see `macos/TokenViewer/Bridge/TokenViewer-Bridging-Header.h`).
- Agent quota/limits data is fetched client-side (existing `Services/LimitsService.cs`), not through the core — same split as macOS.
- Time-range handling must mirror macOS `AppTime`: compute day/hour ranges from the **local** calendar, convert to **UTC** ISO strings (`YYYY-MM-DDTHH:mm:ss'Z'`) before calling the FFI. Never bucket by UTC directly — the core groups with `strftime(..., 'localtime')`.

## Project structure (extend `windows/TokenViewer/`)

```
windows/TokenViewer/
  App.xaml.cs                     // boot: CoreBridge.init(→ %USERPROFILE%\.tokenviewer\data.db), SettingsStore, TrayController
  MainWindow.xaml/.cs             // TabControl: Usage / Limits / Settings / About
  Views/
    UsageView.xaml                // range picker + 4 metric cards + token stacked bar + trend + heatmap + breakdowns + daily table
    TrendChartControl.xaml/.cs    // hand-drawn line/area chart
    HeatmapControl.xaml/.cs       // hand-drawn 53-week GitHub-style heatmap
    LimitsView.xaml               // two-column agent limit cards (progress, plan label, countdown)
    SettingsView.xaml             // sidebar settings
    AboutView.xaml                // app info, supported-agents badges, update check, links
    PopoverWindow.xaml/.cs        // borderless tray popover panel (mirror of PopoverView)
  ViewModels/
    MainViewModel.cs              // existing — keep
    UsageViewModel.cs             // new: summary/daily/hourly/modelBreakdown/heatmap, SelectedRange, CustomFrom/CustomTo
    LimitsViewModel.cs            // extend: countdown + progress data
    SettingsViewModel.cs          // existing — extend as needed
    UpdateViewModel.cs            // existing — keep
  Models/
    UsageModels.cs                // + DailyPoint, ModelEntry, HeatmapPoint
    LimitsModels.cs               // + plan/window/countdown fields
  Interop/CoreBridge.cs           // + tt_query_daily / tt_query_hourly / tt_query_model_breakdown / tt_query_heatmap
  Services/
    Localization.cs               // ported EN/ZH catalog (~490 keys)
    AppTime.cs                    // local-day ↔ UTC range helpers
    LimitsService.cs              // existing — extend for countdown/progress
    TrayController.cs             // existing — extend: click → PopoverWindow, menu → dashboard
    SettingsStore.cs / LaunchAtStartupManager.cs / UpdateService.cs   // existing — keep
  Infrastructure/  ObservableObject.cs / AsyncRelayCommand.cs          // existing — keep
```

## FFI additions (CoreBridge)

Add P/Invoke wrappers (JSON contracts unchanged from macOS):

- `tt_query_daily(from, to)` → `DailyPoint[]` — `date` = `YYYY-MM-DD` (local day)
- `tt_query_hourly(from, to)` → `DailyPoint[]` — `date` = `YYYY-MM-DDTHH` (local hour)
- `tt_query_model_breakdown(from, to)` → `ModelEntry[]` — `model, source, total_tokens, total_cost_usd, percentage`
- `tt_query_heatmap(weeks)` → `HeatmapPoint[]` — `date, count, level (0–4)`
- `tt_get_provider_status` (alias of agent status; optional)

New model records match the Swift `Codable` shapes (snake_case JSON, case-insensitive deserialization already configured in `CoreBridge.cs`).

## UI components

### Usage dashboard (UsageView)
- Header title + Sync button.
- Segmented range picker: Today / Yesterday / Week / Month / All + Custom date-range capsule.
- 4 metric cards (mirror macOS; the Cost card gets a hover model-breakdown popover).
- Token-type stacked bar (input / output / cached / reasoning).
- `TrendChartControl` — hand-drawn multi-series Catmull-Rom line/area with hover crosshair + tooltip and dashed cost series on a right axis (mirror `TrendChartView.swift`).
- `HeatmapControl` — hand-drawn 53-week grid, 5-level color scale from `HeatmapPoint.level`.
- Model / Agent breakdown side-by-side.
- Daily details table (~14 rows).

### Trend / Heatmap controls
- Implemented with WPF `Path`/`StreamGeometry` and `ItemsControl`/`DrawingVisual`. No NuGet chart dependency.
- Data-bound via `ItemsSource`; a config object describes series (color, type, axis) — kept small and testable.

### Tray compact panel (PopoverWindow)
- `NotifyIcon` click opens a **borderless, topmost** `PopoverWindow` mirroring `PopoverView.swift`: header (logo / app name / sync / settings), 2×2 summary cards, mini trend chart, heatmap, top-4 models, footer quick actions (Dashboard / Settings / Quit).
- Behavior mirrors the macOS `NSPopover` (transient): `Deactivated → Hide`, ESC closes, positioned near the tray icon.
- Tray right-click menu: Open Dashboard / Sync now / Quit (extend existing `TrayController`).

### Limits / Settings / About alignment
- **LimitsView**: two-column `AgentLimitCard` grid — agent icon, plan label, reset countdown badge, per-window progress bars. Active vs inactive sections, empty state. `AgentCountdownKind` ported (today / h / m labels).
- **SettingsView**: sidebar layout; sections General (launch at startup, show tray icon, sync frequency), Appearance (theme / currency / language), Menu Bar (panel sections + agent-visibility chips — maps to the tray panel on Windows), Data (rebuild / reset).
- **AboutView**: app info, supported-agents expandable badge grid, update-checker card, GitHub + website links.

### L10n
- Port the macOS EN/ZH catalog into `Services/Localization.cs` (~490 keys): a static catalog + change notification, driven by the existing Language setting.
- **No hardcoded UI strings** anywhere in the ported views (project convention).

## Rust-side Windows data paths

Add Windows path branches so parsers find real data on Windows, preferring `dirs::data_local_dir()` (`%LOCALAPPDATA%`) over hardcoded strings:

| Parser | Current path (Unix) | Windows target |
|---|---|---|
| `kiro.rs` | `~/.config/Kiro/…`, `~/.local/share/kiro-cli/…` | `cfg(windows)` branch (AppData equivalents) |
| `kilocli.rs` | `~/.local/share/kilo/kilo.db` | `%LOCALAPPDATA%` equivalents |
| `mimocode.rs` | `~/.local/share/mimocode/mimocode.db` | `%LOCALAPPDATA%` equivalents |
| `opencode.rs` | `~/.local/share/opencode/opencode.db` | `%LOCALAPPDATA%` equivalents |
| `codex_home.rs` | macOS scan roots | add Windows scan roots |

- `cursor.rs`, `zed.rs`, `goose.rs`, `utils.rs::vscode_global_storage` already have Windows branches — leave as-is.
- **Idempotency must not regress**: `FileCursor` semantics are platform-independent; cursor format unchanged. Run `cargo test --lib --tests` after the change.

## Error handling

- Sync failure → status message in the window (existing `MainViewModel.Status` pattern).
- Empty query results → empty states (mirror macOS).
- Limits fetch failure → card degrades to install/status info (existing pattern).

## CI (GitHub Actions)

New `.github/workflows/windows-build.yml`:

1. Checkout; install Rust with target `x86_64-pc-windows-msvc`.
2. Install MSVC build tools + OpenSSL (vcpkg).
3. `cargo build --release --target x86_64-pc-windows-msvc` → produces `core/target/x86_64-pc-windows-msvc/release/tokenviewer_core.dll`.
4. `dotnet publish` reusing the logic in `script/windows-release.ps1` → zip.
5. Upload the zip as an artifact.

Triggers: `workflow_dispatch` + push to paths under `windows/`, `core/src/parsers/`, `core/src/ffi.rs`, `.github/workflows/windows-build.yml`. Build-only; no auto-release.

## Phasing (implementation order)

1. **Foundation** — Rust Windows paths + `codex_home` Windows roots; CI green (DLL + WPF compile).
2. **Data layer** — CoreBridge 4 queries + Models + `AppTime` + L10n catalog port.
3. **Usage dashboard** — `UsageView` + `TrendChartControl` + `HeatmapControl` + daily table.
4. **Tray panel + Limits/Settings/About alignment**.

## Verification

- **macOS (this machine)**: `cargo test --lib --tests` for Rust changes; code review for WPF.
- **Windows**: download CI artifacts, manually verify dashboard / tray panel / limits / settings / L10n.

## Risks

- **`git2`/OpenSSL on MSVC**: the skills module compiles `git2` (ssh → libssh2-sys + openssl-sys), which is the most likely Windows build blocker even though the Skills UI is deferred. Mitigation: vcpkg OpenSSL or vendored OpenSSL in CI; revisit only if it blocks the green build.
- **No local Windows environment**: Windows UI can't be run on this macOS machine; correctness rests on CI green builds + manual artifact verification.
- **Deferred Skills**: the 19 skills FFI functions remain compiled (crate is shared), so the Windows DLL still links `git2`; the UI is simply not built this round.

## Out of scope

- Skills manager UI and git-sync UX.
- WebView2 / web-dashboard route (TokenTrackerWin pattern).
- Windows release automation beyond the existing `script/windows-release.ps1` (no auto-publish).
- Cloud sync / accounts.
