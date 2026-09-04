# Task: windows-04-resources-tray

## Goal

提供可发布的 WPF 品牌 PNG/原生 ICO，并实现左键打开、右键菜单、多显示器安全定位且共享刷新状态的托盘 PopoverWindow。

## Current behavior

NotifyIcon 使用 `SystemIcons.Application`，双击打开主窗口，无 compact panel；项目没有 Windows 品牌资源目录。

## Desired behavior

左键打开 transient popover，右键保留菜单；Popover 展示 header、2×2 cards、mini trend、heatmap、top models 和快捷动作；图标资源进入 publish 输出。

## Reuse evidence

- 内容/交互：`macos/TokenViewer/Views/PopoverView.swift`、`StatusBarController.swift`。
- Agent logo source：`macos/TokenViewer/Resources/brand-logos/*.svg`、`Services/AgentRegistry.swift`。
- 图表：任务 03 的 TrendChartControl/HeatmapControl。
- 托盘基础：`windows/TokenViewer/Services/TrayController.cs`。

## UI evidence packet

- 相邻组件：macOS PopoverView、Windows UsageView。
- 复用组件：UsageViewModel、SyncCoordinator、Localization、TrendChartControl、HeatmapControl。
- token：brand/card tokens 与任务 03 完全一致。
- 布局：固定约 420×680，活动显示器 work area 内夹紧，任务栏任意边均可定位。
- 状态矩阵：closed/open/loading/empty/error/deactivated；sync disabled while running。
- 响应式：固定 panel；DPI-aware，不跨出 work area。
- 视觉证据：macOS PopoverView 结构 + Windows artifact 截图。

## File-level patch plan

| 文件 | 允许修改 | 预期改动 | 禁止改动 |
|---|---|---|---|
| `script/generate-windows-assets.sh` | add | SVG→确定性 PNG/ICO 生成；无网络 | 改源 SVG |
| `windows/TokenViewer/Resources/brand-logos/*.png` | add | committed generated assets | 手工不一致图标 |
| `windows/TokenViewer/Resources/TokenViewer.ico` | add | 多尺寸 ICO | SystemIcons fallback 作为正常路径 |
| `windows/TokenViewer/TokenViewer.csproj` | yes | ApplicationIcon/Resource/publish 配置 | 新 NuGet 包 |
| `windows/TokenViewer/Services/AgentRegistry.cs` | add | source→名称/色/资源映射 | 改 source IDs |
| `windows/TokenViewer/Services/TrayController.cs` | yes | 左/右键、popover、菜单、本地化、图标 | 双重 sync 状态 |
| `windows/TokenViewer/Views/PopoverWindow.xaml` | add | compact panel | 硬编码文案 |
| `windows/TokenViewer/Views/PopoverWindow.xaml.cs` | add | Deactivated/ESC/定位 | 独立数据查询 |
| `windows/TokenViewer/App.xaml.cs` | yes | 注入 shared dependencies/生命周期 | 新第二实例 coordinator |
| `windows/TokenViewer.Tests/TrayPlacementTests.cs` | add | work-area clamp/edge tests | 操作真实 tray |
| `.github/workflows/windows-build.yml` | yes | 断言 ICO/brand assets | 自动发布 |

## Change budget

- 允许文件：上表；PNG glob 仅现有 canonical/usage Agent logo。
- 允许新增依赖：none；脚本只能使用仓库/runner已有工具，缺失则升级。
- 公共 API/持久化：no。

## Invariants

- 源 SVG 不修改，是资源事实来源。
- 右键不触发 popover；Deactivated/ESC 只 Hide，不销毁共享 VM。
- tray/main/popover 共用 coordinator 和 UsageViewModel。
- 发布包离线包含全部运行时资源。

## Acceptance criteria

- ICO 包含 16/20/24/32/48/64/128/256 px；NotifyIcon 使用它。
- Shell_NotifyIconGetRect 可用时以 icon rect 定位，否则 cursor fallback；所有结果夹紧 work area。
- left click、right click、ESC、deactivate 行为符合设计。
- CI publish 资源断言和 placement tests 通过。

## Required tests

- Windows CI `dotnet test`、`dotnet publish`、资源存在断言。
- `rtk git diff --check`。
- Windows 人工：任务栏四边、100%/150% DPI、双屏边缘。

## Escalation triggers

- 无已安装工具可确定性生成 PNG/ICO。
- WPF 原生资源无法满足 SVG 质量且需要第三方库。
- NotifyIcon API 无法可靠区分按钮或定位。

## Do not change

- Rust/parser、limits provider、settings/about、macOS SVG。

