# Task: windows-06-settings-about-final-fix2

## Goal

完成最终阶段遗留的 15-Agent 可见性 UI、popover Limits 区与稳定持久化。

## Required changes

完整执行 `/Users/wangsw/orca/workspaces/tokenviewer/windows-ui-port/.ai/reviews/task_78c3a93efca5.md` 中 `Fix 1 Review / Remaining required changes` 1–6。

## Constraints

- 仅修改 task 06/fix1 已允许文件与相应测试/handoff。
- 复用共享 `Shell.Limits`，不增加网络调用、第二个 timer 或第二个服务实例。
- visibility 只影响 tray popover；主 Limits 页面始终保留全部 15。
- 不新增依赖，不提交、不推送；完成后发送 `worker_done`。
