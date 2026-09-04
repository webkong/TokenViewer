# Task: windows-04-resources-tray-fix2

## Goal

让 .NET 8 WinForms NotifyIcon 的 `Shell_NotifyIconGetRect` 路径实际可达。

## Required changes

1. 修改 `TrayController.cs`：从 .NET 8 的 `_window` / `_id` 取得 HWND/uID，NOTIFYICONIDENTIFIER 使用 `Guid.Empty`；不再要求不存在的 `_guid` 字段。
2. 在 `TrayPlacementTests.cs` 或最小新增测试中验证一个已创建/visible 的 NotifyIcon identity 可取得非零 HWND/uID；测试须 STA 安全并清理资源，不能新增 NuGet。
3. API、反射或平台失败仍返回 null，cursor fallback 不变。
4. 更新 handoff，重跑两个 build 与 `git diff --check`，发送 worker_done。

## Allowed files

- `windows/TokenViewer/Services/TrayController.cs`
- `windows/TokenViewer.Tests/TrayPlacementTests.cs`
- `.ai/handoffs/task_f63ad51e6c43-implementation.md`
