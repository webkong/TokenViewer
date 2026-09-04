# Task: windows-02-data-layer

## Goal

建立 Windows Usage 功能的共享数据层：完整 FFI 查询模型、DST 正确的 AppTime、动态 EN/ZH Localization、串行 SyncCoordinator 与可取消的 UsageViewModel，并提供可在 Windows CI 运行的测试。

## Current behavior

Windows 仅有 summary/status/sync，`MainViewModel` 同时承担 summary 和同步；没有 daily/hourly/model/heatmap/rebuild、AppTime、Localization、共享刷新或测试项目。

## Desired behavior

- CoreBridge 包装四个查询和 rebuild，snake_case 字段可靠映射。
- `SyncCoordinator` 独占单 Rust handle 的 sync/rebuild/query gate，并发布统一状态/完成事件。
- `UsageViewModel` 支持 Today/Yesterday/Week/Month/All/Custom、单日 hourly、默认范围、面板四卡及 stale result 防护。
- Localization 切换语言后已打开 ViewModel/View 立即收到通知。
- 建立不触碰真实数据库的 Windows 单元测试项目。

## Reuse evidence

- 数据形状/范围/刷新 token：`macos/TokenViewer/ViewModels/UsageViewModel.swift`。
- 时间语义：`macos/TokenViewer/Services/AppTime.swift`。
- FFI 生命周期：`windows/TokenViewer/Interop/CoreBridge.cs::Call` 与 `macos/TokenViewer/Bridge/CoreBridge.swift`。
- Dispatcher/命令：`windows/TokenViewer/Infrastructure/ObservableObject.cs`、`AsyncRelayCommand.cs`。

## File-level patch plan

| 文件 | 允许修改 | 预期改动 | 禁止改动 |
|---|---|---|---|
| `windows/TokenViewer/Interop/CoreBridge.cs` | yes | 四查询、rebuild、统一 JSON/串行调用 seam | 改 Rust ABI |
| `windows/TokenViewer/Models/UsageModels.cs` | yes | DailyPoint/ModelEntry/HeatmapPoint/PanelCard/SyncResult | 改字段语义 |
| `windows/TokenViewer/Services/AppTime.cs` | add | local day→UTC range helper，时区可注入 | 用 UTC day 直接分桶 |
| `windows/TokenViewer/Services/Localization.cs` | add | EN/ZH catalog、索引绑定、通知 | 硬编码 UI 文案 |
| `windows/TokenViewer/Services/SyncCoordinator.cs` | add | shared gate、状态、sync/rebuild、完成通知 | 并发进入 handle |
| `windows/TokenViewer/ViewModels/UsageViewModel.cs` | add | 范围、查询、stale-result、面板卡 | 绘图/UI 控件 |
| `windows/TokenViewer/ViewModels/MainViewModel.cs` | yes | 只保留 shell/Agent status，与 coordinator 协作 | 第二套 usage summary |
| `windows/TokenViewer/ViewModels/ShellViewModel.cs` | yes | 构造并共享 coordinator/usage/localization | limits 重写 |
| `windows/TokenViewer/App.xaml.cs` | yes | 初始化失败处理和依赖注入 | tray UI 实现 |
| `windows/TokenViewer/TokenViewer.csproj` | yes | 测试可见性或必要编译项 | 新 UI/SQLite 包 |
| `windows/TokenViewer.Tests/TokenViewer.Tests.csproj` | add | .NET 8 Windows test project | 第三方 UI 框架 |
| `windows/TokenViewer.Tests/AppTimeTests.cs` | add | DST/范围边界测试 | 访问真实时区状态 |
| `windows/TokenViewer.Tests/CoreBridgeContractTests.cs` | add | fixture JSON 映射测试 | 调用真实用户 DB |
| `windows/TokenViewer.Tests/SyncCoordinatorTests.cs` | add | gate/通知/失败测试 | 并发写共享 DB |
| `windows/TokenViewer.Tests/UsageViewModelTests.cs` | add | range、stale、空/单点测试 | UI snapshot |
| `.github/workflows/windows-build.yml` | yes | 执行 dotnet test | 自动发布 |

## Change budget

- 允许修改/新增文件：仅上表。
- 允许新增依赖：仅测试 SDK/框架若 .NET 无法零依赖测试；选型前通过 Orca `ask` 报告准确包和理由。生产依赖 none。
- 公共 FFI/数据库 schema：no。
- 移动/重命名：no。

## Invariants

- from inclusive / to exclusive；本地日历边界转换成 UTC ISO。
- Rust 返回字符串总在 finally 中释放。
- query 不阻塞 Dispatcher；同一 handle 不并发进入。
- 现有自动同步频率设置继续生效。

## Acceptance criteria

- AppTime 在普通日和 DST 日产生正确的 UTC 边界。
- 快速 Week→Month→Week 只发布最后一次结果。
- tray/main 后续调用同一个 coordinator 时可共享完成事件。
- JSON fixture 的所有 snake_case 字段反序列化正确。
- 零 handle 初始化显示可重试错误，不伪装 Ready。

## Required tests

- `dotnet test windows/TokenViewer.Tests/TokenViewer.Tests.csproj -c Release -p:EnableWindowsTargeting=true`（Windows CI）。
- `dotnet build windows/TokenViewer/TokenViewer.csproj -c Release -p:EnableWindowsTargeting=true`（Windows CI）。
- `rtk git diff --check`。

## Escalation triggers

- 必须引入生产 NuGet 包。
- 需要改变 Rust JSON 字段或数据库查询。
- 测试 seam 要求大范围重构 CoreBridge。

## Do not change

- XAML dashboard、tray、limits provider、settings/about 和资源。

