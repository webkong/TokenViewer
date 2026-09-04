# Task: windows-04-resources-tray-fix3

## Goal

修正 Shell_NotifyIconGetRect HRESULT 成功判断。

## Required changes

1. 仅修改 `TrayController.cs`、`TrayPlacementTests.cs` 与 phase-4 handoff。
2. `Shell_NotifyIconGetRect` 仅在 HRESULT 成功（S_OK=0）时返回 rect；失败返回 null。
3. 抽出最小纯 HRESULT 判定并测试 0 成功、非 0 失败。
4. 重跑两个 build 与 `git diff --check`，发送 worker_done。
