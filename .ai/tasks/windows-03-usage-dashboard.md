# Task: windows-03-usage-dashboard

## Goal

实现数据绑定的完整 WPF Usage Dashboard，包括范围选择、指标、token 类型条、趋势图、53 周热力图、模型/Agent 分布和明细表。

## Current behavior

`MainWindow.xaml` 只有硬编码 Overview 占位内容，没有独立 UsageView 或图表控件。

## Desired behavior

Dashboard 与 macOS UsageView 的结构和状态一致；所有文案走 Localization；绘图无生产 NuGet 依赖。

## Reuse evidence

- 页面结构/token：`macos/TokenViewer/Views/UsageView.swift`。
- 趋势图交互/系列：`macos/TokenViewer/Views/TrendChartView.swift`。
- 数据和范围：`macos/TokenViewer/ViewModels/UsageViewModel.swift`。
- 现有 Windows brand color/card token：`windows/TokenViewer/MainWindow.xaml` 的 `Brand=#059669`、PanelBg/PanelBorder/MutedText/Card。

## UI evidence packet

- 相邻页面：上述三个 macOS 文件和现有 `MainWindow.xaml`。
- 复用组件：任务 02 的 UsageViewModel、Localization；现有 WPF Card/brand tokens 迁至共享资源但视觉值不变。
- 设计 token：emerald `#059669`；现有深色 PanelBg `#111827`、border `#243244`、muted `#94A3B8`、圆角 18、card padding 18。
- 布局规则：主窗口最小 1000×700；宽屏 breakdown 两列，窄于 1100 时垂直排列；主内容可纵向滚动。
- 状态矩阵：loading 禁用范围/同步；empty 显示本地化空态；error 显示状态条；selected 范围为 emerald capsule；单点/零点图表不异常。
- 响应式要求：1240×860 主验收；1000×700 不横向截断关键控件。
- 视觉证据：macOS 源码为结构证据；本机不能运行 WPF，Windows artifact 人工验证。

## File-level patch plan

| 文件 | 允许修改 | 预期改动 | 禁止改动 |
|---|---|---|---|
| `windows/TokenViewer/App.xaml` | yes | 共享 brush/card/button tokens | 改品牌色 |
| `windows/TokenViewer/MainWindow.xaml` | yes | 用独立 Usage/其他 tabs 组合，移除 Overview 占位 | 实现 limits/settings 内容 |
| `windows/TokenViewer/MainWindow.xaml.cs` | yes | 最小 tab/navigation glue | 业务查询 |
| `windows/TokenViewer/Views/UsageView.xaml` | add | dashboard 布局/绑定/状态 | 硬编码文案 |
| `windows/TokenViewer/Views/UsageView.xaml.cs` | add | custom date/hover 最小行为 | 数据查询 |
| `windows/TokenViewer/Views/TrendChartControl.xaml` | add | 绘图 surface/tooltip | 第三方 chart |
| `windows/TokenViewer/Views/TrendChartControl.xaml.cs` | add | StreamGeometry、axes、hover/crosshair | 通用图表框架重构 |
| `windows/TokenViewer/Views/HeatmapControl.xaml` | add | heatmap surface/legend | 第三方 chart |
| `windows/TokenViewer/Views/HeatmapControl.xaml.cs` | add | 53-week cell mapping/tooltip | 改 core heatmap level |
| `windows/TokenViewer.Tests/ChartControlTests.cs` | add | geometry/cell mapping 无 UI 状态测试 | screenshot 伪验证 |
| `windows/TokenViewer.Tests/LocalizationCoverageTests.cs` | add | XAML 硬编码扫描及 key parity | 扫描第三方/生成文件 |

## Change budget

- 允许文件：仅上表 11 个。
- 新生产依赖：none。
- 公共数据模型/API：no。
- 不重命名/移动既有模块。

## Invariants

- 不在 View code-behind 直接调用 CoreBridge。
- daily/hourly 由 ViewModel 决定。
- cost 使用右轴虚线；token 系列使用既定颜色。
- 53 周列映射按本地日期且固定显示范围。

## Acceptance criteria

- 1240×860 下所有指定模块可访问，1000×700 下无关键控件横向丢失。
- hover 显示对应日期/系列值；空/单点数据不抛异常。
- Cost card 可显示 model breakdown hover 内容。
- XAML 新用户文案全部绑定 Localization。
- WPF build 与 chart mapping tests 通过。

## Required tests

- Windows CI 的 `dotnet test`、`dotnet build`。
- `rtk git diff --check`。
- 人工 artifact：1240×860、1000×700，loading/empty/populated/error。

## Escalation triggers

- 需要第三方 chart/SVG/UI 包。
- 既定模型缺字段导致修改任务 02 文件。
- macOS 结构在 WPF 上必须明显改变交互。

## Do not change

- Rust、CoreBridge、limits、tray、settings/about、资源生成。

