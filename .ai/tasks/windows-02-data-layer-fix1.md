# Task: windows-02-data-layer-fix1

## Goal

修正共享数据层 Review 发现的重复刷新、缺失默认范围、不完整 L10n、竞态测试和构建产物污染。

## References

- 原任务：`task_a938286e016b` / `.ai/tasks/windows-02-data-layer.md`
- Review：`/Users/wangsw/orca/workspaces/tokenviewer/windows-ui-port/.ai/reviews/task_a938286e016b.md`
- macOS L10n source：`macos/TokenViewer/Services/Localization.swift`

## Reuse evidence

- 默认范围与 generation token：`macos/TokenViewer/ViewModels/UsageViewModel.swift::refresh`。
- L10n 静态/格式函数：`macos/TokenViewer/Services/Localization.swift`；Skills-only 文案可延后，但 Usage/Limits/Settings/About/Update/tray/error/confirm 域必须完整。
- 单一刷新：当前 `UsageViewModel` 自订阅 SyncCompleted；`ShellViewModel` 只需刷新 Agents。

## File-level patch plan

| 文件 | 允许修改 | 预期改动 | 禁止改动 |
|---|---|---|---|
| `windows/TokenViewer/ViewModels/UsageViewModel.cs` | yes | 一次性默认范围、保持用户选择、单一 sync refresh | UI 绘制 |
| `windows/TokenViewer/ViewModels/ShellViewModel.cs` | yes | 去除重复 Usage refresh | 改 limits/settings 业务 |
| `windows/TokenViewer/Services/Localization.cs` | yes | 完整非 Skills EN/ZH catalog + 格式方法 | 新资源框架 |
| `windows/TokenViewer.Tests/UsageViewModelTests.cs` | yes | 默认选择与确定性 stale 测试 | 真实 DB |
| `windows/TokenViewer.Tests/CoreBridgeContractTests.cs` | yes，仅必要时 | 不改既有契约；无需改则保持 | 降低断言 |
| `windows/TokenViewer.Tests/LocalizationTests.cs` | add | EN/ZH parity/关键域/格式方法 | XAML 扫描（阶段 3） |
| `.gitignore` | yes | 仅添加 `windows/**/bin/`、`windows/**/obj/` | 其他 ignore 规则 |
| `.ai/handoffs/task_a938286e016b-implementation.md` | yes | 更新修复和验证证据 | 隐瞒未运行 tests |

## Change budget

- 仅允许上表文件；可删除未跟踪 `windows/**/bin`、`windows/**/obj` 生成物。
- 不允许新依赖、公共 FFI/schema、移动/重命名。

## Invariants

- 一次 sync completion 只触发一轮 Usage + panel cards 查询。
- 默认范围只在首次自动决策且用户尚未主动选范围时应用。
- 已批准的 xUnit 和基线 compile 修复保留，不扩大。
- L10n language change 继续通知 `Item[]`；不得硬编码将来 UI 所需文案。

## Acceptance criteria

- 同步测试断言每个 Usage 查询只调用一次。
- 首次 Today summary 有 token → Today；零 token → Yesterday；异步期间用户选择 Week → 保留 Week。
- stale-result 测试无 Queue 数据竞争并显式等待首请求进入。
- L10n tests 覆盖 Usage/Limits/Settings/About/Update/tray/error/confirm key 且 EN/ZH 均非空，格式方法输出含参数。
- `git status --untracked-files=all` 不再出现任何 bin/obj 文件。

## Required tests

- `dotnet build windows/TokenViewer/TokenViewer.csproj -c Release -p:EnableWindowsTargeting=true`。
- `dotnet build windows/TokenViewer.Tests/TokenViewer.Tests.csproj -c Release -p:EnableWindowsTargeting=true`。
- 若 macOS 仍不能运行 Windows testhost，在 handoff 如实记录；Windows CI 执行 `dotnet test`。
- `rtk git diff --check`。

## Escalation triggers

- 完整非 Skills catalog 无法在单文件/类型安全方法内表达。
- 修复要求修改列表外生产文件或新增依赖。

## Do not change

- Rust、XAML、tray、limits provider、settings/about 实现及其他阶段文件。

