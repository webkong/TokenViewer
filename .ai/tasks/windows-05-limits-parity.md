# Task: windows-05-limits-parity

## Goal

让 Windows Limits 与 macOS 15 个 canonical Agent 对齐，提供两列卡片、倒计时/进度/空错态，并消除外部 `sqlite3.exe` 运行时依赖。

## Current behavior

Windows 只请求 Claude、ChatGPT、Cursor、Gemini、Kiro、Kimi、Antigravity；UI 是单列文字列表；Cursor 通过 PATH 上的 sqlite3 读取 state DB。

## Desired behavior

实现 Claude Code、ChatGPT/codex、Cursor、Kiro、Copilot、Kimi、Antigravity、Zed、Trae、Windsurf、Qoder、CodeBuddy、WorkBuddy、Gemini、ZCode；失败降级为 install/status；无外部 sqlite3 依赖。

## Reuse evidence

- provider 逻辑：`macos/TokenViewer/Services/LimitsService.swift::fetchAll/fetch*`。
- 卡片与倒计时：`macos/TokenViewer/Views/LimitsView.swift`。
- Windows HTTP/JSON helpers：`windows/TokenViewer/Services/LimitsService.cs`。
- registry/resources：任务 04 `AgentRegistry.cs` 与 brand PNG。

## UI evidence packet

- 相邻页面：macOS LimitsView、Windows UsageView cards。
- 复用组件：AgentRegistry、Localization、现有 AgentLimit/LimitWindow、shared brushes。
- token：Agent brand tint、Card、MutedText、progress background；不引入新色系。
- 布局：宽屏两列，窄于 1000 单列；active 在前、inactive 分区。
- 状态矩阵：loading/configured-unavailable/configured-with-windows/inactive/error/empty。
- 响应式：1240×860 两列，1000×700 单列可滚动。
- 视觉证据：macOS LimitsView 结构；Windows artifact 人工验证。

## File-level patch plan

| 文件 | 允许修改 | 预期改动 | 禁止改动 |
|---|---|---|---|
| `windows/TokenViewer/Services/LimitsService.cs` | yes | 八个缺失 fetch、无外部 sqlite3、Windows 路径 | 改上游 API 语义 |
| `windows/TokenViewer/Models/LimitsModels.cs` | yes | expiry/subscription/quota reset/countdown fields | 不兼容重命名 source |
| `windows/TokenViewer/ViewModels/LimitsViewModel.cs` | yes | active/inactive、倒计时 timer、localization | 独立 registry |
| `windows/TokenViewer/Views/LimitsView.xaml` | add | 两列卡片/进度/空错态 | 硬编码文案 |
| `windows/TokenViewer/Views/LimitsView.xaml.cs` | add | 最小布局 glue | HTTP/凭据读取 |
| `windows/TokenViewer/MainWindow.xaml` | yes | 接入 LimitsView | 改 Usage/Settings 内容 |
| `core/src/ffi.rs` | yes，仅 SQLite helper 方案需要时 | 窄范围只读 account-cache helper | 暴露任意 SQL/凭据 |
| `windows/TokenViewer/Interop/CoreBridge.cs` | yes，仅 helper 方案需要时 | 包装窄范围 helper | 通用 SQL API |
| `windows/TokenViewer.Tests/LimitsContractTests.cs` | add | 15 source、fixture、降级/倒计时 | 真实网络/真实凭据 |

## Change budget

- 允许文件：仅上表。
- 新生产依赖：none；若无法无依赖读取 SQLite，必须先 `ask` 比较窄 Rust FFI 与 NuGet 方案。
- 不改变数据库 schema、现有 usage FFI。

## Invariants

- 测试禁止真实网络和真实用户凭据。
- 错误信息不泄漏 token/cookie/credentials 内容。
- `codex` 内部 ID、ChatGPT 用户显示名保持约定。
- ZCode 仅安装/plan 信息，不伪造 quota。

## Acceptance criteria

- FetchAll 始终包含 15 个 canonical source，单个 provider 失败不使整体失败。
- Cursor 在未安装 sqlite3.exe 的机器仍可读取或安全降级。
- reset countdown 每分钟更新且窗口关闭后不泄漏 timer。
- 卡片在两种目标窗口尺寸满足布局与本地化。
- fixture tests 不访问网络/真实 profile。

## Required tests

- Windows CI `dotnet test`、`dotnet build`。
- `rtk cargo test --lib --tests`（若修改 Rust）。
- `rtk git diff --check`。
- 人工验证 15 卡片及 active/inactive/error。

## Escalation triggers

- 上游 API/本地凭据格式已变化，不能按 macOS 实现复用。
- 需要生产 NuGet SQLite 包或通用 SQL FFI。
- 测试需要真实账号/网络。

## Do not change

- Usage dashboard、tray、settings/about、skills UI。

