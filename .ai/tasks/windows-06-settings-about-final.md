# Task: windows-06-settings-about-final

## Goal

完成 Settings/About parity、数据重建与设置重置闭环，并收紧最终 Windows CI/发布断言，形成可人工验收的候选版本。

## Current behavior

Settings 只有 theme/language/sync/startup 基础控件；没有 sidebar、tray sections/Agent visibility、currency、rebuild/reset；没有 AboutView。更新 UI 混在 Settings。

## Desired behavior

General/Appearance/Menu Bar/Data 分区完整，设置即时持久化并驱动现有服务；rebuild 走 SyncCoordinator；reset settings 不删 usage；About 展示版本、supported agents、更新卡和链接；最终 CI 覆盖测试、publish 和资源断言。

## Reuse evidence

- 设置结构/默认值/确认：`macos/TokenViewer/Views/SettingsView.swift`。
- About：`macos/TokenViewer/Views/AboutView.swift`。
- Windows persistence/startup/update：`SettingsStore.cs`、`LaunchAtStartupManager.cs`、`UpdateService.cs`、相关 ViewModels。
- Agent registry/resources：任务 04；sync/rebuild：任务 02。

## UI evidence packet

- 相邻页面：macOS SettingsView/AboutView；Windows Usage/Limits cards。
- 复用组件：Localization、AgentRegistry、SettingsStore、UpdateViewModel、shared Card/buttons。
- token：沿用 App.xaml 共享 tokens。
- 布局：Settings 左侧 section 导航、右侧可滚动内容；About 卡片和可展开 badge grid。
- 状态矩阵：default/saving/rebuilding/reset-confirm/update-checking/update-available/update-error/supported-expanded。
- 响应式：1240×860；1000×700 sidebar 保持可用或降级为顶部 section selector。
- 视觉证据：macOS 源码结构和 Windows artifact 截图。

## File-level patch plan

| 文件 | 允许修改 | 预期改动 | 禁止改动 |
|---|---|---|---|
| `windows/TokenViewer/Models/AppSettings.cs` | yes | currency/tray/panel sections/visibility defaults | 改旧 key 无迁移 |
| `windows/TokenViewer/Services/SettingsStore.cs` | yes | 兼容加载/重置默认值 | 删除 usage DB |
| `windows/TokenViewer/ViewModels/SettingsViewModel.cs` | yes | 全设置、commands、即时 side effects | 直接调用 Rust handle |
| `windows/TokenViewer/Views/SettingsView.xaml` | add | sidebar sections/确认 UI | 硬编码文案 |
| `windows/TokenViewer/Views/SettingsView.xaml.cs` | add | 最小确认/navigation glue | 业务数据访问 |
| `windows/TokenViewer/Views/AboutView.xaml` | add | app/update/agents/links | skills UI |
| `windows/TokenViewer/Views/AboutView.xaml.cs` | add | link/expand 最小行为 | 自建更新服务 |
| `windows/TokenViewer/MainWindow.xaml` | yes | 接入 Settings/About tabs | 重写 Usage/Limits |
| `windows/TokenViewer/ViewModels/ShellViewModel.cs` | yes | 最终依赖连线 | 第二实例服务 |
| `windows/TokenViewer/Services/TrayController.cs` | yes | show tray/panel setting即时生效 | 改托盘基础行为 |
| `windows/TokenViewer.Tests/SettingsTests.cs` | add | defaults/migration/reset/no-data-delete | 真实 startup 注册表写入 |
| `windows/TokenViewer.Tests/LocalizationCoverageTests.cs` | yes | 覆盖最终 XAML | 放宽硬编码规则 |
| `.github/workflows/windows-build.yml` | yes | 最终 tests/build/publish/资源断言 | 自动 release |
| `script/windows-release.ps1` | yes，仅必要时 | 保证 zip 完整 | 改现有发布约定 |

## Change budget

- 允许文件：仅上表。
- 新依赖：none。
- 不修改 Rust/schema/版本号/发布链接。

## Invariants

- reset settings 不删除 `~/.tokenviewer/data.db`；rebuild 才清 usage/cursors 并重新解析。
- 隐藏 tray 时主窗口必须仍可访问；本次运行即时生效。
- 旧 settings JSON 缺新字段时使用默认值，不丢旧值。
- 所有链接使用既有 GitHub/website/update service 常量。

## Acceptance criteria

- Settings 四分区、About 所有模块在 EN/ZH 可用。
- rebuild/setting reset 均确认、运行中禁用、结果可见；行为边界正确。
- 旧 settings fixture 迁移后保留 theme/language/sync/startup。
- 最终 CI tests/build/publish 全绿，zip 含 DLL/EXE/ICO/brand assets。
- 全部规划验收项有测试或人工 checklist 证据。

## Required tests

- Windows CI：Rust tests、全部 .NET tests、Release build/publish、artifact assertions。
- `rtk git diff --check`。
- 候选 diff 凭据扫描。
- 人工 artifact 全流程验收，记录未能验证项。

## Escalation triggers

- 需要改版本/发布自动化或网站链接。
- settings migration 需要破坏旧格式。
- 更新安装行为需要新的安全权限。

## Do not change

- Skills UI、WebView2、云服务、发布版本、macOS 实现和无关代码。

