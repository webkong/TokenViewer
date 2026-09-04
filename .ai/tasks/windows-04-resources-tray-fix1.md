# Task: windows-04-resources-tray-fix1

## Goal

完成托盘精确锚点、多显示器 DPI 定位、共享同步状态与快捷 tab 路由。

## Required changes

完整按 `/Users/wangsw/orca/workspaces/tokenviewer/windows-ui-port/.ai/reviews/task_f63ad51e6c43.md` 的 Required changes 1–6 修复。

## Key constraints

- `Shell_NotifyIconGetRect` 必须 best-effort；API/反射失败不能崩溃，回退 cursor。
- anchor、monitor work area 的单位必须统一为 WPF DIP；覆盖负 monitor origin 和 150% DPI。
- Popover/Main/Tray 继续共享同一 ShellViewModel/SyncCoordinator。
- 不新增 NuGet，不改 Rust/limits/settings 内容。
- 写回 `.ai/handoffs/task_f63ad51e6c43-implementation.md` 并发送 worker_done。
