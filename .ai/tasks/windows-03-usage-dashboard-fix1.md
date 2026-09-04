# Task: windows-03-usage-dashboard-fix1

## Goal

修复 Usage Dashboard 复审发现的交互、状态与明细问题，使范围变化、hover、loading/error 和 hourly 明细满足任务 03 验收。

## Required changes

1. 保证趋势图每次 Redraw 后 crosshair/tooltip 仍在可视树，hover 可见；增加测试。
2. SelectedRange、CustomFrom、CustomTo 变更触发一次最新 Usage refresh，初始化不重复查询。
3. Usage 页显示本地化 loading 和 sync error 状态，复用 SyncCoordinator 状态，不直接查询 CoreBridge。
4. DailyRowsConverter 聚合 `yyyy-MM-ddTHH` 为日数据，并测试。
5. tooltip 的 Cache 文案走 L10n，并增强相应覆盖测试。
6. 写 implementation handoff，清理 bin/obj，重跑两个 dotnet build 与 `git diff --check`。

## Allowed files

- `windows/TokenViewer/Views/UsageView.xaml`
- `windows/TokenViewer/Views/UsageView.xaml.cs`
- `windows/TokenViewer/Views/TrendChartControl.xaml`
- `windows/TokenViewer/Views/TrendChartControl.xaml.cs`
- `windows/TokenViewer/ViewModels/UsageViewModel.cs`
- `windows/TokenViewer/ViewModels/ShellViewModel.cs`
- `windows/TokenViewer.Tests/ChartControlTests.cs`
- `windows/TokenViewer.Tests/LocalizationCoverageTests.cs`
- `windows/TokenViewer.Tests/UsageViewModelTests.cs`
- `.ai/handoffs/task_c0ce2bcaad18-implementation.md`

## Do not change

- Rust、CoreBridge、LimitsService、tray、settings/about、生产依赖。
