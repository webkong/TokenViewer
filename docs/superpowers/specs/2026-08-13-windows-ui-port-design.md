# Windows UI Port — Design

**Date**: 2026-08-13
**Status**: Reviewed design — ready for implementation
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
3. **Limits parity**: Port all 15 canonical macOS limits integrations, not only the seven currently present in Windows. Parser-only Agents remain usage-only.
4. **Data**: Include Rust parser Windows-path work this round so the dashboard has real data to render.
5. **Implementation approach**: Extend the single WPF project mirroring the macOS directory layout; **hand-drawn charts** (no chart-library dependency); borderless tray popover window; port the macOS L10n EN/ZH catalog.
6. **Assets**: Reuse the existing brand artwork, but generate WPF-compatible bitmap resources and a native tray `.ico`; do not add an SVG-rendering NuGet dependency solely for icons.
7. **Build/verify**: Extend the existing GitHub Actions Windows workflow to build the Rust DLL and `dotnet publish` output and upload artifacts. Local macOS work verifies Rust; Windows CI performs .NET compilation and automated tests before manual artifact verification.

## Architecture

The Windows app talks to the Rust core through the same C-ABI JSON FFI as macOS:

```
tokenviewer_core.dll ⇄ CoreBridge (P/Invoke, UTF-8 C string JSON) → SyncCoordinator → ViewModels (Dispatcher) → XAML Views
```

- The exported Rust FFI contracts are platform-neutral and carry over unchanged (see `macos/TokenViewer/Bridge/TokenViewer-Bridging-Header.h`). The Windows P/Invoke declarations must explicitly marshal every input string as UTF-8 (`StringMarshalling.Utf8` or `UnmanagedType.LPUTF8Str`); `CharSet.Ansi` is not valid because database paths may contain non-ASCII characters.
- Agent quota/limits data is fetched client-side (existing `Services/LimitsService.cs`), not through the core — same split as macOS.
- Time-range handling must mirror macOS `AppTime`: compute day/hour ranges from the **local** calendar, convert to **UTC** ISO strings (`YYYY-MM-DDTHH:mm:ss'Z'`) before calling the FFI. Never bucket by UTC directly — the core groups with `strftime(..., 'localtime')`.

### Sync and refresh ownership

`SyncCoordinator` is the only component allowed to call `tt_sync_all` / `tt_rebuild_all`. Main-window, tray, and popover commands all delegate to it.

- Serialize sync/rebuild operations with one shared async gate; a second request joins or is ignored while one is active.
- Publish shared `IsSyncing`, status, error, and `SyncCompleted` state on the WPF Dispatcher.
- After a successful sync or rebuild, refresh `UsageViewModel`, Agent status, and every visible popover/dashboard projection from the same completion event.
- Query calls run off the UI thread. Calls using the single Rust handle are serialized so a SQLite connection is never entered concurrently.
- `MainViewModel` owns shell-level status only; it does not maintain a second, independent usage summary.

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
    SyncCoordinator.cs            // single sync/rebuild owner + refresh notification
    TrayController.cs             // existing — extend: click → PopoverWindow, menu → dashboard
    SettingsStore.cs / LaunchAtStartupManager.cs / UpdateService.cs   // existing — keep
  Infrastructure/  ObservableObject.cs / AsyncRelayCommand.cs          // existing — keep
  Resources/
    brand-logos/*.png             // generated WPF bitmap variants of the existing SVG logos
    TokenViewer.ico               // multi-size tray/application icon
```

## FFI additions (CoreBridge)

Add P/Invoke wrappers (JSON contracts unchanged from macOS):

- `tt_query_daily(from, to)` → `DailyPoint[]` — `date` = `YYYY-MM-DD` (local day)
- `tt_query_hourly(from, to)` → `DailyPoint[]` — `date` = `YYYY-MM-DDTHH` (local hour)
- `tt_query_model_breakdown(from, to)` → `ModelEntry[]` — `model, source, total_tokens, total_cost_usd, percentage`
- `tt_query_heatmap(weeks)` → `HeatmapPoint[]` — `date, count, level (0–4)`
- `tt_rebuild_all()` → serialized rebuild result; invoked only by `SyncCoordinator`
- Keep the existing `tt_get_agent_status`; do not introduce the legacy `tt_get_provider_status` alias.

New model records match the Swift `Codable` shapes. Rust emits `snake_case`; `PropertyNameCaseInsensitive` does **not** translate underscores, so every C# record property must use `[JsonPropertyName("snake_case_name")]`, or the shared options must use `.NET 8`'s `JsonNamingPolicy.SnakeCaseLower`. Use one convention consistently and add a contract-deserialization test. Returned strings continue to be decoded with `Marshal.PtrToStringUTF8` and freed in `finally` via `tt_free_string`.

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
- Handle left and right mouse buttons separately so right-click continues to open the context menu. Position with `Shell_NotifyIconGetRect` when available and fall back to the cursor/work-area coordinates; clamp the popover to the active monitor work area.

### Limits / Settings / About alignment
- **LimitsView**: two-column `AgentLimitCard` grid — agent icon, plan label, reset countdown badge, per-window progress bars. Active vs inactive sections, empty state. `AgentCountdownKind` ported (today / h / m labels). Port all canonical integrations: Claude Code, ChatGPT (`codex`), Cursor, Kiro, GitHub Copilot, Kimi, Antigravity, Zed, Trae, Windsurf, Qoder, CodeBuddy, WorkBuddy, Gemini, and ZCode. The current Windows service already has only Claude, ChatGPT, Cursor, Gemini, Kiro, Kimi, and Antigravity; the other eight are implementation work in this phase.
- **Limits data access**: do not depend on an externally installed `sqlite3.exe`. Cursor and any other SQLite-backed account cache must be read through an in-process implementation (prefer a narrowly scoped Rust FFI helper or an existing bundled database capability; adding a .NET SQLite dependency requires a separate justification). Preserve the macOS degradation behavior when a cache is locked or unavailable.
- **SettingsView**: sidebar layout; sections General (launch at startup, show tray icon, sync frequency), Appearance (theme / currency / language), Menu Bar (panel sections + agent-visibility chips — maps to the tray panel on Windows), Data (rebuild / reset). Rebuild calls `SyncCoordinator.RebuildAsync`, requires confirmation, and refreshes all projections. Reset Settings requires separate confirmation, restores documented defaults, and does not delete usage data. Hiding the tray icon is allowed only while the dashboard remains reachable; the setting takes effect immediately.
- **AboutView**: app info, supported-agents expandable badge grid, update-checker card, GitHub + website links.

### Brand resources

- Keep the macOS `brand-logos/*.svg` files as the source of truth. A deterministic repository script generates fixed-size transparent PNG variants for WPF; generated files are committed so Windows builds need no SVG tooling.
- Generate `TokenViewer.ico` with 16/20/24/32/48/64/128/256 px entries and use it for both the executable and `NotifyIcon`.
- Declare the PNG and ICO files as WPF `Resource` items and verify they survive `dotnet publish`. Agent IDs and fallback behavior must use the same registry keys as macOS.

### L10n
- Port the macOS EN/ZH catalog into `Services/Localization.cs` (~490 keys): a static catalog + change notification, driven by the existing Language setting.
- Expose strings through a binding-friendly indexer/service and raise the appropriate property/indexer notification when language changes so already-open windows update immediately.
- **No hardcoded UI strings** anywhere in the ported views (project convention).

## Rust-side Windows data paths

Add Windows path branches so parsers find real data on Windows. Do not treat all AppData locations as interchangeable: IDE/VS Code-style state normally belongs under `%APPDATA%` (Roaming), while CLI/XDG-style databases normally belong under `%LOCALAPPDATA%`. Probe an ordered list when released versions have used more than one location.

| Parser | Ordered Windows candidates | Validation requirement |
|---|---|---|
| `kiro.rs` IDE | `%APPDATA%\Kiro\User\globalStorage\kiro.kiroagent\dev_data`, then compatibility candidates discovered from installed versions | Verify `devdata.sqlite`, sibling JSONL, and settings paths independently |
| `kiro.rs` CLI | `%LOCALAPPDATA%\kiro-cli\data.sqlite3`, then the documented/user-home compatibility locations; `~\.kiro\sessions\…` remains under the profile | Verify both legacy DB and v3 sessions |
| `kilocli.rs` | `%LOCALAPPDATA%\kilo\kilo.db`, then verified compatibility candidates | Confirm against a real Windows installation |
| `mimocode.rs` | `%LOCALAPPDATA%\mimocode\mimocode.db`, then verified compatibility candidates | Confirm against a real Windows installation |
| `opencode.rs` | `%LOCALAPPDATA%\opencode\opencode.db`, then verified compatibility candidates | Confirm against a real Windows installation |
| `codex_home.rs` | default `~\.codex`, `CODEX_HOME`, known Windows host locations, then narrow `%APPDATA%` / `%LOCALAPPDATA%` scan roots | Retain pruning and bounded-depth rules |

- `cursor.rs`, `zed.rs`, `goose.rs`, `utils.rs::vscode_global_storage` already have Windows branches — leave as-is.
- Centralize Windows base-directory resolution in a small helper. Production resolution may use `dirs::data_local_dir()` / `dirs::config_dir()`, but parsers must still accept injectable base paths or environment values so temporary-directory tests do not read the developer's real profile.
- Before finalizing a candidate, record the application/version used to verify it; an unverified guessed path must not be the only candidate.
- **Idempotency must not regress**: `FileCursor` semantics are platform-independent; cursor format unchanged. Run `cargo test --lib --tests` after the change.

## Error handling

- Sync failure → status message in the window (existing `MainViewModel.Status` pattern).
- Empty query results → empty states (mirror macOS).
- Limits fetch failure → card degrades to install/status info (existing pattern).
- FFI initialization failure → localized blocking error with the resolved database path and a retry action; never continue with a zero handle as if the app were ready.
- Rebuild/reset actions → confirmation, disabled controls while running, localized success/failure result.

## CI (GitHub Actions)

Extend the existing `.github/workflows/windows-build.yml` rather than creating a second workflow:

1. Checkout; install Rust with target `x86_64-pc-windows-msvc`.
2. Install .NET 8. The hosted runner already includes MSVC; install OpenSSL/vcpkg only if the actual `git2` build proves it is required, and pin/cache that setup if added.
3. `cargo build --release --target x86_64-pc-windows-msvc` → produces `core/target/x86_64-pc-windows-msvc/release/tokenviewer_core.dll`.
4. `dotnet publish` reusing the logic in `script/windows-release.ps1` → zip.
5. Upload the zip as an artifact.

Triggers: preserve `workflow_dispatch`, pull requests, and relevant pushes. Keep the current broad `core/**` trigger (pricing/model/skills dependency changes can also break the DLL), plus `windows/**`, `script/windows-release.ps1`, and the workflow itself. Build-only; no auto-release.

The job must run Rust tests, .NET tests, `cargo build --release --target x86_64-pc-windows-msvc`, and `dotnet publish` before uploading the existing zip artifact. Verify that `tokenviewer_core.dll`, `TokenViewer.ico`, and all brand resources exist in the publish output.

## Phasing (implementation order)

1. **Foundation** — UTF-8 CoreBridge declarations; Rust Windows paths + `codex_home` Windows roots; extend the existing CI and make DLL + WPF compilation green.
2. **Shared data layer** — CoreBridge queries/rebuild + Models + `AppTime` + `SyncCoordinator` + L10n catalog and contract tests.
3. **Usage dashboard** — `UsageView` + `TrendChartControl` + `HeatmapControl` + daily table, wired to shared sync completion.
4. **Resources + tray panel** — deterministic brand assets, native ICO, `PopoverWindow`, positioning and shared refresh.
5. **Limits/Settings/About parity** — port all eight missing limits integrations, remove the external `sqlite3.exe` dependency, and complete data/settings actions.

## Verification

- **macOS (this machine)**: `cargo test --lib --tests` for Rust changes; code review for WPF.
- **Windows CI**: Rust tests, .NET tests, release DLL build, WPF publish, and required-artifact assertions must pass on every relevant PR.
- **Windows manual artifact pass**: verify initial launch with an ASCII and a non-ASCII profile/database path; dashboard ranges; sync/rebuild propagation; empty/error states; tray left/right-click and multi-monitor positioning; all 15 limits cards; settings; About; live EN/ZH switching; and published icons/resources.

Required automated coverage:

- `AppTime`: Today, Yesterday, trailing week/month, inclusive Custom ranges, month/year boundaries, and DST transitions.
- CoreBridge: UTF-8 database path initialization and fixture JSON deserialization for every response model.
- `SyncCoordinator`: concurrent request serialization, failure propagation, and refresh notifications to Usage/Agent/popover consumers.
- `UsageViewModel`: range changes, hourly-vs-daily selection, empty/single-point data, and stale-result cancellation/versioning.
- Charts: heatmap date-to-cell mapping for exactly 53 displayed weeks; trend control empty, single-point, and multi-axis geometry.
- L10n: EN/ZH key parity and a test that ported XAML contains no user-facing hardcoded strings.

## Risks

- **`git2`/OpenSSL on MSVC**: the skills module compiles `git2` (ssh → libssh2-sys + openssl-sys), which is the most likely Windows build blocker even though the Skills UI is deferred. Mitigation: vcpkg OpenSSL or vendored OpenSSL in CI; revisit only if it blocks the green build.
- **No local Windows environment**: Windows UI can't be run on this macOS machine; correctness rests on CI green builds + manual artifact verification.
- **Deferred Skills**: the 19 skills FFI functions remain compiled (crate is shared), so the Windows DLL still links `git2`; the UI is simply not built this round.
- **Path uncertainty**: Windows application data locations can change between app generations. Use ordered candidates, document verified versions, and degrade to “not installed” without touching unrelated files.
- **WPF rendering cost**: charts and a 53-week heatmap can trigger excessive layout/allocations. Freeze reusable brushes/geometries, redraw only on data/size changes, and validate interaction with representative large ranges.

## Out of scope

- Skills manager UI and git-sync UX.
- WebView2 / web-dashboard route (TokenTrackerWin pattern).
- Windows release automation beyond the existing `script/windows-release.ps1` (no auto-publish).
- Cloud sync / accounts.
